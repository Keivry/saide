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
