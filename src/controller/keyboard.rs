use {
    crate::{
        config::mapping::{AdbAction, Key, MappingsConfig, Modifiers, Profile},
        controller::adb::AdbShell,
    },
    anyhow::{Result, anyhow},
    parking_lot::Mutex,
    std::{collections::HashMap, sync::Arc},
    tracing::{error, info},
};

lazy_static::lazy_static! {
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

        // ────── 编辑命令（Copy/Cut/Paste 在 Android 没有单键，用组合键处理，这里留空或映射常用快捷键） ──────
        // m.insert(Key::Copy,      ???);   // 无单键
        // m.insert(Key::Cut,       ???);
        // m.insert(Key::Paste,     ???);

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

        // ────── 功能键 F1~F35（Android 官方支持到 F12，后面是扩展键码） ──────
        for i in 1..=35 {
            if let Some(key) = Key::from_name(&format!("F{i}")) {
                m.insert(key, 130 + i); // F1=131, F2=132, ..., F35=165
            }
        }

        // ────── 标点符号（完整覆盖 egui 新增键） ──────
        m.insert(Key::Colon,           243);  // KEYCODE_COLON (Android 8.0+)
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
        m.insert(Key::Plus,             81);  // KEYCODE_PLUS
        m.insert(Key::Backtick,         68);  // KEYCODE_GRAVE ( ` )
        m.insert(Key::Pipe,            228);  // KEYCODE_PIPE (Android 11+)
        m.insert(Key::Questionmark,    232);  // KEYCODE_QUESTION (Android 11+)
        m.insert(Key::Exclamationmark, 231);  // KEYCODE_EXPLAMATION (Android 11+)
        m.insert(Key::OpenCurlyBracket,  71); // 没有专用键码，通常用 Shift + [
        m.insert(Key::CloseCurlyBracket, 72); // 没有专用键码，通常用 Shift + ]

        // ────── 特殊键 ──────
        m.insert(Key::BrowserBack,       4);   // 官方说明就是 Back 键

        m
    };
}

/// Keyboard mapping state
pub struct KeyboardMapper {
    config: Arc<MappingsConfig>,

    adb_shell: AdbShell,

    avail_profiles: Mutex<Vec<Arc<Profile>>>,
    active_profile: Mutex<Option<Arc<Profile>>>,
}

impl KeyboardMapper {
    /// Create a new keyboard mapper
    pub fn new(config: Arc<MappingsConfig>) -> Result<Self> {
        let adb = AdbShell::new(false);
        adb.connect()?;
        Ok(Self {
            config,

            adb_shell: adb,

            avail_profiles: Mutex::new(Vec::new()),
            active_profile: Mutex::new(None),
        })
    }

    /// Refresh available profiles based on device ID and rotation
    pub fn refresh_profiles(&self, device_id: &str, device_rotation: u32) -> Result<()> {
        let mut available_profiles = Vec::new();

        self.config
            .profiles
            .iter()
            .filter(|profile| profile.device_id == device_id && profile.rotation == device_rotation)
            .for_each(|profile| {
                available_profiles.push(profile.clone());
            });

        if available_profiles.is_empty() {
            info!(
                "No matching profiles found for device ID '{}' with rotation {}.",
                device_id, device_rotation
            );
            info!("Disable custom key mappings for this device/rotation.");

            self.active_profile.lock().take();
        } else {
            info!(
                "Found {} matching profiles for device ID '{}' with rotation {}.",
                available_profiles.len(),
                device_id,
                device_rotation
            );
            self.active_profile
                .lock()
                .replace(available_profiles[0].clone());
            info!("Active profile set to: {}", available_profiles[0].name);
        }

        let mut avail_lock = self.avail_profiles.lock();
        *avail_lock = available_profiles;

        Ok(())
    }

    /// Load profile by index
    pub fn load_profile(&mut self, index: usize) -> Result<()> {
        let avail_profiles = self.avail_profiles.lock();
        if index < avail_profiles.len() {
            self.active_profile
                .lock()
                .replace(avail_profiles[index].clone());
            info!("Profile switched to: {}", avail_profiles[index].name);
        } else {
            error!("Profile index {} out of range.", index);
            return Err(anyhow!("Profile index out of range."));
        }
        Ok(())
    }

    /// Load profile by name
    pub fn load_profile_by_name(&mut self, name: &str) -> Result<()> {
        let avail_profiles = self.avail_profiles.lock();
        if let Some(profile) = avail_profiles.iter().find(|p| p.name == name) {
            self.active_profile.lock().replace(profile.clone());
            info!("Profile switched to: {}", profile.name);
        } else {
            error!("Profile '{}' not found.", name);
            return Err(anyhow!("Profile not found."));
        }
        Ok(())
    }

    /// Get active profile name
    pub fn get_active_profile_name(&self) -> Option<String> {
        self.active_profile
            .lock()
            .as_ref()
            .map(|profile| profile.name.clone())
    }

    /// Handle keyboard event
    pub fn handle_standard_key_event(&self, key: &Key) -> Result<()> {
        if let Some(keycode) = EGUI_TO_ANDROID_KEY.get(key) {
            self.adb_shell
                .send_input(&AdbAction::Key { keycode: *keycode })?;
        }
        Ok(())
    }

    pub fn handle_keycombo_event(&self, modifiers: Modifiers, key: &Key) -> Result<()> {
        if let Some(keycode) = EGUI_TO_ANDROID_KEY.get(key) {
            self.adb_shell.send_input(&AdbAction::KeyCombo {
                modifiers,
                keycode: *keycode,
            })?;
        }
        Ok(())
    }

    /// Handle custom keyboard event
    pub fn handle_custom_keymapping_event(&self, key: &Key, pressed: bool) -> Result<()> {
        let active_profile = self.active_profile.lock();
        if active_profile.is_none() {
            return Ok(());
        }

        // TODO: handle key hold actions
        if !pressed {
            return Ok(());
        }

        active_profile
            .as_ref()
            .unwrap()
            .mappings
            .get(key)
            .map(|action| self.adb_shell.send_input(action))
            .or_else(|| Some(self.handle_standard_key_event(key)))
            .transpose()?;

        Ok(())
    }

    /// Get list of available profiles
    pub fn get_avail_profiles(&self) -> Vec<String> {
        let avail_profiles = self.avail_profiles.lock();
        avail_profiles.iter().map(|p| p.name.clone()).collect()
    }

    /// Get number of profiles
    pub fn get_avail_profiles_count(&self) -> usize {
        let avail_profiles = self.avail_profiles.lock();
        avail_profiles.len()
    }
}
