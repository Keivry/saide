// SPDX-License-Identifier: MIT OR Apache-2.0

//! Parsing of device-to-client messages received from the scrcpy server on
//! the control channel.
//!
//! The control channel is a bidirectional TCP socket.  Client→server messages
//! ([`ControlMessage`]) are handled by the `scrcpy_protocol` crate, while
//! server→client messages (this module) must be parsed by SAide itself.
//!
//! # Wire format
//!
//! Each message starts with a single type byte, followed by a type-specific
//! payload.  Multi-byte integers are encoded in network byte order
//! (big-endian).
//!
//! | Type | Name               | Payload                                   |
//! |------|--------------------|-------------------------------------------|
//! | 0    | Clipboard          | 4 B length (BE) + UTF-8 text              |
//! | 1    | AckClipboard       | 8 B sequence number (BE)                  |
//! | 2    | UhidOutput         | 2 B id (BE) + 2 B data len (BE) + data    |
//! | 3    | RotationChanged    | 4 B rotation (BE): 0, 90, 180, or 270     |
//! | 4    | ImeStateChanged    | 1 B visible (0 = hidden, non‑zero = vis.) |

use {
    crate::error::{Result, SAideError},
    std::io::{ErrorKind, Read},
};

/// Device message types received from the scrcpy server on the control
/// channel.
#[derive(Debug, Clone, PartialEq)]
pub enum DeviceMessage {
    /// Clipboard content from device (type 0).
    Clipboard(String),
    /// Acknowledge clipboard set, carrying a monotonic sequence number
    /// (type 1).
    AckClipboard(u64),
    /// UHID output report forwarded from the kernel (type 2).
    UhidOutput {
        /// HID device identifier.
        id: u16,
        /// Raw HID report bytes.
        data: Vec<u8>,
    },
    /// Screen rotation changed (type 3).
    ///
    /// The value is one of 0, 90, 180, or 270 degrees.
    RotationChanged(u32),
    /// IME keyboard visibility changed (type 4).
    ImeStateChanged(bool),
}

impl DeviceMessage {
    /// Try to read and parse a single device message from the given reader.
    ///
    /// Returns `Ok(None)` when the underlying source has no data available
    /// (i.e. `WouldBlock` on a non-blocking stream or `UnexpectedEof` on a
    /// blocking one that has been cleanly shut down).
    pub fn read_from<R: Read>(reader: &mut R) -> Result<Option<Self>> {
        // --- type byte ---
        let mut type_buf = [0u8; 1];
        match reader.read_exact(&mut type_buf) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::WouldBlock => return Ok(None),
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(SAideError::from(e)),
        }
        let msg_type = type_buf[0];

        match msg_type {
            0 => {
                // CLIPBOARD: length(u32 BE) + UTF-8 text
                let mut len_buf = [0u8; 4];
                reader.read_exact(&mut len_buf)?;
                let len = u32::from_be_bytes(len_buf) as usize;
                let mut text_buf = vec![0u8; len];
                reader.read_exact(&mut text_buf)?;
                let text = String::from_utf8(text_buf).map_err(|e| {
                    SAideError::ProtocolError(format!(
                        "invalid UTF-8 in device clipboard message: {e}"
                    ))
                })?;
                Ok(Some(DeviceMessage::Clipboard(text)))
            }
            1 => {
                // ACK_CLIPBOARD: sequence(u64 BE)
                let mut seq_buf = [0u8; 8];
                reader.read_exact(&mut seq_buf)?;
                Ok(Some(DeviceMessage::AckClipboard(u64::from_be_bytes(
                    seq_buf,
                ))))
            }
            2 => {
                // UHID_OUTPUT: id(u16 BE) + len(u16 BE) + data
                let mut header = [0u8; 4];
                reader.read_exact(&mut header)?;
                let id = u16::from_be_bytes([header[0], header[1]]);
                let len = u16::from_be_bytes([header[2], header[3]]) as usize;
                let mut data = vec![0u8; len];
                reader.read_exact(&mut data)?;
                Ok(Some(DeviceMessage::UhidOutput { id, data }))
            }
            3 => {
                // ROTATION_CHANGED: rotation(u32 BE)
                let mut rot_buf = [0u8; 4];
                reader.read_exact(&mut rot_buf)?;
                Ok(Some(DeviceMessage::RotationChanged(u32::from_be_bytes(
                    rot_buf,
                ))))
            }
            4 => {
                // IME_STATE_CHANGED: visible(bool, 1 byte)
                let mut vis_buf = [0u8; 1];
                reader.read_exact(&mut vis_buf)?;
                Ok(Some(DeviceMessage::ImeStateChanged(vis_buf[0] != 0)))
            }
            _ => Err(SAideError::ProtocolError(format!(
                "unknown device message type: {msg_type}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use {super::*, std::io::Cursor};

    #[test]
    fn parse_clipboard_message() {
        let text = "hello clipboard";
        let mut buf = Vec::new();
        buf.push(0); // type
        buf.extend_from_slice(&(text.len() as u32).to_be_bytes());
        buf.extend_from_slice(text.as_bytes());

        let msg = DeviceMessage::read_from(&mut Cursor::new(buf))
            .unwrap()
            .unwrap();
        assert_eq!(msg, DeviceMessage::Clipboard("hello clipboard".into()));
    }

    #[test]
    fn parse_ack_clipboard() {
        let mut buf = Vec::new();
        buf.push(1); // type
        buf.extend_from_slice(&42u64.to_be_bytes());

        let msg = DeviceMessage::read_from(&mut Cursor::new(buf))
            .unwrap()
            .unwrap();
        assert_eq!(msg, DeviceMessage::AckClipboard(42));
    }

    #[test]
    fn parse_uhid_output() {
        let data = vec![0xAA, 0xBB, 0xCC];
        let mut buf = Vec::new();
        buf.push(2); // type
        buf.extend_from_slice(&1u16.to_be_bytes()); // id
        buf.extend_from_slice(&(data.len() as u16).to_be_bytes());
        buf.extend_from_slice(&data);

        let msg = DeviceMessage::read_from(&mut Cursor::new(buf))
            .unwrap()
            .unwrap();
        assert_eq!(
            msg,
            DeviceMessage::UhidOutput {
                id: 1,
                data: vec![0xAA, 0xBB, 0xCC]
            }
        );
    }

    #[test]
    fn parse_rotation_changed() {
        let mut buf = Vec::new();
        buf.push(3); // type
        buf.extend_from_slice(&90u32.to_be_bytes());

        let msg = DeviceMessage::read_from(&mut Cursor::new(buf))
            .unwrap()
            .unwrap();
        assert_eq!(msg, DeviceMessage::RotationChanged(90));
    }

    #[test]
    fn parse_ime_state_changed() {
        let mut buf = Vec::new();
        buf.push(4); // type
        buf.push(1); // visible = true

        let msg = DeviceMessage::read_from(&mut Cursor::new(buf))
            .unwrap()
            .unwrap();
        assert_eq!(msg, DeviceMessage::ImeStateChanged(true));

        let mut buf = Vec::new();
        buf.push(4);
        buf.push(0); // visible = false
        let msg = DeviceMessage::read_from(&mut Cursor::new(buf))
            .unwrap()
            .unwrap();
        assert_eq!(msg, DeviceMessage::ImeStateChanged(false));
    }

    #[test]
    fn empty_reader_returns_none() {
        let msg = DeviceMessage::read_from(&mut Cursor::new([])).unwrap();
        assert!(msg.is_none());
    }

    #[test]
    fn truncated_payload_is_io_error() {
        // Type byte present but payload truncated → genuine I/O error
        let buf = vec![0u8]; // type byte only, no length follows
        let result = DeviceMessage::read_from(&mut Cursor::new(buf));
        assert!(result.is_err());
        match result {
            Err(SAideError::IoError(_)) => {}
            other => panic!("expected IoError, got {other:?}"),
        }
    }

    #[test]
    fn invalid_utf8_clipboard_is_error() {
        let mut buf = Vec::new();
        buf.push(0); // type
        buf.extend_from_slice(&3u32.to_be_bytes());
        // Invalid UTF-8: 0xFF 0xFE 0xFD
        buf.extend_from_slice(&[0xFF, 0xFE, 0xFD]);

        let result = DeviceMessage::read_from(&mut Cursor::new(buf));
        assert!(result.is_err());
        match result {
            Err(SAideError::ProtocolError(msg)) => {
                assert!(msg.contains("invalid UTF-8"));
            }
            _ => panic!("expected ProtocolError"),
        }
    }

    #[test]
    fn unknown_type_is_error() {
        let mut buf = vec![99u8]; // unknown type
        // Add enough bytes for the largest possible payload so read_exact
        // won't fail with UnexpectedEof before matching the unknown type.
        buf.extend_from_slice(&[0u8; 8]);

        let result = DeviceMessage::read_from(&mut Cursor::new(buf));
        assert!(result.is_err());
        match result {
            Err(SAideError::ProtocolError(msg)) => {
                assert!(msg.contains("unknown device message type"));
            }
            _ => panic!("expected ProtocolError"),
        }
    }
}
