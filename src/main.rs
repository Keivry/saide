use {
    crossbeam_channel::Receiver,
    eframe::{egui, egui_wgpu},
    saide::{
        app::ui::{SAideApp, Toolbar},
        config::ConfigManager,
        controller::AdbShell,
        error::{Result, SAideError},
        t,
        t_args,
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

    info!("{}", t!("app-starting"));

    info!(
        "{}",
        t_args!("config-video-backend", "backend" => config.gpu.backend.to_string())
    );
    info!(
        "{}",
        t_args!("config-max-video-size", "size" => config.scrcpy.video.max_size.to_string())
    );
    info!(
        "{}",
        t_args!("config-max-fps", "fps" => config.scrcpy.video.max_fps.to_string())
    );
    info!(
        "{}",
        t_args!("config-logging-level", "level" => config.logging.level.as_str())
    );

    let (tx, rx) = crossbeam_channel::bounded(1);
    let tx_clone = tx.clone();
    ctrlc::set_handler(move || {
        info!("{}", t!("ctrlc-received"));
        let _ = tx_clone.send(());
    })
    .map_err(|e| SAideError::Other(t_args!("ctrlc-handler-failed", "error" => e.to_string())))?;

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
