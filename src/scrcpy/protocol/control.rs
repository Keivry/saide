// SPDX-License-Identifier: MIT OR Apache-2.0

/// Scrcpy Control Protocol Implementation
///
/// Reference: scrcpy/app/src/control_msg.c
/// Protocol version: 3.3.3
use {
    byteorder::{BigEndian, WriteBytesExt},
    std::io::{Result, Write},
};

/// Control message type enumeration
/// Must match server/src/main/java/com/genymobile/scrcpy/control/ControlMessage.java
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlMessageType {
    InjectKeycode = 0,
    InjectText = 1,
    InjectTouchEvent = 2,
    InjectScrollEvent = 3,
    BackOrScreenOn = 4,
    ExpandNotificationPanel = 5,
    ExpandSettingsPanel = 6,
    CollapsePanels = 7,
    GetClipboard = 8,
    SetClipboard = 9,
    SetDisplayPower = 10,
    RotateDevice = 11,
    UhidCreate = 12,
    UhidInput = 13,
    UhidDestroy = 14,
    OpenHardKeyboardSettings = 15,
    StartApp = 16,
    ResetVideo = 17,
}

/// Android KeyEvent actions
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AndroidKeyEventAction {
    Down = 0,
    Up = 1,
}

/// Android MotionEvent actions
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AndroidMotionEventAction {
    Down = 0,
    Up = 1,
    Move = 2,
}

/// Screen position with size context
#[derive(Debug, Clone, Copy)]
pub struct Position {
    pub x: u32,
    pub y: u32,
    pub screen_width: u16,
    pub screen_height: u16,
}

impl Position {
    pub fn new(x: u32, y: u32, screen_width: u16, screen_height: u16) -> Self {
        Self {
            x,
            y,
            screen_width,
            screen_height,
        }
    }

    /// Serialize position to 12 bytes (as per control_msg.c write_position)
    fn serialize<W: Write>(&self, buf: &mut W) -> Result<()> {
        buf.write_u32::<BigEndian>(self.x)?;
        buf.write_u32::<BigEndian>(self.y)?;
        buf.write_u16::<BigEndian>(self.screen_width)?;
        buf.write_u16::<BigEndian>(self.screen_height)?;
        Ok(())
    }
}

/// Special pointer ID values (as per control_msg.h)
pub const POINTER_ID_MOUSE: u64 = u64::MAX; // -1 as signed
pub const POINTER_ID_GENERIC_FINGER: u64 = u64::MAX - 1; // -2
pub const POINTER_ID_VIRTUAL_FINGER: u64 = u64::MAX - 2; // -3

/// Control message variants
#[derive(Debug, Clone)]
pub enum ControlMessage {
    /// Inject keycode event (TYPE_INJECT_KEYCODE = 0)
    /// Total size: 14 bytes
    InjectKeycode {
        action: AndroidKeyEventAction,
        keycode: u32, // Android KeyEvent.KEYCODE_*
        repeat: u32,
        metastate: u32, // META_SHIFT_ON=1, META_CTRL_ON=4096, etc.
    },

    /// Inject text (TYPE_INJECT_TEXT = 1)
    /// Total size: 5 + text_length bytes
    InjectText {
        text: String, // UTF-8, max 300 bytes
    },

    /// Inject touch event (TYPE_INJECT_TOUCH_EVENT = 2)
    /// Total size: 32 bytes
    InjectTouchEvent {
        action: AndroidMotionEventAction,
        pointer_id: u64,
        position: Position,
        pressure: f32, // 0.0-1.0, will be converted to u16 fixed-point
        action_button: u32,
        buttons: u32,
    },

    /// Inject scroll event (TYPE_INJECT_SCROLL_EVENT = 3)
    /// Total size: 21 bytes
    InjectScrollEvent {
        position: Position,
        hscroll: f32, // -1.0 to 1.0
        vscroll: f32, // -1.0 to 1.0
        buttons: u32,
    },

    /// Back or screen on (TYPE_BACK_OR_SCREEN_ON = 4)
    /// Total size: 2 bytes
    BackOrScreenOn { action: AndroidKeyEventAction },

    /// Expand notification panel (TYPE_EXPAND_NOTIFICATION_PANEL = 5)
    /// Total size: 1 byte
    ExpandNotificationPanel,

    /// Expand settings panel (TYPE_EXPAND_SETTINGS_PANEL = 6)
    /// Total size: 1 byte
    ExpandSettingsPanel,

    /// Collapse panels (TYPE_COLLAPSE_PANELS = 7)
    /// Total size: 1 byte
    CollapsePanels,

    /// Set display power (TYPE_SET_DISPLAY_POWER = 10)
    /// Total size: 2 bytes
    SetDisplayPower { on: bool },

    /// Rotate device (TYPE_ROTATE_DEVICE = 11)
    /// Total size: 1 byte
    RotateDevice,
}

impl ControlMessage {
    /// Serialize message to binary format (as per control_msg.c sc_control_msg_serialize)
    pub fn serialize(&self, buf: &mut Vec<u8>) -> Result<usize> {
        let start_len = buf.len();

        match self {
            ControlMessage::InjectKeycode {
                action,
                keycode,
                repeat,
                metastate,
            } => {
                buf.write_u8(ControlMessageType::InjectKeycode as u8)?;
                buf.write_u8(*action as u8)?;
                buf.write_u32::<BigEndian>(*keycode)?;
                buf.write_u32::<BigEndian>(*repeat)?;
                buf.write_u32::<BigEndian>(*metastate)?;
                // Total: 14 bytes
            }

            ControlMessage::InjectText { text } => {
                buf.write_u8(ControlMessageType::InjectText as u8)?;

                // Truncate to max 300 bytes UTF-8
                let text_bytes = text.as_bytes();
                let text_len = text_bytes.len().min(300);

                buf.write_u32::<BigEndian>(text_len as u32)?;
                buf.write_all(&text_bytes[..text_len])?;
                // Total: 5 + text_len bytes
            }

            ControlMessage::InjectTouchEvent {
                action,
                pointer_id,
                position,
                pressure,
                action_button,
                buttons,
            } => {
                buf.write_u8(ControlMessageType::InjectTouchEvent as u8)?;
                buf.write_u8(*action as u8)?;
                buf.write_u64::<BigEndian>(*pointer_id)?;
                position.serialize(buf)?; // 12 bytes

                // Convert pressure (0.0-1.0) to u16 fixed-point (as per control_msg.c)
                let pressure_fp = (pressure.clamp(0.0, 1.0) * 65535.0) as u16;
                buf.write_u16::<BigEndian>(pressure_fp)?;

                buf.write_u32::<BigEndian>(*action_button)?;
                buf.write_u32::<BigEndian>(*buttons)?;
                // Total: 32 bytes
            }

            ControlMessage::InjectScrollEvent {
                position,
                hscroll,
                vscroll,
                buttons,
            } => {
                buf.write_u8(ControlMessageType::InjectScrollEvent as u8)?;
                position.serialize(buf)?; // 12 bytes

                // Normalize to [-1, 1] then convert to i16 fixed-point
                // (as per control_msg.c: accept [-16, 16], normalize to [-1, 1])
                let hscroll_norm = (hscroll / 16.0).clamp(-1.0, 1.0);
                let vscroll_norm = (vscroll / 16.0).clamp(-1.0, 1.0);
                let hscroll_fp = (hscroll_norm * 32767.0) as i16;
                let vscroll_fp = (vscroll_norm * 32767.0) as i16;

                buf.write_i16::<BigEndian>(hscroll_fp)?;
                buf.write_i16::<BigEndian>(vscroll_fp)?;
                buf.write_u32::<BigEndian>(*buttons)?;
                // Total: 21 bytes
            }

            ControlMessage::BackOrScreenOn { action } => {
                buf.write_u8(ControlMessageType::BackOrScreenOn as u8)?;
                buf.write_u8(*action as u8)?;
                // Total: 2 bytes
            }

            ControlMessage::ExpandNotificationPanel => {
                buf.write_u8(ControlMessageType::ExpandNotificationPanel as u8)?;
                // Total: 1 byte
            }

            ControlMessage::ExpandSettingsPanel => {
                buf.write_u8(ControlMessageType::ExpandSettingsPanel as u8)?;
                // Total: 1 byte
            }

            ControlMessage::CollapsePanels => {
                buf.write_u8(ControlMessageType::CollapsePanels as u8)?;
                // Total: 1 byte
            }

            ControlMessage::SetDisplayPower { on } => {
                buf.write_u8(ControlMessageType::SetDisplayPower as u8)?;
                buf.write_u8(if *on { 1 } else { 0 })?;
                // Total: 2 bytes
            }

            ControlMessage::RotateDevice => {
                buf.write_u8(ControlMessageType::RotateDevice as u8)?;
                // Total: 1 byte
            }
        }

        Ok(buf.len() - start_len)
    }

    /// Helper: Create touch down event
    pub fn touch_down(x: u32, y: u32, screen_width: u16, screen_height: u16) -> Self {
        Self::InjectTouchEvent {
            action: AndroidMotionEventAction::Down,
            pointer_id: POINTER_ID_MOUSE,
            position: Position::new(x, y, screen_width, screen_height),
            pressure: 1.0,
            action_button: 0,
            buttons: 0,
        }
    }

    /// Helper: Create touch up event
    pub fn touch_up(x: u32, y: u32, screen_width: u16, screen_height: u16) -> Self {
        Self::InjectTouchEvent {
            action: AndroidMotionEventAction::Up,
            pointer_id: POINTER_ID_MOUSE,
            position: Position::new(x, y, screen_width, screen_height),
            pressure: 1.0,
            action_button: 0,
            buttons: 0,
        }
    }

    /// Helper: Create touch move event
    pub fn touch_move(x: u32, y: u32, screen_width: u16, screen_height: u16) -> Self {
        Self::InjectTouchEvent {
            action: AndroidMotionEventAction::Move,
            pointer_id: POINTER_ID_MOUSE,
            position: Position::new(x, y, screen_width, screen_height),
            pressure: 1.0,
            action_button: 0,
            buttons: 0,
        }
    }

    /// Helper: Create key down event
    pub fn key_down(keycode: u32, metastate: u32) -> Self {
        Self::InjectKeycode {
            action: AndroidKeyEventAction::Down,
            keycode,
            repeat: 0,
            metastate,
        }
    }

    /// Helper: Create key up event
    pub fn key_up(keycode: u32, metastate: u32) -> Self {
        Self::InjectKeycode {
            action: AndroidKeyEventAction::Up,
            keycode,
            repeat: 0,
            metastate,
        }
    }

    /// Helper: Create scroll event
    pub fn scroll(
        x: u32,
        y: u32,
        screen_width: u16,
        screen_height: u16,
        hscroll: f32,
        vscroll: f32,
    ) -> Self {
        Self::InjectScrollEvent {
            position: Position::new(x, y, screen_width, screen_height),
            hscroll,
            vscroll,
            buttons: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_touch_down_serialization() {
        let msg = ControlMessage::touch_down(100, 200, 1080, 2340);
        let mut buf = Vec::new();
        let size = msg.serialize(&mut buf).unwrap();

        assert_eq!(size, 32, "Touch event should be 32 bytes");
        assert_eq!(buf[0], 2, "Type should be INJECT_TOUCH_EVENT");
        assert_eq!(buf[1], 0, "Action should be DOWN");

        // Verify pointer_id (u64 BE at offset 2)
        let pointer_id = u64::from_be_bytes(buf[2..10].try_into().unwrap());
        assert_eq!(pointer_id, POINTER_ID_MOUSE);

        // Verify position x (u32 BE at offset 10)
        let x = u32::from_be_bytes(buf[10..14].try_into().unwrap());
        assert_eq!(x, 100);

        // Verify position y (u32 BE at offset 14)
        let y = u32::from_be_bytes(buf[14..18].try_into().unwrap());
        assert_eq!(y, 200);

        // Verify screen_width (u16 BE at offset 18)
        let width = u16::from_be_bytes(buf[18..20].try_into().unwrap());
        assert_eq!(width, 1080);

        // Verify pressure (u16 BE at offset 22, should be 0xFFFF for 1.0)
        let pressure = u16::from_be_bytes(buf[22..24].try_into().unwrap());
        assert_eq!(pressure, 0xFFFF);
    }

    #[test]
    fn test_key_event_serialization() {
        let msg = ControlMessage::key_down(66, 0); // KEYCODE_ENTER
        let mut buf = Vec::new();
        let size = msg.serialize(&mut buf).unwrap();

        assert_eq!(size, 14, "Key event should be 14 bytes");
        assert_eq!(buf[0], 0, "Type should be INJECT_KEYCODE");
        assert_eq!(buf[1], 0, "Action should be DOWN");

        // Verify keycode (u32 BE at offset 2)
        let keycode = u32::from_be_bytes(buf[2..6].try_into().unwrap());
        assert_eq!(keycode, 66);
    }

    #[test]
    fn test_scroll_event_serialization() {
        let msg = ControlMessage::scroll(500, 600, 1080, 2340, 0.0, 5.0);
        let mut buf = Vec::new();
        let size = msg.serialize(&mut buf).unwrap();

        assert_eq!(size, 21, "Scroll event should be 21 bytes");
        assert_eq!(buf[0], 3, "Type should be INJECT_SCROLL_EVENT");

        // Verify vscroll (i16 BE at offset 15)
        let vscroll_fp = i16::from_be_bytes(buf[15..17].try_into().unwrap());
        // 5.0 / 16.0 = 0.3125, clamp to [-1, 1], * 32767 ≈ 10239
        assert!(vscroll_fp > 10000 && vscroll_fp < 11000);
    }

    #[test]
    fn test_text_injection_serialization() {
        let msg = ControlMessage::InjectText {
            text: "Hello".to_string(),
        };
        let mut buf = Vec::new();
        let size = msg.serialize(&mut buf).unwrap();

        assert_eq!(size, 10, "Text event should be 5 + text_length bytes");
        assert_eq!(buf[0], 1, "Type should be INJECT_TEXT");

        // Verify text length (u32 BE at offset 1)
        let text_len = u32::from_be_bytes(buf[1..5].try_into().unwrap());
        assert_eq!(text_len, 5);

        // Verify text content
        assert_eq!(&buf[5..10], b"Hello");
    }

    #[test]
    fn test_simple_commands() {
        let commands = vec![
            (ControlMessage::ExpandNotificationPanel, 1),
            (ControlMessage::CollapsePanels, 1),
            (ControlMessage::RotateDevice, 1),
        ];

        for (msg, expected_size) in commands {
            let mut buf = Vec::new();
            let size = msg.serialize(&mut buf).unwrap();
            assert_eq!(size, expected_size);
        }
    }
}
