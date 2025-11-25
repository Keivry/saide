use {
    super::adb::{AdbInputCommand, AdbShell},
    crate::config::mapping::MappingConfig,
    anyhow::Result,
    std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    },
    tracing::{error, info},
};

/// Keyboard mapping state
pub struct KeyboardMapper {
    config: MappingConfig,
    adb: AdbShell,
    enabled: Arc<Mutex<bool>>,
    active_profile: Arc<Mutex<usize>>,
    /// Cache of keycode mappings for performance
    keycode_cache: HashMap<String, String>,
}

impl KeyboardMapper {
    /// Create a new keyboard mapper
    pub fn new(config: MappingConfig) -> Self {
        let initial_state = config.initial_state;
        let mut keycode_cache = HashMap::new();
        keycode_cache.insert("KEY_A".to_string(), "a".to_string());
        keycode_cache.insert("KEY_B".to_string(), "b".to_string());
        keycode_cache.insert("KEY_C".to_string(), "c".to_string());
        keycode_cache.insert("KEY_D".to_string(), "d".to_string());
        keycode_cache.insert("KEY_E".to_string(), "e".to_string());
        keycode_cache.insert("KEY_F".to_string(), "f".to_string());
        keycode_cache.insert("KEY_G".to_string(), "g".to_string());
        keycode_cache.insert("KEY_H".to_string(), "h".to_string());
        keycode_cache.insert("KEY_I".to_string(), "i".to_string());
        keycode_cache.insert("KEY_J".to_string(), "j".to_string());
        keycode_cache.insert("KEY_K".to_string(), "k".to_string());
        keycode_cache.insert("KEY_L".to_string(), "l".to_string());
        keycode_cache.insert("KEY_M".to_string(), "m".to_string());
        keycode_cache.insert("KEY_N".to_string(), "n".to_string());
        keycode_cache.insert("KEY_O".to_string(), "o".to_string());
        keycode_cache.insert("KEY_P".to_string(), "p".to_string());
        keycode_cache.insert("KEY_Q".to_string(), "q".to_string());
        keycode_cache.insert("KEY_R".to_string(), "r".to_string());
        keycode_cache.insert("KEY_S".to_string(), "s".to_string());
        keycode_cache.insert("KEY_T".to_string(), "t".to_string());
        keycode_cache.insert("KEY_U".to_string(), "u".to_string());
        keycode_cache.insert("KEY_V".to_string(), "v".to_string());
        keycode_cache.insert("KEY_W".to_string(), "w".to_string());
        keycode_cache.insert("KEY_X".to_string(), "x".to_string());
        keycode_cache.insert("KEY_Y".to_string(), "y".to_string());
        keycode_cache.insert("KEY_Z".to_string(), "z".to_string());

        keycode_cache.insert("KEY_0".to_string(), "0".to_string());
        keycode_cache.insert("KEY_1".to_string(), "1".to_string());
        keycode_cache.insert("KEY_2".to_string(), "2".to_string());
        keycode_cache.insert("KEY_3".to_string(), "3".to_string());
        keycode_cache.insert("KEY_4".to_string(), "4".to_string());
        keycode_cache.insert("KEY_5".to_string(), "5".to_string());
        keycode_cache.insert("KEY_6".to_string(), "6".to_string());
        keycode_cache.insert("KEY_7".to_string(), "7".to_string());
        keycode_cache.insert("KEY_8".to_string(), "8".to_string());
        keycode_cache.insert("KEY_9".to_string(), "9".to_string());

        keycode_cache.insert("KEY_SPACE".to_string(), "SPACE".to_string());
        keycode_cache.insert("KEY_ENTER".to_string(), "ENTER".to_string());
        keycode_cache.insert("KEY_TAB".to_string(), "TAB".to_string());
        keycode_cache.insert("KEY_BACKSPACE".to_string(), "DEL".to_string());
        keycode_cache.insert("KEY_ESC".to_string(), "ESCAPE".to_string());
        keycode_cache.insert("KEY_F1".to_string(), "F1".to_string());
        keycode_cache.insert("KEY_F2".to_string(), "F2".to_string());
        keycode_cache.insert("KEY_F3".to_string(), "F3".to_string());
        keycode_cache.insert("KEY_F4".to_string(), "F4".to_string());
        keycode_cache.insert("KEY_GRAVE".to_string(), "GRAVE".to_string());

        Self {
            config,
            adb: AdbShell::new(),
            enabled: Arc::new(Mutex::new(initial_state)),
            active_profile: Arc::new(Mutex::new(0)),
            keycode_cache,
        }
    }

    /// Enable or disable keyboard mapping
    pub fn set_enabled(&self, enabled: bool) {
        let mut enabled_lock = self.enabled.lock().unwrap();
        *enabled_lock = enabled;
        info!(
            "Keyboard mapping {}",
            if enabled { "enabled" } else { "disabled" }
        );
    }

    /// Check if keyboard mapping is enabled
    pub fn is_enabled(&self) -> bool {
        let enabled_lock = self.enabled.lock().unwrap();
        *enabled_lock
    }

    /// Set active profile by index
    pub fn set_active_profile(&self, index: usize) {
        if index < self.config.profiles.len() {
            let mut active_lock = self.active_profile.lock().unwrap();
            *active_lock = index;
            info!(
                "Active profile set to: {}",
                self.config.profiles[index].name
            );
        }
    }

    /// Get active profile name
    pub fn get_active_profile_name(&self) -> String {
        let active_lock = self.active_profile.lock().unwrap();
        let profile_index = *active_lock;
        if profile_index < self.config.profiles.len() {
            self.config.profiles[profile_index].name.clone()
        } else {
            "Unknown".to_string()
        }
    }

    /// Handle keyboard event
    pub fn handle_key_event(&self, keycode: &str, pressed: bool) -> Result<()> {
        if !self.is_enabled() || !pressed {
            return Ok(());
        }

        let active_lock = self.active_profile.lock().unwrap();
        let profile_index = *active_lock;

        if profile_index >= self.config.profiles.len() {
            error!("Invalid profile index: {}", profile_index);
            return Ok(());
        }

        let profile = &self.config.profiles[profile_index];

        // Find matching key mapping
        for mapping in &profile.mappings {
            if mapping.key == keycode {
                info!(
                    "Key pressed: {} -> {} at ({}, {})",
                    keycode, mapping.action, mapping.pos.x, mapping.pos.y
                );

                match mapping.action.as_str() {
                    "TAP" => {
                        // Convert normalized position to actual coordinates
                        let android_x = (mapping.pos.x * 1080.0).round() as i32;
                        let android_y = (mapping.pos.y * 1920.0).round() as i32;

                        if let Err(e) = self.adb.lock().unwrap().send_input(AdbInputCommand::Tap {
                            x: android_x,
                            y: android_y,
                        }) {
                            error!("Failed to send tap command: {}", e);
                        }
                    }
                    "KEY" => {
                        // Handle special key events
                        let android_keycode = self
                            .keycode_cache
                            .get(keycode)
                            .map(|s| s.as_str())
                            .unwrap_or(keycode)
                            .to_string();
                        if let Err(e) = self.adb.lock().unwrap().send_input(AdbInputCommand::Key {
                            keycode: android_keycode,
                        }) {
                            error!("Failed to send key command: {}", e);
                        }
                    }
                    _ => {
                        error!("Unknown keyboard action: {}", mapping.action);
                    }
                }
                break;
            }
        }

        Ok(())
    }

    /// Get list of available profiles
    pub fn get_profiles(&self) -> Vec<String> {
        self.config
            .profiles
            .iter()
            .map(|p| p.name.clone())
            .collect()
    }

    /// Get number of profiles
    pub fn get_profile_count(&self) -> usize { self.config.profiles.len() }
}
