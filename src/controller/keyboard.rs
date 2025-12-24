//! Keyboard input mapping and handling
//!
//! Handles mapping from egui Key events to Android keycodes and custom actions.

use {
    crate::{
        app::coords::{MappingCoordSys, ScrcpyCoordSys},
        config::mapping::{MappingAction, Mappings, Modifiers, Profile, ScrcpyAction},
        controller::control_sender::ControlSender,
        error::Result,
    },
    arc_swap::ArcSwap,
    egui::Key,
    parking_lot::RwLock,
    std::{collections::HashMap, sync::Arc},
    tracing::{info, trace},
};

lazy_static::lazy_static! {
    /// Mapping from egui Key to Android keycode
    pub static ref EGUI_TO_ANDROID_KEY: HashMap<Key, u8> = {
        let mut m = HashMap::new();

        // Arrow keys
        m.insert(Key::ArrowUp,      19);   // KEYCODE_DPAD_UP
        m.insert(Key::ArrowDown,    20);   // KEYCODE_DPAD_DOWN
        m.insert(Key::ArrowLeft,    21);   // KEYCODE_DPAD_LEFT
        m.insert(Key::ArrowRight,   22);   // KEYCODE_DPAD_RIGHT

        // Common control keys
        m.insert(Key::Escape,       4);    // Map to KEYCODE_BACK
        m.insert(Key::Tab,          61);   // KEYCODE_TAB
        m.insert(Key::Space,        62);   // KEYCODE_SPACE
        m.insert(Key::Enter,        66);   // KEYCODE_ENTER
        m.insert(Key::Backspace,    67);   // KEYCODE_DEL

        // Editing/navigation keys
        m.insert(Key::Insert,       110);  // KEYCODE_INSERT
        m.insert(Key::Delete,       112);  // KEYCODE_FORWARD_DEL
        m.insert(Key::Home,         122);  // KEYCODE_MOVE_HOME
        m.insert(Key::End,          123);  // KEYCODE_MOVE_END
        m.insert(Key::PageUp,       92);   // KEYCODE_PAGE_UP
        m.insert(Key::PageDown,     93);   // KEYCODE_PAGE_DOWN

        // Letters A-Z
        m.insert(Key::A, 29); m.insert(Key::B, 30); m.insert(Key::C, 31);
        m.insert(Key::D, 32); m.insert(Key::E, 33); m.insert(Key::F, 34);
        m.insert(Key::G, 35); m.insert(Key::H, 36); m.insert(Key::I, 37);
        m.insert(Key::J, 38); m.insert(Key::K, 39); m.insert(Key::L, 40);
        m.insert(Key::M, 41); m.insert(Key::N, 42); m.insert(Key::O, 43);
        m.insert(Key::P, 44); m.insert(Key::Q, 45); m.insert(Key::R, 46);
        m.insert(Key::S, 47); m.insert(Key::T, 48); m.insert(Key::U, 49);
        m.insert(Key::V, 50); m.insert(Key::W, 51); m.insert(Key::X, 52);
        m.insert(Key::Y, 53); m.insert(Key::Z, 54);

        // Numbers 0-9
        m.insert(Key::Num0, 7);  m.insert(Key::Num1, 8);  m.insert(Key::Num2, 9);
        m.insert(Key::Num3,10);  m.insert(Key::Num4,11);  m.insert(Key::Num5,12);
        m.insert(Key::Num6,13);  m.insert(Key::Num7,14);  m.insert(Key::Num8,15);
        m.insert(Key::Num9,16);

        // Other common symbols
        m.insert(Key::Comma,            55);  // KEYCODE_COMMA
        m.insert(Key::Period,           56);  // KEYCODE_PERIOD
        m.insert(Key::Slash,            76);  // KEYCODE_SLASH
        m.insert(Key::Backslash,        73);  // KEYCODE_BACKSLASH
        m.insert(Key::Semicolon,        74);  // KEYCODE_SEMICOLON
        m.insert(Key::Quote,            75);  // KEYCODE_APOSTROPHE
        m.insert(Key::OpenBracket,      71);  // KEYCODE_LEFT_BRACKET  [
        m.insert(Key::CloseBracket,     72);  // KEYCODE_RIGHT_BRACKET ]
        m.insert(Key::Minus,            69);  // KEYCODE_MINUS
        m.insert(Key::Equals,           70);  // KEYCODE_EQUALS

        // Function keys F1-F12
        for i in 1..=12 {
            if let Some(key) = Key::from_name(&format!("F{i}")) {
                m.insert(key, 130 + i);
            }
        }

        m
    };

    /// Mapping from egui Key to Android shifted keycode
    /// These keys in Android require SHIFT to produce the desired character
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
    config: Arc<Mappings>,

    sender: ControlSender,

    avail_profiles: RwLock<Vec<Arc<Profile>>>,

    active_profile: ArcSwap<Option<Arc<Profile>>>,

    /// Active mappings for runtime use, converted to scrcpy video coordinates
    active_mappings: RwLock<HashMap<Key, ScrcpyAction>>,

    /// Scrcpy capture orientation state
    capture_orientation: Option<u32>,
}

impl KeyboardMapper {
    /// Create a new keyboard mapper
    pub fn new(
        config: Arc<Mappings>,
        sender: ControlSender,
        capture_orientation: Option<u32>,
    ) -> Self {
        Self {
            config,
            sender,
            avail_profiles: RwLock::new(Vec::new()),
            active_profile: ArcSwap::from_pointee(None),
            active_mappings: RwLock::new(HashMap::new()),
            capture_orientation,
        }
    }

    /// Refresh available profiles based on device serial and rotation
    ///
    /// # Parameters
    /// - `device_serial`: current device serial
    /// - `device_rotation`: current device rotation (0, 1, 2, 3)
    pub fn refresh_profiles(&self, device_serial: &str, device_rotation: u32) {
        let avail_profiles = self.config.filter_profiles(device_serial, device_rotation);

        if avail_profiles.is_empty() {
            info!(
                "No matching profiles found for device serial '{}' with rotation {}.",
                device_serial, device_rotation
            );
            info!("Disable custom key mappings for this device/rotation.");

            self.active_profile.store(Arc::new(None));
            self.active_mappings.write().clear();
        } else {
            info!(
                "Found {} matching profiles for device serial '{}' with rotation {}.",
                avail_profiles.len(),
                device_serial,
                device_rotation
            );

            // Set active profile (keeping percentage coordinates)
            let profile = avail_profiles[0].clone();
            self.active_profile.store(Arc::new(Some(profile.clone())));
            info!("Active profile set to: {}", profile.name);

            // Convert percentage to pixels for runtime use
            self.apply_active_profile();
        }

        *self.avail_profiles.write() = avail_profiles;
    }

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

            // Convert modifiers to Android metastate
            // AMETA_SHIFT_ON = 1, AMETA_ALT_ON = 2, AMETA_CTRL_ON = 4096, AMETA_META_ON = 65536
            let mut metastate = 0u32;
            if modifiers.shift {
                metastate |= 1;
            }
            if modifiers.alt {
                metastate |= 2;
            }
            if modifiers.ctrl || modifiers.command {
                metastate |= 4096;
            }

            self.sender.send_key_press(keycode as u32, metastate)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Handle custom key mapping event, returns true if handled
    pub fn handle_custom_keymapping_event(&self, key: &Key) -> Result<bool> {
        // Use pixel-converted mappings for control
        if let Some(action) = self.active_mappings.read().get(key) {
            trace!(
                "Handling custom key mapping event: {:?} -> {:?}",
                key, action
            );
            self.send_input_action(action)?;
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
                    metastate |= 1;
                }
                if modifiers.alt {
                    metastate |= 2;
                }
                if modifiers.ctrl || modifiers.command {
                    metastate |= 4096;
                }
                self.sender.send_key_press(*keycode as u32, metastate)?;
            }
            ScrcpyAction::Text { text } => {
                self.sender.send_text(text)?;
            }
            ScrcpyAction::Back => {
                self.sender.send_key_press(4, 0)?; // KEYCODE_BACK
            }
            ScrcpyAction::Home => {
                self.sender.send_key_press(3, 0)?; // KEYCODE_HOME
            }
            ScrcpyAction::Menu => {
                self.sender.send_key_press(82, 0)?; // KEYCODE_MENU
            }
            ScrcpyAction::Power => {
                self.sender.send_key_press(26, 0)?; // KEYCODE_POWER
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

    /// Get list of available profiles
    pub fn get_avail_profiles(&self) -> Vec<String> {
        let avail_profiles = self.avail_profiles.read();
        avail_profiles.iter().map(|p| p.name.clone()).collect()
    }

    /// Get active profile (for read-only access)
    pub fn get_active_profile(&self) -> Option<Arc<Profile>> {
        self.active_profile.load().as_ref().clone()
    }

    /// Get active profile name
    pub fn get_active_profile_name(&self) -> Option<String> {
        self.get_active_profile().map(|p| p.name.clone())
    }

    /// Apply active profile by converting percentage mappings to pixel coordinates
    pub fn apply_active_profile(&self) {
        let active_profile = match self.get_active_profile() {
            Some(p) => p,
            None => {
                trace!("No active profile to apply.");
                return;
            }
        };
        let mut active_mappings = HashMap::new();
        let (video_width, video_height) = self.sender.get_screen_size();

        // Create coordinate systems for conversion
        let mapping_sys = MappingCoordSys::new(active_profile.rotation);
        let scrcpy_sys = ScrcpyCoordSys::new(video_width, video_height, self.capture_orientation);

        for (key, action) in active_profile.mappings.read().iter() {
            let scrcpy_action =
                ScrcpyAction::from_mapping_action(action, &scrcpy_sys, &mapping_sys);
            active_mappings.insert(*key, scrcpy_action);
        }

        if let Some(capture_orientation) = self.capture_orientation {
            trace!(
                "Converted {} mappings from percentage (rotation={}) to {}x{} pixels (capture locked to {}°)",
                active_mappings.len(),
                active_profile.rotation,
                video_width,
                video_height,
                capture_orientation * 90
            );
        } else {
            trace!(
                "Converted {} mappings from percentage to {}x{} pixels (no transform)",
                active_mappings.len(),
                video_width,
                video_height
            );
        }

        *self.active_mappings.write() = active_mappings;
    }

    pub fn add_profile_mapping(&self, key: Key, action: MappingAction) {
        if let Some(active_profile) = self.get_active_profile() {
            active_profile.mappings.write().insert(key, action.clone());

            let (video_width, video_height) = self.sender.get_screen_size();

            // Create coordinate systems for conversion
            let mapping_sys = MappingCoordSys::new(active_profile.rotation);
            let scrcpy_sys =
                ScrcpyCoordSys::new(video_width, video_height, self.capture_orientation);
            let scrcpy_action =
                ScrcpyAction::from_mapping_action(&action, &scrcpy_sys, &mapping_sys);
            self.add_mapping(key, scrcpy_action);
        }
    }

    pub fn delete_profile_mapping(&self, key: &Key) {
        if let Some(active_profile) = self.get_active_profile() {
            active_profile.mappings.write().remove(key);

            self.delete_mapping(key);
        }
    }

    fn add_mapping(&self, key: Key, action: ScrcpyAction) {
        self.active_mappings.write().insert(key, action);
    }

    fn delete_mapping(&self, key: &Key) { self.active_mappings.write().remove(key); }

    pub fn get_profile_mapping(&self, key: &Key) -> Option<MappingAction> {
        if let Some(active_profile) = self.get_active_profile()
            && let Some(action) = active_profile.mappings.read().get(key)
        {
            return Some(action.clone());
        }
        None
    }
}
