use {
    crate::{
        config::mapping::{Key, KeyboardConfig},
        controller::adb::AdbShell,
    },
    anyhow::Result,
    parking_lot::RwLock,
    std::sync::Arc,
    tracing::info,
};

/// Keyboard mapping state
pub struct KeyboardMapper {
    config: Arc<KeyboardConfig>,
    adb_shell: Arc<RwLock<AdbShell>>,
    active_profile: Option<usize>,
}

impl KeyboardMapper {
    /// Create a new keyboard mapper
    pub fn new(config: Arc<KeyboardConfig>, adb_shell: Arc<RwLock<AdbShell>>) -> Self {
        Self {
            config,
            adb_shell,
            active_profile: None,
        }
    }

    /// Set active profile by index
    pub fn set_active_profile(&mut self, index: usize) {
        if index < self.config.profiles.len() {
            self.active_profile = Some(index);
            info!(
                "Active profile set to: {}",
                self.config.profiles[index].name
            );
        }
    }

    /// Get active profile name
    pub fn get_active_profile_name(&self) -> Option<String> {
        self.active_profile
            .map(|idx| self.config.profiles[idx].name.clone())
    }

    /// Handle keyboard event
    pub fn handle_key_event(&self, key: &Key, pressed: bool) -> Result<()> {
        if self.active_profile.is_none() {
            return Ok(());
        }

        // TODO: handle key hold actions
        if !pressed {
            return Ok(());
        }

        self.config.profiles[self.active_profile.unwrap()]
            .mappings
            .get(key)
            .map(|action| self.adb_shell.write().send_input(action));

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
