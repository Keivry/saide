use {
    crossbeam_channel::Receiver,
    eframe::{egui, egui_wgpu},
    egui_cjk_font::load_cjk_font,
    saide::{
        config::ConfigManager,
        controller::AdbShell,
        error::{Result, SAideError},
        saide::ui::{SAideApp, Toolbar},
    },
    tracing::info,
    tracing_subscriber::{EnvFilter, fmt, prelude::*},
};

const WGPU_LOG_LEVEL: &str = "error";

// Default player window size
const DEFAULT_WIDTH: u32 = 1280;
const DEFAULT_HEIGHT: u32 = 720;

fn main() -> Result<()> {
    let config_manager = ConfigManager::new()?;

    let config = config_manager.config();
    let level = config.logging.level.as_str();

    // Build default filter from config, but allow RUST_LOG to override
    // Filter out verbose third-party logs unless explicitly debugging
    let default_filter = format!(
        "{},wgpu_hal={},wgpu_core={},naga={},eframe=info,winit=info,sctk=info,egui_wgpu=info",
        level,
        match level {
            "trace" => "debug",
            _ => WGPU_LOG_LEVEL,
        },
        WGPU_LOG_LEVEL,
        WGPU_LOG_LEVEL
    );

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer())
        .init();

    info!("SAide starting...");

    info!("Video backend: {}", config.gpu.backend.to_string());
    info!(
        "Max video size: {}",
        config.scrcpy.video.max_size.to_string()
    );
    info!("Max FPS: {}", config.scrcpy.video.max_fps.to_string());
    info!("Logging level: {}", config.logging.level.as_str());

    let (tx, rx) = crossbeam_channel::bounded(1);
    let tx_clone = tx.clone();
    ctrlc::set_handler(move || {
        info!("Received Ctrl-C, shutting down...");
        let _ = tx_clone.send(());
    })
    .map_err(|e| SAideError::Other(format!("Failed to set Ctrl-C handler: {}", e)))?;

    let serial = AdbShell::get_device_serial()?;
    start_ui(&serial, config_manager, rx)
}

fn start_ui(serial: &str, config_manager: ConfigManager, shutdown_rx: Receiver<()>) -> Result<()> {
    let toolbar_width = Toolbar::width();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("SAide")
            .with_inner_size([DEFAULT_WIDTH as f32 + toolbar_width, DEFAULT_HEIGHT as f32]),
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: egui_wgpu::WgpuConfiguration {
            // Set present mode based on VSync config
            present_mode: if config_manager.config().gpu.vsync {
                wgpu::PresentMode::AutoVsync
            } else {
                wgpu::PresentMode::AutoNoVsync
            },

            wgpu_setup: egui_wgpu::WgpuSetup::from(egui_wgpu::WgpuSetupCreateNew {
                instance_descriptor: wgpu::InstanceDescriptor {
                    // Select GPU backend from config
                    backends: (&config_manager.config().gpu.backend).into(),
                    ..Default::default()
                },
                ..Default::default()
            }),

            // Set low latency frame pacing
            desired_maximum_frame_latency: Some(1),

            ..Default::default()
        },
        ..Default::default()
    };

    eframe::run_native(
        "SAide",
        options,
        Box::new(move |cc| {
            load_cjk_font(&cc.egui_ctx);
            Ok(Box::new(SAideApp::new(
                cc,
                serial,
                config_manager,
                shutdown_rx,
            )))
        }),
    )
    .map_err(|e| SAideError::UiError(e.to_string()))
}
