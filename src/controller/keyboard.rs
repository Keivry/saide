//! Keyboard input mapping and handling
//!
//! Handles mapping from egui Key events to Android keycodes and custom actions.

use {
    crate::{
        config::mapping::{KeyMapping, MappingAction, Mappings, Modifiers, Profile, ScrcpyAction},
        controller::{
            android_keycode::{keycode as kc, metastate},
            control_sender::ControlSender,
        },
        error::Result,
        saide::coords::{MappingCoordSys, ScrcpyCoordSys},
    },
    arc_swap::ArcSwap,
    egui::Key,
    parking_lot::RwLock,
    std::{collections::HashMap, sync::Arc},
    tracing::{info, trace},
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
    config: Arc<Mappings>,

    sender: ControlSender,

    avail_profiles: RwLock<Vec<Arc<Profile>>>,

    active_profile: ArcSwap<Option<Arc<Profile>>>,

    /// Active mappings for runtime use, converted to scrcpy video coordinates
    scrcpy_mappings: RwLock<HashMap<Key, ScrcpyAction>>,

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
            scrcpy_mappings: RwLock::new(HashMap::new()),
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
            self.scrcpy_mappings.write().clear();
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
        if let Some(action) = self.scrcpy_mappings.read().get(key) {
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

    /// Get active profile mappings (percentage coordinates)
    pub fn get_active_mappings(&self) -> Option<Arc<KeyMapping>> {
        self.active_profile
            .load()
            .as_ref()
            .clone()
            .map(|p| Arc::clone(&p.mappings))
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

        *self.scrcpy_mappings.write() = active_mappings;
    }

    /// Get mapping action for a given key from the active profile
    pub fn get_mapping(&self, key: &Key) -> Option<MappingAction> {
        if let Some(active_profile) = self.get_active_profile()
            && let Some(action) = active_profile.mappings.read().get(key)
        {
            return Some(action.clone());
        }
        None
    }

    /// Add a new mapping to the active profile
    pub fn add_mapping(&self, key: Key, action: MappingAction) {
        if let Some(active_profile) = self.get_active_profile() {
            active_profile.mappings.write().insert(key, action.clone());

            let (video_width, video_height) = self.sender.get_screen_size();

            // Create coordinate systems for conversion
            let mapping_sys = MappingCoordSys::new(active_profile.rotation);
            let scrcpy_sys =
                ScrcpyCoordSys::new(video_width, video_height, self.capture_orientation);
            let scrcpy_action =
                ScrcpyAction::from_mapping_action(&action, &scrcpy_sys, &mapping_sys);
            self.add_scrcpy_mapping(key, scrcpy_action);
        }
    }

    /// Delete a mapping from the active profile
    pub fn delete_mapping(&self, key: &Key) {
        if let Some(active_profile) = self.get_active_profile() {
            active_profile.mappings.write().remove(key);

            self.delete_scrcpy_mapping(key);
        }
    }

    /// Add a new scrcpy mapping (pixel coordinates)
    fn add_scrcpy_mapping(&self, key: Key, action: ScrcpyAction) {
        self.scrcpy_mappings.write().insert(key, action);
    }

    /// Delete a scrcpy mapping (pixel coordinates)
    fn delete_scrcpy_mapping(&self, key: &Key) { self.scrcpy_mappings.write().remove(key); }
}
