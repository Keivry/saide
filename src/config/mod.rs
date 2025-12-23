//! Configuration management for SAide application
//!
//! This module defines the configuration structures and management for the SAide application,
//! including loading and saving configuration files, as well as default values.

pub mod log;
pub mod mapping;
pub mod scrcpy;

use {
    crate::{
        config::{log::LogConfig, mapping::Mappings, scrcpy::ScrcpyConfig},
        constant::SCRCPY_SERVER_VERSION_STRING,
        error::{Result, SAideError},
    },
    directories::ProjectDirs,
    lazy_static::lazy_static,
    serde::{Deserialize, Serialize},
    std::{
        fmt::{self, Display},
        fs,
        path::Path,
        sync::Arc,
    },
};

lazy_static! {
    /// Default configuration file path
    /// If the standard config directory cannot be determined, falls back to "config.toml" in the current directory.
    /// E.g., on Linux, this would typically be "~/.config/saide/config.toml"
    /// on Windows, it would be "C:\Users\<User>\AppData\Roaming\saide\config.toml"
    static ref DEFAULT_CONFIG_PATH: String = match ProjectDirs::from("io", "keivry", "saide") {
        Some(proj_dirs) => proj_dirs
            .config_dir()
            .join("config.toml")
            .to_str()
            .unwrap()
            .to_string(),
        None => "config.toml".to_string(),
    };
}

/// Position of the indicator on the screen
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum IndicatorPosition {
    #[default]
    #[serde(rename = "top-left")]
    TopLeft,
    #[serde(rename = "top-right")]
    TopRight,
    #[serde(rename = "bottom-left")]
    BottomLeft,
    #[serde(rename = "bottom-right")]
    BottomRight,
}

/// GPU backend options
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

    #[serde(default = "default_true")]
    pub indicator: bool,

    #[serde(default)]
    pub indicator_position: IndicatorPosition,

    /// Path to the scrcpy server file, if not set, uses the built-in version
    /// Defaults to "scrcpy-server-<version>" in the user data directory if available
    /// otherwise falls back to the filename in the current directory
    #[serde(default = "default_scrcpy_server_path")]
    pub scrcpy_server: String,
}

fn default_true() -> bool { true }
fn default_init_timeout() -> u32 { 15 }
fn default_scrcpy_server_path() -> String {
    let scrcpy_server = format!("scrcpy-server-{}", SCRCPY_SERVER_VERSION_STRING);
    if let Some(dir) = ProjectDirs::from("io", "keivry", "saide") {
        let path = dir.data_dir().join(scrcpy_server.as_str());
        if path.is_file() {
            return path.to_str().unwrap().to_string();
        }
    }

    scrcpy_server
}

/// Main configuration structure
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SAideConfig {
    pub general: GeneralConfig,
    pub scrcpy: Arc<ScrcpyConfig>,
    pub gpu: GPUConfig,
    pub mappings: Arc<Mappings>,
    pub logging: LogConfig,
}

impl SAideConfig {
    /// Load configuration from file
    pub fn load(path: &str) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        toml::from_str(&content)
            .map_err(|e| SAideError::Config(format!("Failed to parse config file {}: {}", path, e)))
    }

    /// Save configuration to file
    pub fn save(&self, path: &str) -> Result<()> {
        let content = toml::to_string_pretty(self).map_err(|e| {
            SAideError::Config(format!("Failed to serialize config for {}: {}", path, e))
        })?;
        fs::write(path, content)
            .map_err(|e| SAideError::Io(format!("Failed to write config file {}: {}", path, e)))?;
        Ok(())
    }
}

/// Configuration manager
/// Handles loading, saving, and providing access to the configuration file.
pub struct ConfigManager {
    path: String,
    config: Arc<SAideConfig>,
}

impl ConfigManager {
    /// Create a new ConfigManager, loading existing config or using defaults
    pub fn new() -> Result<Self> {
        // Determine which config file to load
        // 1. Check if the default config path exists
        // 2. If not, check if "config.toml" exists in the current directory
        // 3. If neither exists, use default config values
        let (path, config) = if Path::new(DEFAULT_CONFIG_PATH.as_str()).is_file() {
            (
                DEFAULT_CONFIG_PATH.clone(),
                SAideConfig::load(&DEFAULT_CONFIG_PATH)?,
            )
        } else if Path::new("config.toml").is_file() {
            ("config.toml".to_string(), SAideConfig::load("config.toml")?)
        } else {
            (DEFAULT_CONFIG_PATH.clone(), SAideConfig::default())
        };

        Ok(Self {
            path,
            config: Arc::new(config),
        })
    }

    pub fn config(&self) -> Arc<SAideConfig> { Arc::clone(&self.config) }

    /// Save configuration
    pub fn save(&self) -> Result<()> { self.config.save(&self.path) }
}
