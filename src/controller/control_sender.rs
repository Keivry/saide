//! Scrcpy Control Channel Sender
//!
//! Encapsulates control stream and provides type-safe methods for sending
//! input events to Android device via scrcpy protocol.

use {
    crate::{error::Result, scrcpy::protocol::control::ControlMessage},
    parking_lot::Mutex,
    std::{io::Write, net::TcpStream, sync::Arc},
    tracing::trace,
};

/// Shared control channel sender
///
/// This struct wraps a TcpStream and provides methods to send control messages.
/// Multiple instances can share the same underlying stream via Arc.
#[derive(Clone)]
pub struct ControlSender {
    /// Shared control stream (locked for concurrent writes)
    stream: Arc<Mutex<TcpStream>>,
    /// Current screen dimensions
    screen_size: Arc<Mutex<(u16, u16)>>,
}

impl ControlSender {
    /// Create a new control sender
    pub fn new(stream: TcpStream, screen_width: u16, screen_height: u16) -> Self {
        Self {
            stream: Arc::new(Mutex::new(stream)),
            screen_size: Arc::new(Mutex::new((screen_width, screen_height))),
        }
    }

    /// Update screen dimensions (e.g., after rotation or resolution change)
    pub fn update_screen_size(&self, width: u16, height: u16) {
        *self.screen_size.lock() = (width, height);
        trace!("Updated control sender screen size to {}x{}", width, height);
    }

    /// Get current screen dimensions
    pub fn get_screen_size(&self) -> (u16, u16) { *self.screen_size.lock() }

    /// Send a control message (internal helper)
    fn send_message(&self, msg: &ControlMessage) -> Result<()> {
        let mut buf = Vec::with_capacity(64);

        msg.serialize(&mut buf)?;
        trace!("Serialized message: {} bytes", buf.len());

        let mut stream = self.stream.lock();
        stream.write_all(&buf)?;
        stream.flush()?;
        trace!("Sent control message: {:?}", msg);

        Ok(())
    }

    /// Send touch down event
    pub fn send_touch_down(&self, x: u32, y: u32) -> Result<()> {
        let (width, height) = self.get_screen_size();
        let msg = ControlMessage::touch_down(x, y, width, height);
        self.send_message(&msg)
    }

    /// Send touch up event
    pub fn send_touch_up(&self, x: u32, y: u32) -> Result<()> {
        let (width, height) = self.get_screen_size();
        let msg = ControlMessage::touch_up(x, y, width, height);
        self.send_message(&msg)
    }

    /// Send touch move event
    pub fn send_touch_move(&self, x: u32, y: u32) -> Result<()> {
        let (width, height) = self.get_screen_size();
        let msg = ControlMessage::touch_move(x, y, width, height);
        self.send_message(&msg)
    }

    /// Send key down event
    pub fn send_key_down(&self, keycode: u32, metastate: u32) -> Result<()> {
        let msg = ControlMessage::key_down(keycode, metastate);
        self.send_message(&msg)
    }

    /// Send key up event
    pub fn send_key_up(&self, keycode: u32, metastate: u32) -> Result<()> {
        let msg = ControlMessage::key_up(keycode, metastate);
        self.send_message(&msg)
    }

    /// Send key press (down + up)
    pub fn send_key_press(&self, keycode: u32, metastate: u32) -> Result<()> {
        self.send_key_down(keycode, metastate)?;
        self.send_key_up(keycode, metastate)?;
        Ok(())
    }

    /// Set display power (turn screen backlight on/off)
    /// Note: This only controls the backlight (POWER_MODE_OFF/NORMAL), NOT lock the device
    /// The device remains unlocked and can still mirror/control
    pub fn send_set_display_power(&self, on: bool) -> Result<()> {
        let msg = ControlMessage::SetDisplayPower { on };
        self.send_message(&msg)
    }

    /// Turn screen off
    pub fn send_screen_off_with_brightness_save(&self) -> Result<()> {
        self.send_set_display_power(false)
    }

    /// Send text injection
    pub fn send_text(&self, text: &str) -> Result<()> {
        let msg = ControlMessage::InjectText {
            text: text.to_string(),
        };
        self.send_message(&msg)
    }

    /// Send scroll event
    pub fn send_scroll(&self, x: u32, y: u32, hscroll: f32, vscroll: f32) -> Result<()> {
        let (width, height) = self.get_screen_size();
        let msg = ControlMessage::scroll(x, y, width, height, hscroll, vscroll);
        self.send_message(&msg)
    }

    /// Send custom control message (advanced usage)
    pub fn send_custom(&self, msg: &ControlMessage) -> Result<()> { self.send_message(msg) }
}

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
