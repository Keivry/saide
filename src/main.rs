mod app;
mod config;
mod controller;
mod v4l2;

use {
    app::SAideApp,
    config::SAideConfig,
    tracing::{info, warn},
    tracing_subscriber::{EnvFilter, fmt, prelude::*},
};

const CONFIG_PATH: &str = "config.toml";

fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    info!("SAide starting...");

    let config = match SAideConfig::load_from_file(CONFIG_PATH) {
        Ok(cfg) => {
            info!("Loaded configuration from {}", CONFIG_PATH);
            cfg
        }
        Err(e) => {
            warn!(
                "Failed to load config from {}: {}, using default config",
                CONFIG_PATH, e
            );
            SAideConfig::from_toml_value(toml::Value::Table(toml::value::Table::new()))
                .map_err(|e| anyhow::anyhow!("Failed to create default config: {}", e))?
        }
    };

    info!("V4L2 device: {}", config.scrcpy.v4l2.device);
    info!("Video backend: {}", config.video.backend);
    info!("Max FPS: {}", config.scrcpy.video.max_fps);
    info!("Logging level: {}", config.logging.level);

    // Re-initialize tracing with the configured logging level
    tracing_subscriber::registry()
        .with(EnvFilter::new(config.logging.level.clone()))
        .try_init()
        .ok();

    SAideApp::start(&config)
}
