pub mod log;
pub mod mapping;
pub mod scrcpy;

use {
    crate::config::{log::LogConfig, mapping::Mappings, scrcpy::ScrcpyConfig},
    anyhow::Result,
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
}

fn default_true() -> bool { true }
fn default_init_timeout() -> u32 { 15 }

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
        Ok(toml::from_str(&content)?)
    }

    /// Save configuration to file
    pub fn save(&self, path: &str) -> Result<()> {
        let content = toml::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }
}

/// Configuration file manager
pub struct ConfigManager {
    path: String,
    config: Arc<SAideConfig>,
}

impl ConfigManager {
    pub fn new() -> Result<Self> {
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
