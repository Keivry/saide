pub mod log;
pub mod mapping;
pub mod scrcpy;

use {
    crate::config::{log::LogConfig, mapping::MappingConfig, scrcpy::ScrcpyConfig},
    anyhow::{Result, anyhow},
    std::path::Path,
};

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub scrcpy: ScrcpyConfig,
    pub video: VideoConfig,
    #[allow(dead_code)]
    pub keymapping: MappingConfig,
    pub logging: LogConfig,
}

#[derive(Debug, Clone)]
pub struct VideoConfig {
    #[allow(dead_code)]
    pub vsync: bool,
    pub backend: String,
}

impl AppConfig {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(anyhow!("Config file not found: {:?}", path));
        }

        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow!("Failed to read config file: {}", e))?;

        let config: toml::Value =
            toml::from_str(&content).map_err(|e| anyhow!("Failed to parse config: {}", e))?;

        Self::from_toml_value(config)
    }

    pub fn from_toml_value(value: toml::Value) -> Result<Self> {
        let scrcpy = value
            .get("scrcpy")
            .and_then(|v| ScrcpyConfig::from_toml_value(v.clone()).ok())
            .unwrap_or_default();

        let video = value
            .get("video")
            .map(|v| VideoConfig {
                vsync: v.get("vsync").and_then(|x| x.as_bool()).unwrap_or(true),
                backend: v
                    .get("backend")
                    .and_then(|x| x.as_str())
                    .unwrap_or("VULKAN")
                    .to_string(),
            })
            .unwrap_or_default();

        let keymapping = value
            .get("kaymapping")
            .or_else(|| value.get("keymapping"))
            .and_then(|v| MappingConfig::from_toml_value(v.clone()).ok())
            .unwrap_or_default();

        let logging = value
            .get("logging")
            .and_then(|v| LogConfig::from_toml_value(v.clone()).ok())
            .unwrap_or_default();

        Ok(Self {
            scrcpy,
            video,
            keymapping,
            logging,
        })
    }
}

impl Default for VideoConfig {
    fn default() -> Self {
        Self {
            vsync: true,
            backend: "VULKAN".to_string(),
        }
    }
}
