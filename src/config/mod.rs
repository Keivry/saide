pub mod log;
pub mod mapping;
pub mod scrcpy;

use {
    crate::config::{log::LogConfig, mapping::MappingConfig, scrcpy::ScrcpyConfig},
    anyhow::{Result, anyhow},
    std::{path::Path, sync::Arc},
};

#[derive(Debug, Clone)]
pub struct SAideConfig {
    pub scrcpy: Arc<ScrcpyConfig>,
    pub video: VideoConfig,
    #[allow(dead_code)]
    pub mappings: MappingConfig,
    pub logging: LogConfig,

    pub timeout: u64,
}

#[derive(Debug, Clone)]
pub struct VideoConfig {
    #[allow(dead_code)]
    pub vsync: bool,
    pub backend: String,
}

impl SAideConfig {
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

        let timeout = value
            .get("timeout")
            .and_then(|v| v.as_integer())
            .unwrap_or(30) as u64;

        Ok(Self {
            scrcpy: Arc::new(scrcpy),
            video,
            mappings: keymapping,
            logging,
            timeout,
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
