// SPDX-License-Identifier: MIT OR Apache-2.0

//! Configuration management for SAide application
//!
//! This module defines the configuration structures and management for the SAide application,
//! including loading and saving configuration files, as well as default values.

pub mod log;
pub mod mapping;
pub mod scrcpy;

use {
    crate::{
        config::{log::LogConfig, mapping::MappingsConfig, scrcpy::ScrcpyConfig},
        constant::{self},
        error::{Result, SAideError},
    },
    directories::UserDirs,
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
    pub long_press_ms: u64,

    /// Drag threshold in pixels (default: 5.0px)
    /// Minimum movement distance to distinguish drag from click
    #[serde(default = "default_drag_threshold")]
    pub drag_threshold_px: f32,

    /// Drag update interval in milliseconds (default: 8ms ≈ 120fps)
    /// Interval for sending touch move events during dragging
    #[serde(default = "default_drag_interval_ms")]
    pub drag_interval_ms: u64,
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

fn default_long_press_ms() -> u64 { 300 }
fn default_drag_threshold() -> f32 { 5.0 }
fn default_drag_interval_ms() -> u64 { 8 }

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct GPUConfig {
    #[serde(default)]
    pub vsync: bool,

    #[serde(default)]
    pub backend: GpuBackend,

    /// Enable hardware decoding (VAAPI/NVDEC)
    /// true: auto-detect and use hardware decoder (default)
    /// false: force software decoder
    #[serde(default = "default_true")]
    pub hwdecode: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_true")]
    pub keyboard_enabled: bool,

    #[serde(default = "default_true")]
    pub mouse_enabled: bool,

    #[serde(default)]
    pub auto_hide_toolbar: bool,

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

    /// Enable intelligent window resizing when video exceeds screen bounds
    /// If true, automatically scales down window size using preset resolution tiers
    /// If false, uses video native resolution (may be constrained by WM)
    #[serde(default = "default_true")]
    pub smart_window_resize: bool,

    /// Network bind address for scrcpy server connection
    /// "127.0.0.1" for IPv4 localhost, "[::1]" for IPv6 localhost
    #[serde(default = "default_bind_address")]
    pub bind_address: String,

    /// Path to the scrcpy server file, if not set, uses the built-in version
    /// Defaults to "scrcpy-server-<version>" in the user data directory if available
    /// otherwise falls back to the filename in the current directory
    #[serde(default = "default_scrcpy_server_path")]
    pub scrcpy_server: String,

    /// Directory where screenshots are saved.
    /// Resolved from the platform's configured pictures directory (e.g.
    /// `XDG_PICTURES_DIR` on Linux, `My Pictures` on Windows) with a `saide`
    /// subdirectory appended; falls back to `$HOME/Pictures/saide` if the
    /// platform pictures directory cannot be determined.
    #[serde(default = "default_screenshot_path")]
    pub screenshot_path: String,

    /// Directory where screen recordings are saved.
    /// Resolved from the platform's configured videos directory (e.g.
    /// `XDG_VIDEOS_DIR` on Linux, `My Videos` on Windows) with a `saide`
    /// subdirectory appended; falls back to `$HOME/Videos/saide` if the
    /// platform videos directory cannot be determined.
    #[serde(default = "default_recording_path")]
    pub recording_path: String,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            keyboard_enabled: default_true(),
            mouse_enabled: default_true(),
            auto_hide_toolbar: false,
            init_timeout: default_init_timeout(),
            indicator: default_true(),
            indicator_position: IndicatorPosition::default(),
            window_width: default_window_width(),
            window_height: default_window_height(),
            smart_window_resize: default_true(),
            bind_address: default_bind_address(),
            scrcpy_server: default_scrcpy_server_path(),
            screenshot_path: default_screenshot_path(),
            recording_path: default_recording_path(),
        }
    }
}

fn default_true() -> bool { true }
fn default_init_timeout() -> u32 { 15 }
fn default_window_width() -> u32 { 1280 }
fn default_window_height() -> u32 { 720 }
fn default_bind_address() -> String { "127.0.0.1".to_string() }
fn default_scrcpy_server_path() -> String {
    constant::resolve_scrcpy_server_path()
        .to_string_lossy()
        .to_string()
}

fn default_screenshot_path() -> String {
    UserDirs::new()
        .and_then(|d| d.picture_dir().map(|p| p.join("saide")))
        .unwrap_or_else(|| dirs_fallback_home().join("Pictures").join("saide"))
        .to_string_lossy()
        .to_string()
}

fn default_recording_path() -> String {
    UserDirs::new()
        .and_then(|d| d.video_dir().map(|p| p.join("saide")))
        .unwrap_or_else(|| dirs_fallback_home().join("Videos").join("saide"))
        .to_string_lossy()
        .to_string()
}

fn dirs_fallback_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Main configuration structure
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SAideConfig {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub scrcpy: Arc<ScrcpyConfig>,
    #[serde(default)]
    pub gpu: GPUConfig,
    #[serde(default)]
    pub input: InputConfig,
    #[serde(default)]
    pub mappings: Arc<MappingsConfig>,
    #[serde(default)]
    pub logging: LogConfig,
}

impl SAideConfig {
    /// Load configuration from file
    pub fn load<P>(path: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let content = fs::read_to_string(path)
            .map_err(|e| SAideError::ConfigError(format!("Failed to read config file: {}", e)))?;
        let config: SAideConfig = toml::from_str(&content)
            .map_err(|e| SAideError::ConfigError(format!("Failed to parse config file: {}", e)))?;
        config.validate()?;
        Ok(config)
    }

    /// Validate configuration values are within acceptable ranges
    pub fn validate(&self) -> Result<()> {
        if !(1..=300).contains(&self.general.init_timeout) {
            return Err(SAideError::ConfigError(format!(
                "general.init_timeout ({}) must be 1-300 seconds",
                self.general.init_timeout
            )));
        }

        if !(320..=7680).contains(&self.general.window_width) {
            return Err(SAideError::ConfigError(format!(
                "general.window_width ({}) must be 320-7680 pixels",
                self.general.window_width
            )));
        }

        if !(240..=4320).contains(&self.general.window_height) {
            return Err(SAideError::ConfigError(format!(
                "general.window_height ({}) must be 240-4320 pixels",
                self.general.window_height
            )));
        }

        if !(1..=240).contains(&self.scrcpy.video.min_fps) {
            return Err(SAideError::ConfigError(format!(
                "scrcpy.video.min_fps ({}) must be 1-240",
                self.scrcpy.video.min_fps
            )));
        }

        if !(1..=240).contains(&self.scrcpy.video.max_fps) {
            return Err(SAideError::ConfigError(format!(
                "scrcpy.video.max_fps ({}) must be 1-240",
                self.scrcpy.video.max_fps
            )));
        }

        if self.scrcpy.video.min_fps > self.scrcpy.video.max_fps {
            return Err(SAideError::ConfigError(format!(
                "scrcpy.video.min_fps ({}) must be <= scrcpy.video.max_fps ({})",
                self.scrcpy.video.min_fps, self.scrcpy.video.max_fps
            )));
        }

        if !(100..=4096).contains(&self.scrcpy.video.max_size) {
            return Err(SAideError::ConfigError(format!(
                "scrcpy.video.max_size ({}) must be 100-4096 pixels",
                self.scrcpy.video.max_size
            )));
        }

        if !(50..=2000).contains(&self.input.long_press_ms) {
            return Err(SAideError::ConfigError(format!(
                "input.long_press_ms ({}) must be 50-2000ms",
                self.input.long_press_ms
            )));
        }

        if !(1.0..=50.0).contains(&self.input.drag_threshold_px) {
            return Err(SAideError::ConfigError(format!(
                "input.drag_threshold_px ({}) must be 1.0-50.0px",
                self.input.drag_threshold_px
            )));
        }

        if !(1..=100).contains(&self.input.drag_interval_ms) {
            return Err(SAideError::ConfigError(format!(
                "input.drag_interval_ms ({}) must be 1-100ms",
                self.input.drag_interval_ms
            )));
        }

        if !(32..=16384).contains(&self.scrcpy.audio.buffer_frames) {
            return Err(SAideError::ConfigError(format!(
                "scrcpy.audio.buffer_frames ({}) must be 32-16384",
                self.scrcpy.audio.buffer_frames
            )));
        }

        if !(1024..=65536).contains(&self.scrcpy.audio.ring_capacity) {
            return Err(SAideError::ConfigError(format!(
                "scrcpy.audio.ring_capacity ({}) must be 1024-65536",
                self.scrcpy.audio.ring_capacity
            )));
        }

        let screenshot_dir = PathBuf::from(&self.general.screenshot_path);
        if !screenshot_dir.exists() {
            fs::create_dir_all(&screenshot_dir).map_err(|e| {
                SAideError::ConfigError(format!(
                    "Failed to create screenshot directory {:?}: {}",
                    screenshot_dir, e
                ))
            })?;
        }

        let recording_dir = PathBuf::from(&self.general.recording_path);
        if !recording_dir.exists() {
            fs::create_dir_all(&recording_dir).map_err(|e| {
                SAideError::ConfigError(format!(
                    "Failed to create recording directory {:?}: {}",
                    recording_dir, e
                ))
            })?;
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
    degraded: bool,
}

impl ConfigManager {
    /// Create a new ConfigManager, loading existing config or using defaults
    pub fn new() -> Result<Self> {
        let config_file = constant::config_file();

        let config = if config_file.is_file() {
            SAideConfig::load(&config_file)?
        } else if PathBuf::from("config.toml").is_file() {
            SAideConfig::load("config.toml")?
        } else {
            let config = SAideConfig::default();

            if let Some(parent) = config_file.parent() {
                fs::create_dir_all(parent)?;
            }
            config.save(&config_file)?;

            config
        };

        Ok(Self {
            path: config_file,
            config: Arc::new(config),
            degraded: false,
        })
    }

    /// Create a ConfigManager, falling back to built-in defaults if config loading fails.
    ///
    /// Returns `(ConfigManager, Option<error_string>)`. On failure the returned manager
    /// is marked as degraded (`is_degraded() == true`): callers must check `is_degraded()`
    /// before calling `save()` to avoid overwriting a corrupt config with default values.
    pub fn new_or_default() -> (Self, Option<String>) {
        match Self::new() {
            Ok(cm) => (cm, None),
            Err(e) => {
                warn!("Failed to load config, using defaults: {}", e);
                let config_file = constant::config_file();
                (
                    Self {
                        path: config_file,
                        config: Arc::new(SAideConfig::default()),
                        degraded: true,
                    },
                    Some(e.to_string()),
                )
            }
        }
    }

    pub fn config(&self) -> Arc<SAideConfig> { Arc::clone(&self.config) }

    pub fn is_degraded(&self) -> bool { self.degraded }

    /// Save configuration
    pub fn save(&self) -> Result<()> { self.config.save(&self.path) }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::config::scrcpy::{AudioConfig, VideoConfig},
    };

    #[test]
    fn test_default_config_validates() {
        let config = SAideConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_init_timeout_validation() {
        let mut config = SAideConfig::default();

        config.general.init_timeout = 0;
        assert!(config.validate().is_err());

        config.general.init_timeout = 1;
        assert!(config.validate().is_ok());

        config.general.init_timeout = 300;
        assert!(config.validate().is_ok());

        config.general.init_timeout = 301;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_window_size_validation() {
        let mut config = SAideConfig::default();

        config.general.window_width = 319;
        assert!(config.validate().is_err());

        config.general.window_width = 320;
        assert!(config.validate().is_ok());

        config.general.window_width = 7680;
        assert!(config.validate().is_ok());

        config.general.window_width = 7681;
        assert!(config.validate().is_err());

        config.general.window_width = 1280;
        config.general.window_height = 239;
        assert!(config.validate().is_err());

        config.general.window_height = 240;
        assert!(config.validate().is_ok());

        config.general.window_height = 4320;
        assert!(config.validate().is_ok());

        config.general.window_height = 4321;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_video_config_validation() {
        let mut config = SAideConfig {
            scrcpy: Arc::new(ScrcpyConfig {
                video: VideoConfig {
                    min_fps: 0,
                    max_fps: 0,
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(config.validate().is_err());

        config.scrcpy = Arc::new(ScrcpyConfig {
            video: VideoConfig {
                min_fps: 1,
                max_fps: 1,
                ..Default::default()
            },
            ..Default::default()
        });
        assert!(config.validate().is_ok());

        config.scrcpy = Arc::new(ScrcpyConfig {
            video: VideoConfig {
                min_fps: 60,
                max_fps: 240,
                ..Default::default()
            },
            ..Default::default()
        });
        assert!(config.validate().is_ok());

        config.scrcpy = Arc::new(ScrcpyConfig {
            video: VideoConfig {
                min_fps: 60,
                max_fps: 241,
                ..Default::default()
            },
            ..Default::default()
        });
        assert!(config.validate().is_err());

        config.scrcpy = Arc::new(ScrcpyConfig {
            video: VideoConfig {
                min_fps: 61,
                max_fps: 60,
                ..Default::default()
            },
            ..Default::default()
        });
        assert!(config.validate().is_err());

        config.scrcpy = Arc::new(ScrcpyConfig {
            video: VideoConfig {
                min_fps: 5,
                max_fps: 60,
                max_size: 99,
                ..Default::default()
            },
            ..Default::default()
        });
        assert!(config.validate().is_err());

        config.scrcpy = Arc::new(ScrcpyConfig {
            video: VideoConfig {
                min_fps: 5,
                max_fps: 60,
                max_size: 100,
                ..Default::default()
            },
            ..Default::default()
        });
        assert!(config.validate().is_ok());

        config.scrcpy = Arc::new(ScrcpyConfig {
            video: VideoConfig {
                min_fps: 5,
                max_fps: 60,
                max_size: 4096,
                ..Default::default()
            },
            ..Default::default()
        });
        assert!(config.validate().is_ok());

        config.scrcpy = Arc::new(ScrcpyConfig {
            video: VideoConfig {
                min_fps: 5,
                max_fps: 60,
                max_size: 4097,
                ..Default::default()
            },
            ..Default::default()
        });
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_input_config_validation() {
        let mut config = SAideConfig::default();

        config.input.long_press_ms = 49;
        assert!(config.validate().is_err());

        config.input.long_press_ms = 50;
        assert!(config.validate().is_ok());

        config.input.long_press_ms = 2000;
        assert!(config.validate().is_ok());

        config.input.long_press_ms = 2001;
        assert!(config.validate().is_err());

        config.input.long_press_ms = 300;
        config.input.drag_threshold_px = 0.9;
        assert!(config.validate().is_err());

        config.input.drag_threshold_px = 1.0;
        assert!(config.validate().is_ok());

        config.input.drag_threshold_px = 50.0;
        assert!(config.validate().is_ok());

        config.input.drag_threshold_px = 50.1;
        assert!(config.validate().is_err());

        config.input.drag_threshold_px = 5.0;
        config.input.drag_interval_ms = 0;
        assert!(config.validate().is_err());

        config.input.drag_interval_ms = 1;
        assert!(config.validate().is_ok());

        config.input.drag_interval_ms = 100;
        assert!(config.validate().is_ok());

        config.input.drag_interval_ms = 101;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_audio_config_validation() {
        let mut config = SAideConfig {
            scrcpy: Arc::new(ScrcpyConfig {
                audio: AudioConfig {
                    buffer_frames: 31,
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(config.validate().is_err());

        config.scrcpy = Arc::new(ScrcpyConfig {
            audio: AudioConfig {
                buffer_frames: 32,
                ..Default::default()
            },
            ..Default::default()
        });
        assert!(config.validate().is_ok());

        config.scrcpy = Arc::new(ScrcpyConfig {
            audio: AudioConfig {
                buffer_frames: 16384,
                ..Default::default()
            },
            ..Default::default()
        });
        assert!(config.validate().is_ok());

        config.scrcpy = Arc::new(ScrcpyConfig {
            audio: AudioConfig {
                buffer_frames: 16385,
                ..Default::default()
            },
            ..Default::default()
        });
        assert!(config.validate().is_err());

        config.scrcpy = Arc::new(ScrcpyConfig {
            audio: AudioConfig {
                enabled: true,
                codec: "opus".to_string(),
                source: "playback".to_string(),
                buffer_frames: 64,
                ring_capacity: 1023,
            },
            ..Default::default()
        });
        assert!(config.validate().is_err());

        config.scrcpy = Arc::new(ScrcpyConfig {
            audio: AudioConfig {
                enabled: true,
                codec: "opus".to_string(),
                source: "playback".to_string(),
                buffer_frames: 64,
                ring_capacity: 1024,
            },
            ..Default::default()
        });
        assert!(config.validate().is_ok());

        config.scrcpy = Arc::new(ScrcpyConfig {
            audio: AudioConfig {
                enabled: true,
                codec: "opus".to_string(),
                source: "playback".to_string(),
                buffer_frames: 31,
                ring_capacity: 5760,
            },
            ..Default::default()
        });
        assert!(config.validate().is_err());
    }
}
