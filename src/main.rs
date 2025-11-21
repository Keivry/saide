mod app;
mod v4l2_capture;

use {
    app::VideoApp,
    std::{sync::Arc, thread},
    v4l2_capture::{V4l2Capture, Yu12Frame},
};

const VIDEO_DEVICE: &str = "/dev/video0";
const VIDEO_WIDTH: u32 = 1600;
const VIDEO_HEIGHT: u32 = 720;

fn main() -> eframe::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("v4l2play starting...");

    // Channel for frame transfer
    let (tx, rx) = crossbeam_channel::bounded::<Arc<Yu12Frame>>(2);

    // Start capture thread
    thread::spawn(move || {
        let mut capture = match V4l2Capture::new(VIDEO_DEVICE, VIDEO_WIDTH, VIDEO_HEIGHT) {
            Ok(c) => c,
            Err(e) => {
                log::error!("Failed to open video device: {}", e);
                return;
            }
        };

        log::info!(
            "Capture started: {}x{}",
            capture.dimensions().0,
            capture.dimensions().1
        );

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
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("V4L2 Player")
            .with_inner_size([VIDEO_WIDTH as f32, VIDEO_HEIGHT as f32]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        "v4l2play",
        options,
        Box::new(move |cc| Ok(Box::new(VideoApp::new(cc, rx, VIDEO_WIDTH, VIDEO_HEIGHT)))),
    )
}
