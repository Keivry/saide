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
        constant::{self, SCRCPY_SERVER_VERSION_STRING},
        error::Result,
    },
    directories::ProjectDirs,
    serde::{Deserialize, Serialize},
    std::{
        fmt::{self, Display},
        fs,
        path::{Path, PathBuf},
        sync::Arc,
    },
    tracing::warn,
};

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

/// Input control configuration (mouse, touch, etc)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputConfig {
    /// Long press duration threshold in milliseconds (default: 300ms)
    /// Time the mouse button must be held before triggering long-press
    #[serde(default = "default_long_press_ms")]
    pub long_press_ms: u128,

    /// Drag threshold in pixels (default: 5.0px)
    /// Minimum movement distance to distinguish drag from click
    #[serde(default = "default_drag_threshold")]
    pub drag_threshold_px: f32,

    /// Drag update interval in milliseconds (default: 8ms ≈ 120fps)
    /// Interval for sending touch move events during dragging
    #[serde(default = "default_drag_interval_ms")]
    pub drag_interval_ms: u128,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            long_press_ms: default_long_press_ms(),
            drag_threshold_px: default_drag_threshold(),
            drag_interval_ms: default_drag_interval_ms(),
        }
    }
}

fn default_long_press_ms() -> u128 { 300 }
fn default_drag_threshold() -> f32 { 5.0 }
fn default_drag_interval_ms() -> u128 { 8 }

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct GPUConfig {
    #[serde(default)]
    pub vsync: bool,

    #[serde(default)]
    pub backend: GpuBackend,
}

#[derive(Debug, Serialize, Deserialize)]
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

    /// Default window width in pixels
    #[serde(default = "default_window_width")]
    pub window_width: u32,

    /// Default window height in pixels
    #[serde(default = "default_window_height")]
    pub window_height: u32,

    /// Network bind address for scrcpy server connection
    /// "127.0.0.1" for IPv4 localhost, "[::1]" for IPv6 localhost
    #[serde(default = "default_bind_address")]
    pub bind_address: String,

    /// Path to the scrcpy server file, if not set, uses the built-in version
    /// Defaults to "scrcpy-server-<version>" in the user data directory if available
    /// otherwise falls back to the filename in the current directory
    #[serde(default = "default_scrcpy_server_path")]
    pub scrcpy_server: String,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            keyboard_enabled: default_true(),
            mouse_enabled: default_true(),
            init_timeout: default_init_timeout(),
            indicator: default_true(),
            indicator_position: IndicatorPosition::default(),
            window_width: default_window_width(),
            window_height: default_window_height(),
            bind_address: default_bind_address(),
            scrcpy_server: default_scrcpy_server_path(),
        }
    }
}

fn default_true() -> bool { true }
fn default_init_timeout() -> u32 { 15 }
fn default_window_width() -> u32 { 1280 }
fn default_window_height() -> u32 { 720 }
fn default_bind_address() -> String { "127.0.0.1".to_string() }
fn default_scrcpy_server_path() -> String {
    let scrcpy_server = format!("scrcpy-server-{}", SCRCPY_SERVER_VERSION_STRING);
    if let Some(dir) = ProjectDirs::from("io", "keivry", "saide") {
        let path = dir.data_dir().join(scrcpy_server.as_str());
        if path.is_file() {
            // Use to_string_lossy to handle non-UTF-8 paths on Windows
            return path.to_string_lossy().to_string();
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
    pub input: InputConfig,
    pub mappings: Arc<Mappings>,
    pub logging: LogConfig,
}

impl SAideConfig {
    /// Load configuration from file
    pub fn load<P>(path: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let content = fs::read_to_string(path)?;
        let config: SAideConfig = toml::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    /// Validate configuration values are within acceptable ranges
    pub fn validate(&self) -> Result<()> {
        if !(50..=2000).contains(&self.input.long_press_ms) {
            return Err(crate::error::SAideError::ConfigError(format!(
                "input.long_press_ms ({}) must be 50-2000ms",
                self.input.long_press_ms
            )));
        }

        if !(1.0..=50.0).contains(&self.input.drag_threshold_px) {
            return Err(crate::error::SAideError::ConfigError(format!(
                "input.drag_threshold_px ({}) must be 1.0-50.0px",
                self.input.drag_threshold_px
            )));
        }

        if !(1..=100).contains(&self.input.drag_interval_ms) {
            return Err(crate::error::SAideError::ConfigError(format!(
                "input.drag_interval_ms ({}) must be 1-100ms",
                self.input.drag_interval_ms
            )));
        }

        if !(32..=16384).contains(&self.scrcpy.audio.buffer_frames) {
            return Err(crate::error::SAideError::ConfigError(format!(
                "scrcpy.audio.buffer_frames ({}) must be 32-16384",
                self.scrcpy.audio.buffer_frames
            )));
        }

        if !(1024..=65536).contains(&self.scrcpy.audio.ring_capacity) {
            return Err(crate::error::SAideError::ConfigError(format!(
                "scrcpy.audio.ring_capacity ({}) must be 1024-65536",
                self.scrcpy.audio.ring_capacity
            )));
        }

        Ok(())
    }

    /// Save configuration to file atomically
    ///
    /// Writes to a temporary file first, then atomically renames it to avoid
    /// corruption if interrupted (power loss, ctrl-c, etc).
    pub fn save<P>(&self, path: P) -> Result<()>
    where
        P: AsRef<Path>,
    {
        let path = path.as_ref();
        let content = toml::to_string_pretty(self)?;

        let tmp_path = path.with_extension("toml.tmp");
        fs::write(&tmp_path, content)?;
        fs::rename(&tmp_path, path)?;

        Ok(())
    }
}

/// Configuration manager
/// Handles loading, saving, and providing access to the configuration file.
pub struct ConfigManager {
    path: PathBuf,
    config: Arc<SAideConfig>,
}

impl ConfigManager {
    /// Create a new ConfigManager, loading existing config or using defaults
    pub fn new() -> Result<Self> {
        // Determine which config file to load
        // 1. Try standard config path (user config dir)
        // 2. Fallback to ./config.toml in current directory
        // 3. Fallback to temp directory if ProjectDirs unavailable
        // 4. If none exist, create default config
        let path = constant::config_dir().unwrap_or_else(|| {
            warn!(
                "Unable to determine config directory, using fallback: {:?}",
                constant::fallback_config_path()
            );
            constant::fallback_config_path()
        });

        let config = if path.is_file() {
            SAideConfig::load(&path)?
        } else if PathBuf::from("config.toml").is_file() {
            SAideConfig::load("config.toml")?
        } else {
            let config = SAideConfig::default();

            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            config.save(&path)?;

            config
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
