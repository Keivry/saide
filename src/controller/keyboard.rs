use {
    crate::{
        config::mapping::{Mappings, Modifiers, Profile},
        controller::control_sender::ControlSender,
    },
    anyhow::{Result, anyhow},
    arc_swap::ArcSwap,
    egui::Key,
    parking_lot::RwLock,
    std::{collections::HashMap, sync::Arc},
    tracing::{error, info, trace},
};

lazy_static::lazy_static! {
    /// Mapping from egui Key to Android keycode
    pub static ref EGUI_TO_ANDROID_KEY: HashMap<Key, u8> = {
        let mut m = HashMap::new();

        // ────── 方向 & 导航键 ──────
        m.insert(Key::ArrowUp,      19);   // KEYCODE_DPAD_UP
        m.insert(Key::ArrowDown,    20);   // KEYCODE_DPAD_DOWN
        m.insert(Key::ArrowLeft,    21);   // KEYCODE_DPAD_LEFT
        m.insert(Key::ArrowRight,   22);   // KEYCODE_DPAD_RIGHT

        m.insert(Key::Escape,       4);    // 强烈推荐映射为 Back 键（手机上最常用）
        m.insert(Key::Tab,          61);   // KEYCODE_TAB
        m.insert(Key::Space,        62);   // KEYCODE_SPACE
        m.insert(Key::Enter,        66);   // KEYCODE_ENTER
        m.insert(Key::Backspace,    67);   // KEYCODE_DEL

        m.insert(Key::Insert,       110);  // KEYCODE_INSERT
        m.insert(Key::Delete,       112);  // KEYCODE_FORWARD_DEL
        m.insert(Key::Home,         122);  // KEYCODE_MOVE_HOME
        m.insert(Key::End,          123);  // KEYCODE_MOVE_END
        m.insert(Key::PageUp,       92);   // KEYCODE_PAGE_UP
        m.insert(Key::PageDown,     93);   // KEYCODE_PAGE_DOWN

        // ────── 字母 A-Z ──────
        m.insert(Key::A, 29); m.insert(Key::B, 30); m.insert(Key::C, 31);
        m.insert(Key::D, 32); m.insert(Key::E, 33); m.insert(Key::F, 34);
        m.insert(Key::G, 35); m.insert(Key::H, 36); m.insert(Key::I, 37);
        m.insert(Key::J, 38); m.insert(Key::K, 39); m.insert(Key::L, 40);
        m.insert(Key::M, 41); m.insert(Key::N, 42); m.insert(Key::O, 43);
        m.insert(Key::P, 44); m.insert(Key::Q, 45); m.insert(Key::R, 46);
        m.insert(Key::S, 47); m.insert(Key::T, 48); m.insert(Key::U, 49);
        m.insert(Key::V, 50); m.insert(Key::W, 51); m.insert(Key::X, 52);
        m.insert(Key::Y, 53); m.insert(Key::Z, 54);

        // ────── 主键盘数字 0-9 ──────
        m.insert(Key::Num0, 7);  m.insert(Key::Num1, 8);  m.insert(Key::Num2, 9);
        m.insert(Key::Num3,10);  m.insert(Key::Num4,11);  m.insert(Key::Num5,12);
        m.insert(Key::Num6,13);  m.insert(Key::Num7,14);  m.insert(Key::Num8,15);
        m.insert(Key::Num9,16);

        // ────── 标点符号（完整覆盖 egui 新增键） ──────
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

        // ────── 功能键 F1~F12 ──────
        for i in 1..=12 {
            if let Some(key) = Key::from_name(&format!("F{i}")) {
                m.insert(key, 130 + i);
            }
        }

        m
    };

    /// Mapping from egui Key to Android shifted keycode
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

    /// Keys that should not be handled, handled via text input instead
    pub static ref SHOULD_NOT_HANDLED_KEYS: Vec<Key> = vec![
        Key::Backtick
    ];

    // Text input special character mappings before sending to adb shell
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
}

impl KeyboardMapper {
    /// Create a new keyboard mapper
    pub fn new(config: Arc<Mappings>, sender: ControlSender) -> Result<Self> {
        Ok(Self {
            config,
            sender,
            avail_profiles: RwLock::new(Vec::new()),
            active_profile: ArcSwap::from_pointee(None),
        })
    }

    /// Refresh available profiles based on device ID and rotation
    pub fn refresh_profiles(&self, device_id: &str, device_rotation: u32) -> Result<()> {
        let avail_profiles = self.config.filter_profiles(device_id, device_rotation);

        if avail_profiles.is_empty() {
            info!(
                "No matching profiles found for device ID '{}' with rotation {}.",
                device_id, device_rotation
            );
            info!("Disable custom key mappings for this device/rotation.");

            self.active_profile.store(Arc::new(None));
        } else {
            info!(
                "Found {} matching profiles for device ID '{}' with rotation {}.",
                avail_profiles.len(),
                device_id,
                device_rotation
            );
            
            // Get video resolution from ControlSender
            let (video_width, video_height) = self.sender.get_screen_size();
            
            // Convert percentage coordinates to pixels for all profiles
            for profile in &avail_profiles {
                profile.convert_to_pixels(video_width as u32, video_height as u32);
            }
            
            self.active_profile
                .store(Arc::new(Some(avail_profiles[0].clone())));
            info!("Active profile set to: {}", avail_profiles[0].name);
        }

        *self.avail_profiles.write() = avail_profiles;

        Ok(())
    }

    /// Load profile by name
    #[allow(dead_code)]
    pub fn load_profile(&mut self, name: &str) -> Result<()> {
        let profile = self
            .avail_profiles
            .read()
            .iter()
            .find(|p| p.name == name)
            .cloned()
            .ok_or_else(|| {
                error!("Profile '{}' not found.", name);
                anyhow!("Profile not found: {}.", name)
            })?;

        self.active_profile.store(Arc::new(Some(profile.clone())));
        info!("Active profile set to: {}", profile.name);

        Ok(())
    }

    /// Get active profile name
    pub fn get_active_profile_name(&self) -> Option<String> {
        self.active_profile
            .load()
            .as_ref()
            .as_ref()
            .map(|p| p.name.clone())
    }

    /// Handle keyboard event
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

    /// Handle text input event
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

    /// Handle custom keyboard event using legacy ADB actions
    ///
    /// Note: This still uses the legacy AdbAction format from config.toml
    /// TODO: Convert custom mappings to use ControlMessage directly
    pub fn handle_custom_keymapping_event(&self, key: &Key) -> Result<bool> {
        if let Some(profile) = self.active_profile.load().as_ref()
            && let Some(action) = profile.get_mapping(key)
        {
            trace!(
                "Handling custom key mapping event: {:?} -> {:?}",
                key, action
            );
            self.send_adb_action(&action)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Convert AdbAction to control messages (temporary bridge)
    fn send_adb_action(&self, action: &crate::config::mapping::AdbAction) -> Result<()> {
        use crate::config::mapping::AdbAction;

        match action {
            AdbAction::Key { keycode } => {
                self.sender.send_key_press(*keycode as u32, 0)?;
            }
            AdbAction::KeyCombo { modifiers, keycode } => {
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
            AdbAction::Text { text } => {
                self.sender.send_text(text)?;
            }
            AdbAction::Back => {
                self.sender.send_key_press(4, 0)?; // KEYCODE_BACK
            }
            AdbAction::Home => {
                self.sender.send_key_press(3, 0)?; // KEYCODE_HOME
            }
            AdbAction::Menu => {
                self.sender.send_key_press(82, 0)?; // KEYCODE_MENU
            }
            AdbAction::Power => {
                self.sender.send_key_press(26, 0)?; // KEYCODE_POWER
            }
            AdbAction::Tap { x, y } => {
                self.sender.send_touch_down(*x, *y)?;
                self.sender.send_touch_up(*x, *y)?;
            }
            AdbAction::TouchDown { x, y } => {
                self.sender.send_touch_down(*x, *y)?;
            }
            AdbAction::TouchMove { x, y } => {
                self.sender.send_touch_move(*x, *y)?;
            }
            AdbAction::TouchUp { x, y } => {
                self.sender.send_touch_up(*x, *y)?;
            }
            AdbAction::Scroll { x, y, direction } => {
                use crate::config::mapping::WheelDirection;
                let (h, v) = match direction {
                    WheelDirection::Up => (0.0, -5.0),
                    WheelDirection::Down => (0.0, 5.0),
                };
                self.sender.send_scroll(*x, *y, h, v)?;
            }
            AdbAction::Swipe { x1, y1, x2, y2, .. } => {
                // Simulate swipe with touch down + move + up
                self.sender.send_touch_down(*x1, *y1)?;
                self.sender.send_touch_move(*x2, *y2)?;
                self.sender.send_touch_up(*x2, *y2)?;
            }
            AdbAction::Ignore => {}
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
}
