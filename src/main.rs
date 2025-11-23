mod config;
mod ui;
mod v4l2;

use {
    config::AppConfig,
    eframe::{egui, egui_wgpu},
    std::{sync::Arc, thread},
    ui::VideoApp,
    v4l2::{V4l2Capture, Yu12Frame},
};

const VIDEO_DEVICE: &str = "/dev/video0";
const MAX_FPS: f32 = 60.0;
const CONFIG_PATH: &str = "config.toml";

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("SAide starting...");

    let config = match AppConfig::load_from_file(CONFIG_PATH) {
        Ok(cfg) => {
            log::info!("Loaded configuration from {}", CONFIG_PATH);
            cfg
        }
        Err(e) => {
            log::warn!(
                "Failed to load config from {}: {}, using default config",
                CONFIG_PATH,
                e
            );
            AppConfig::from_toml_value(toml::Value::Table(toml::value::Table::new()))
                .map_err(|e| anyhow::anyhow!("Failed to create default config: {}", e))?
        }
    };

    log::info!("V4L2 device: {}", config.scrcpy.v4l2.device);
    log::info!("Video backend: {}", config.video.backend);
    log::info!("Max FPS: {}", config.scrcpy.video.max_fps);
    log::info!("Logging level: {}", config.logging.level);

    // Channel for frame transfer
    let (tx, rx) = crossbeam_channel::bounded::<Arc<Yu12Frame>>(2);

    let mut capture = match V4l2Capture::new(VIDEO_DEVICE) {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to initialize V4L2 capture");
            return Err(e);
        }
    };

    let (width, height) = capture.dimensions();
    log::info!("Capture started: {}x{}", width, height);

    // Start capture thread
    thread::spawn(move || {
        loop {
            match capture.capture_frame() {
                Ok(frame) => {
                    let _ = tx.try_send(Arc::new(frame));
                }
                Err(e) => {
                    log::error!("Capture error: {}", e);
                    break;
                }
            }
        }
    });

    // Run eframe app
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("SAide")
            .with_inner_size([
                width as f32 + VideoApp::toolbar_width(),
                height as f32 + VideoApp::statusbar_height(),
            ]),
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: egui_wgpu::WgpuConfiguration {
            // Use AutoVsync to reduce CPU/GPU usage
            present_mode: wgpu::PresentMode::AutoVsync,
            // Request low latency for real-time video
            desired_maximum_frame_latency: Some(1),
            ..Default::default()
        },
        ..Default::default()
    };

    eframe::run_native(
        "v4l2play",
        options,
        Box::new(move |cc| Ok(Box::new(VideoApp::new(cc, rx, width, height, MAX_FPS)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {}", e))?;

    Ok(())
}
