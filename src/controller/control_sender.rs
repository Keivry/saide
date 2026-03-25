// SPDX-License-Identifier: MIT OR Apache-2.0

pub use scrcpy::ControlSender;

#[cfg(test)]
mod tests {
    use {
        super::*,
        std::{
            io::Read,
            net::{TcpListener, TcpStream},
            thread,
            time::Duration,
        },
    };

    fn setup_mock_server() -> (TcpStream, thread::JoinHandle<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut all_data = Vec::new();
            let mut buf = [0u8; 1024];

            // Read all data with timeout
            stream
                .set_read_timeout(Some(Duration::from_millis(500)))
                .unwrap();

            while let Ok(n) = stream.read(&mut buf) {
                if n == 0 {
                    break;
                }
                all_data.extend_from_slice(&buf[..n]);
            }
            all_data
        });

        thread::sleep(Duration::from_millis(10)); // Give server time to start
        let stream = TcpStream::connect(addr).unwrap();
        (stream, handle)
    }

    #[test]
    fn test_send_touch_events() {
        let (stream, handle) = setup_mock_server();
        let sender = ControlSender::new(stream, 1080, 2340);

        sender.send_touch_down(100, 200).unwrap();
        sender.send_touch_move(150, 250).unwrap();
        sender.send_touch_up(150, 250).unwrap();

        drop(sender); // Close connection
        let data = handle.join().unwrap();

        // Each touch event is 32 bytes, should have 3 events = 96 bytes
        assert_eq!(data.len(), 96, "Should have 96 bytes (3 x 32-byte events)");

        // First message should be touch down (type 2, action 0)
        assert_eq!(data[0], 2, "First message should be touch event");
        assert_eq!(data[1], 0, "First message should be DOWN action");

        // Second message at offset 32 should be touch move (type 2, action 2)
        assert_eq!(data[32], 2, "Second message should be touch event");
        assert_eq!(data[33], 2, "Second message should be MOVE action");

        // Third message at offset 64 should be touch up (type 2, action 1)
        assert_eq!(data[64], 2, "Third message should be touch event");
        assert_eq!(data[65], 1, "Third message should be UP action");
    }

    #[test]
    fn test_send_key_events() {
        let (stream, handle) = setup_mock_server();
        let sender = ControlSender::new(stream, 1080, 2340);

        sender.send_key_press(66, 0).unwrap(); // KEYCODE_ENTER

        drop(sender);
        let data = handle.join().unwrap();

        // Each key event is 14 bytes, should have 2 events (down + up) = 28 bytes
        assert_eq!(data.len(), 28, "Should have 28 bytes (2 x 14-byte events)");

        // First message: key down (type 0, action 0)
        assert_eq!(data[0], 0, "First message should be INJECT_KEYCODE");
        assert_eq!(data[1], 0, "First message should be DOWN action");

        // Second message: key up (type 0, action 1)
        assert_eq!(data[14], 0, "Second message should be INJECT_KEYCODE");
        assert_eq!(data[15], 1, "Second message should be UP action");
    }

    #[test]
    fn test_send_text() {
        let (stream, handle) = setup_mock_server();
        let sender = ControlSender::new(stream, 1080, 2340);

        sender.send_text("Hello").unwrap();

        drop(sender);
        let data = handle.join().unwrap();

        assert!(!data.is_empty(), "Should have received data");
        assert_eq!(data[0], 1, "Should be INJECT_TEXT");

        // Text format: [type:1][length:4][text:N]
        let text_len = u32::from_be_bytes([data[1], data[2], data[3], data[4]]);
        assert_eq!(text_len, 5, "Text length should be 5");

        // Verify text content
        let text = String::from_utf8_lossy(&data[5..10]);
        assert_eq!(text, "Hello", "Text content should be 'Hello'");
    }

    #[test]
    fn test_update_screen_size() {
        let (stream, _handle) = setup_mock_server();
        let sender = ControlSender::new(stream, 1080, 2340);

        assert_eq!(sender.get_screen_size(), (1080, 2340));

        sender.update_screen_size(2340, 1080);
        assert_eq!(sender.get_screen_size(), (2340, 1080));
    }
}
