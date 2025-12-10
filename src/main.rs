mod app;
mod config;
mod controller;
mod scrcpy;
mod v4l2;

use {
    anyhow::anyhow,
    app::ui::{SAideApp, Toolbar},
    config::ConfigManager,
    eframe::{egui, egui_wgpu},
    tracing::info,
    tracing_subscriber::{EnvFilter, fmt, prelude::*},
};

const CONFIG_PATH: &str = "config.toml";
const WGPU_LOG_LEVEL: &str = "error";
const DEFAULT_WIDTH: u32 = 1280;
const DEFAULT_HEIGHT: u32 = 720;

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

    start_ui(config_manager)
}

fn start_ui(config_manager: ConfigManager) -> anyhow::Result<()> {
    let toolbar_width = Toolbar::width();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("SAide")
            .with_inner_size([DEFAULT_WIDTH as f32 + toolbar_width, DEFAULT_HEIGHT as f32]),
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: egui_wgpu::WgpuConfiguration {
            present_mode: if config_manager.config().gpu.vsync {
                wgpu::PresentMode::AutoVsync
            } else {
                wgpu::PresentMode::AutoNoVsync
            },

            wgpu_setup: egui_wgpu::WgpuSetup::from(egui_wgpu::WgpuSetupCreateNew {
                instance_descriptor: wgpu::InstanceDescriptor {
                    backends: (&config_manager.config().gpu.backend).into(),
                    ..Default::default()
                },
                ..Default::default()
            }),

            desired_maximum_frame_latency: Some(1),

            ..Default::default()
        },
        ..Default::default()
    };

    eframe::run_native(
        "SAide",
        options,
        Box::new(move |cc| Ok(Box::new(SAideApp::new(cc, config_manager)))),
    )
    .map_err(|e| anyhow!("eframe error: {}", e))
}
