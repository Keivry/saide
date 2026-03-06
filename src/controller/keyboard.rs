//! Keyboard input mapping and handling
//!
//! Handles mapping from egui Key events to Android keycodes and custom actions.

use {
    crate::{
        config::mapping::{Modifiers, ProfileRef, ScrcpyAction},
        controller::{
            android_keycode::{keycode as kc, metastate},
            control_sender::ControlSender,
        },
        core::coords::{MappingCoordSys, ScrcpyCoordSys},
        error::Result,
    },
    egui::Key,
    parking_lot::RwLock,
    std::collections::HashMap,
    tracing::trace,
};

lazy_static::lazy_static! {
    pub static ref EGUI_TO_ANDROID_KEY: HashMap<Key, u8> = {
        let mut m = HashMap::new();

        m.insert(Key::ArrowUp, kc::DPAD_UP);
        m.insert(Key::ArrowDown, kc::DPAD_DOWN);
        m.insert(Key::ArrowLeft, kc::DPAD_LEFT);
        m.insert(Key::ArrowRight, kc::DPAD_RIGHT);

        m.insert(Key::Escape, kc::BACK);
        m.insert(Key::Tab, kc::TAB);
        m.insert(Key::Space, kc::SPACE);
        m.insert(Key::Enter, kc::ENTER);
        m.insert(Key::Backspace, kc::DEL);

        m.insert(Key::Insert, kc::INSERT);
        m.insert(Key::Delete, kc::FORWARD_DEL);
        m.insert(Key::Home, kc::MOVE_HOME);
        m.insert(Key::End, kc::MOVE_END);
        m.insert(Key::PageUp, kc::PAGE_UP);
        m.insert(Key::PageDown, kc::PAGE_DOWN);

        m.insert(Key::A, kc::A); m.insert(Key::B, kc::B); m.insert(Key::C, kc::C);
        m.insert(Key::D, kc::D); m.insert(Key::E, kc::E); m.insert(Key::F, kc::F);
        m.insert(Key::G, kc::G); m.insert(Key::H, kc::H); m.insert(Key::I, kc::I);
        m.insert(Key::J, kc::J); m.insert(Key::K, kc::K); m.insert(Key::L, kc::L);
        m.insert(Key::M, kc::M); m.insert(Key::N, kc::N); m.insert(Key::O, kc::O);
        m.insert(Key::P, kc::P); m.insert(Key::Q, kc::Q); m.insert(Key::R, kc::R);
        m.insert(Key::S, kc::S); m.insert(Key::T, kc::T); m.insert(Key::U, kc::U);
        m.insert(Key::V, kc::V); m.insert(Key::W, kc::W); m.insert(Key::X, kc::X);
        m.insert(Key::Y, kc::Y); m.insert(Key::Z, kc::Z);

        m.insert(Key::Num0, kc::NUM_0); m.insert(Key::Num1, kc::NUM_1); m.insert(Key::Num2, kc::NUM_2);
        m.insert(Key::Num3, kc::NUM_3); m.insert(Key::Num4, kc::NUM_4); m.insert(Key::Num5, kc::NUM_5);
        m.insert(Key::Num6, kc::NUM_6); m.insert(Key::Num7, kc::NUM_7); m.insert(Key::Num8, kc::NUM_8);
        m.insert(Key::Num9, kc::NUM_9);

        m.insert(Key::Comma, kc::COMMA);
        m.insert(Key::Period, kc::PERIOD);
        m.insert(Key::Slash, kc::SLASH);
        m.insert(Key::Backslash, kc::BACKSLASH);
        m.insert(Key::Semicolon, kc::SEMICOLON);
        m.insert(Key::Quote, kc::APOSTROPHE);
        m.insert(Key::OpenBracket, kc::LEFT_BRACKET);
        m.insert(Key::CloseBracket, kc::RIGHT_BRACKET);
        m.insert(Key::Minus, kc::MINUS);
        m.insert(Key::Equals, kc::EQUALS);

        for i in 1..=12 {
            if let Some(key) = Key::from_name(&format!("F{i}")) {
                m.insert(key, kc::F1 + i - 1);
            }
        }

        m
    };
}

lazy_static::lazy_static! {
    pub static ref EGUI_TO_ANDROID_SHIFT_KEY: HashMap<Key, u8> = {
        let mut m = HashMap::new();
        m.insert(Key::Exclamationmark,   8);        // KEYCODE_1
        m.insert(Key::Pipe,              73);       // KEYCODE_BACKSLASH
        m.insert(Key::OpenCurlyBracket,  71);       // KEYCODE_LEFT_BRACKET
        m.insert(Key::CloseCurlyBracket, 72);       // KEYCODE_RIGHT_BRACKET
        m.insert(Key::Colon,             74);       // KEYCODE_SEMICOLON
        m.insert(Key::Questionmark,      76);       // KEYCODE_SLASH
        m
    };

    /// Keys that should not be handled in Android, should be handled via text input instead
    pub static ref SHOULD_NOT_HANDLED_KEYS: Vec<Key> = vec![
        Key::Backtick
    ];

    /// Text input mappings for special characters
    /// These characters need to be escaped or replaced for proper input
    pub static ref TEXT_MAPPINGS: HashMap<String, String > = {
        let mut m = HashMap::new();
        m.insert("`".to_owned(), "\\`".to_owned());
        m
    };
}

/// Keyboard mapping state
pub struct KeyboardMapper {
    sender: ControlSender,

    /// Active mappings for runtime use, converted to scrcpy video coordinates
    mappings: RwLock<HashMap<Key, ScrcpyAction>>,
}

impl KeyboardMapper {
    /// Create a new keyboard mapper
    pub fn new(sender: ControlSender) -> Self {
        Self {
            sender,
            mappings: RwLock::new(HashMap::new()),
        }
    }

    /// Update mappings from profile, converting to scrcpy video coordinates
    pub fn update_mappings(
        &self,
        profile: &ProfileRef,
        scrcpy_coords: &ScrcpyCoordSys,
        mapping_coords: &MappingCoordSys,
    ) {
        let profile = profile.read();
        let name = profile.name();
        *self.mappings.write() = profile
            .mappings()
            .to_scrcpy_mapping(scrcpy_coords, mapping_coords);

        trace!("Updated keyboard mappings from profile '{name}'",);
    }

    pub fn add_mapping(&self, key: Key, action: ScrcpyAction) {
        self.mappings.write().insert(key, action);
    }

    pub fn remove_mapping(&self, key: &Key) { self.mappings.write().remove(key); }

    /// Handle keyboard event, returns true if handled
    pub fn handle_standard_key_event(&self, key: &Key) -> Result<bool> {
        if SHOULD_NOT_HANDLED_KEYS.contains(key) {
            return Ok(false);
        }

        if let Some(&keycode) = EGUI_TO_ANDROID_KEY.get(key) {
            trace!(
                "Handling standard key event: {:?} -> keycode {}",
                key, keycode
            );
            self.sender.send_key_press(keycode as u32, 0)?;
            return Ok(true);
        }

        Ok(false)
    }

    /// Handle shifted key event, returns true if handled
    pub fn handle_shifted_key_event(&self, key: &Key) -> Result<bool> {
        if let Some(&keycode) = EGUI_TO_ANDROID_SHIFT_KEY.get(key) {
            trace!(
                "Handling shifted key event: {:?} -> keycode {}",
                key, keycode
            );
            // SHIFT metastate = 1 (AMETA_SHIFT_ON)
            self.sender.send_key_press(keycode as u32, 1)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Handle text input event, returns true if handled
    pub fn handle_text_input_event(&self, text: &str) -> Result<bool> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(false);
        }

        let text = TEXT_MAPPINGS
            .iter()
            .fold(text.to_owned(), |acc, (k, v)| acc.replace(k, v));

        trace!("Handling text input event: {}", text);
        self.sender.send_text(&text)?;
        Ok(true)
    }

    /// Handle key combo event, returns true if handled
    pub fn handle_keycombo_event(&self, modifiers: Modifiers, key: &Key) -> Result<bool> {
        if let Some(&keycode) = EGUI_TO_ANDROID_KEY.get(key) {
            trace!("Handling key combo event: {:?} + {:?}", modifiers, key);

            let mut metastate = 0u32;
            if modifiers.shift {
                metastate |= metastate::SHIFT_ON;
            }
            if modifiers.alt {
                metastate |= metastate::ALT_ON;
            }
            if modifiers.ctrl || modifiers.command {
                metastate |= metastate::CTRL_ON;
            }

            self.sender.send_key_press(keycode as u32, metastate)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Handle custom key mapping event, returns true if handled
    pub fn handle_custom_keymapping_event(&self, key: &Key) -> Result<bool> {
        // Use pixel-converted mappings for control
        let action = {
            let mappings = self.mappings.read();
            mappings.get(key).cloned()
        };
        if let Some(action) = action {
            trace!(
                "Handling custom key mapping event: {:?} -> {:?}",
                key, action
            );
            self.send_input_action(&action)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Send input action via control sender
    fn send_input_action(&self, action: &ScrcpyAction) -> Result<()> {
        match action {
            ScrcpyAction::Key { keycode } => {
                self.sender.send_key_press(*keycode as u32, 0)?;
            }
            ScrcpyAction::KeyCombo { modifiers, keycode } => {
                let mut metastate = 0u32;
                if modifiers.shift {
                    metastate |= metastate::SHIFT_ON;
                }
                if modifiers.alt {
                    metastate |= metastate::ALT_ON;
                }
                if modifiers.ctrl || modifiers.command {
                    metastate |= metastate::CTRL_ON;
                }
                self.sender.send_key_press(*keycode as u32, metastate)?;
            }
            ScrcpyAction::Text { text } => {
                self.sender.send_text(text)?;
            }
            ScrcpyAction::Back => {
                self.sender.send_key_press(kc::BACK as u32, 0)?;
            }
            ScrcpyAction::Home => {
                self.sender.send_key_press(kc::HOME as u32, 0)?;
            }
            ScrcpyAction::Menu => {
                self.sender.send_key_press(kc::MENU as u32, 0)?;
            }
            ScrcpyAction::Power => {
                self.sender.send_key_press(kc::POWER as u32, 0)?;
            }
            ScrcpyAction::Tap { pos } => {
                self.sender.send_touch_down(pos.x, pos.y)?;
                self.sender.send_touch_up(pos.x, pos.y)?;
            }
            ScrcpyAction::TouchDown { pos } => {
                self.sender.send_touch_down(pos.x, pos.y)?;
            }
            ScrcpyAction::TouchMove { pos } => {
                self.sender.send_touch_move(pos.x, pos.y)?;
            }
            ScrcpyAction::TouchUp { pos } => {
                self.sender.send_touch_up(pos.x, pos.y)?;
            }
            ScrcpyAction::Scroll { pos, direction } => {
                use crate::config::mapping::WheelDirection;
                let (h, v) = match direction {
                    WheelDirection::Up => (0.0, -5.0),
                    WheelDirection::Down => (0.0, 5.0),
                };
                self.sender.send_scroll(pos.x, pos.y, h, v)?;
            }
            ScrcpyAction::Swipe { path, .. } => {
                // Simulate swipe with touch down + move + up
                self.sender.send_touch_down(path[0].x, path[0].y)?;
                self.sender.send_touch_move(path[1].x, path[1].y)?;
                self.sender.send_touch_up(path[1].x, path[1].y)?;
            }
            ScrcpyAction::Ignore => {}
        }
        Ok(())
    }
}
