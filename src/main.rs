mod app;
mod config;
mod controller;
mod v4l2;

use {
    app::SAideApp,
    config::ConfigManager,
    tracing::info,
    tracing_subscriber::{EnvFilter, fmt, prelude::*},
};

const CONFIG_PATH: &str = "config.toml";

const WGPU_LOG_LEVEL: &str = "error";

fn main() -> anyhow::Result<()> {
    let config_manager = ConfigManager::new(CONFIG_PATH)?;

    let config = config_manager.config();
    let level = config.logging.level.as_str();
    tracing_subscriber::registry()
        .with(EnvFilter::new(
            level.to_owned() + ",wgpu_hal=" + {
                match level {
                    "trace" | "debug" => "debug",
                    _ => WGPU_LOG_LEVEL,
                }
            },
        ))
        .with(fmt::layer())
        .init();

    info!("SAide starting...");

    info!("V4L2 device: {}", config.scrcpy.v4l2.device);
    info!("Video backend: {}", config.gpu.backend);
    info!("Max FPS: {}", config.scrcpy.video.max_fps);
    info!("Logging level: {}", config.logging.level);

    SAideApp::start(config_manager)
}
