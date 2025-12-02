pub mod log;
pub mod mapping;
pub mod scrcpy;

use {
    crate::config::{log::LogConfig, mapping::MappingsConfig, scrcpy::ScrcpyConfig},
    serde::{Deserialize, Serialize},
    std::{fmt::Display, sync::Arc},
};

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum GpuBackend {
    #[default]
    Vulkan,
    OpenGL,
}

impl From<&GpuBackend> for wgpu::Backends {
    fn from(backend: &GpuBackend) -> Self {
        match backend {
            GpuBackend::Vulkan => wgpu::Backends::VULKAN,
            GpuBackend::OpenGL => wgpu::Backends::GL,
        }
    }
}

impl Display for GpuBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            GpuBackend::Vulkan => "Vulkan",
            GpuBackend::OpenGL => "OpenGL",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct GPUConfig {
    #[serde(default = "default_vsync")]
    pub vsync: bool,
    #[serde(default = "default_gpu_backend")]
    pub backend: GpuBackend,
}

fn default_vsync() -> bool { true }

fn default_gpu_backend() -> GpuBackend { GpuBackend::Vulkan }

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_true")]
    pub keyboard_enabled: bool,
    #[serde(default = "default_true")]
    pub mouse_enabled: bool,
    #[serde(default = "default_init_timeout")]
    pub init_timeout: u32,
}

fn default_true() -> bool { true }
fn default_init_timeout() -> u32 { 15 }

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SAideConfig {
    pub general: GeneralConfig,
    pub scrcpy: Arc<ScrcpyConfig>,
    pub gpu: GPUConfig,
    pub mappings: Arc<MappingsConfig>,
    pub logging: LogConfig,
}

impl SAideConfig {
    /// Load configuration from file
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    }

    /// Save configuration to file
    pub fn save(&self, path: &str) -> anyhow::Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

/// Configuration file manager
pub struct ConfigManager {
    path: String,
}

impl ConfigManager {
    pub fn new(path: impl Into<String>) -> Self { Self { path: path.into() } }

    /// Load configuration
    pub fn load(&self) -> anyhow::Result<SAideConfig> { SAideConfig::load(&self.path) }

    /// Save profile to configuration file
    pub fn save_profile(&self, profile: &mapping::Profile) -> anyhow::Result<()> {
        use std::io::Write;

        // Read current config file
        let config_str = std::fs::read_to_string(&self.path)?;

        // Parse as TOML value for manipulation
        let mut config_value: toml::Value = toml::from_str(&config_str)?;

        // Find and update the matching profile
        if let Some(profiles) = config_value
            .get_mut("mappings")
            .and_then(|m| m.get_mut("profiles"))
            .and_then(|p| p.as_array_mut())
        {
            for profile_value in profiles.iter_mut() {
                if Self::profile_matches(profile_value, profile) {
                    // Update the mappings array
                    let new_mappings = Self::serialize_mappings(&profile.mappings);
                    profile_value.as_table_mut().map(|t| {
                        t.insert("mappings".to_string(), toml::Value::Array(new_mappings))
                    });
                    break;
                }
            }
        }

        // Write back to file
        let new_config_str = toml::to_string_pretty(&config_value)?;
        let mut file = std::fs::File::create(&self.path)?;
        file.write_all(new_config_str.as_bytes())?;

        Ok(())
    }

    /// Check if a TOML profile value matches the given profile
    fn profile_matches(profile_value: &toml::Value, profile: &mapping::Profile) -> bool {
        let name_match = profile_value
            .get("name")
            .and_then(|n| n.as_str())
            .map(|n| n == profile.name)
            .unwrap_or(false);

        let device_id_match = profile_value
            .get("device_id")
            .and_then(|d| d.as_str())
            .map(|d| d == profile.device_id)
            .unwrap_or(false);

        let rotation_match = profile_value
            .get("rotation")
            .and_then(|r| r.as_integer())
            .map(|r| r as u32 == profile.rotation)
            .unwrap_or(false);

        name_match && device_id_match && rotation_match
    }

    /// Serialize mappings to TOML array
    fn serialize_mappings(mappings: &mapping::KeyMapping) -> Vec<toml::Value> {
        mappings
            .lock()
            .iter()
            .map(|(key, action)| {
                let mut mapping = toml::map::Map::new();
                mapping.insert("key".to_string(), toml::Value::String(format!("{:?}", key)));

                match action {
                    mapping::AdbAction::Tap { x, y } => {
                        mapping
                            .insert("action".to_string(), toml::Value::String("Tap".to_string()));
                        mapping.insert("x".to_string(), toml::Value::Integer(*x as i64));
                        mapping.insert("y".to_string(), toml::Value::Integer(*y as i64));
                    }
                    mapping::AdbAction::TouchDown { x, y } => {
                        mapping.insert(
                            "action".to_string(),
                            toml::Value::String("TouchDown".to_string()),
                        );
                        mapping.insert("x".to_string(), toml::Value::Integer(*x as i64));
                        mapping.insert("y".to_string(), toml::Value::Integer(*y as i64));
                    }
                    mapping::AdbAction::Swipe {
                        x1,
                        y1,
                        x2,
                        y2,
                        duration,
                    } => {
                        mapping.insert(
                            "action".to_string(),
                            toml::Value::String("Swipe".to_string()),
                        );
                        mapping.insert("x1".to_string(), toml::Value::Integer(*x1 as i64));
                        mapping.insert("y1".to_string(), toml::Value::Integer(*y1 as i64));
                        mapping.insert("x2".to_string(), toml::Value::Integer(*x2 as i64));
                        mapping.insert("y2".to_string(), toml::Value::Integer(*y2 as i64));
                        mapping.insert(
                            "duration".to_string(),
                            toml::Value::Integer(*duration as i64),
                        );
                    }
                    _ => {
                        // For other action types, just mark as Ignore
                        mapping.insert(
                            "action".to_string(),
                            toml::Value::String("Ignore".to_string()),
                        );
                    }
                }

                toml::Value::Table(mapping)
            })
            .collect()
    }
}
