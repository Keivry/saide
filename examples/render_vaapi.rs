//! Real device rendering with VAAPI hardware decoder and NV12

use {
    anyhow::{Context, Result},
    crossbeam_channel::{Receiver, Sender, bounded},
    eframe::{egui, egui_wgpu},
    saide::{
        ScrcpyConnection,
        ServerParams,
        decoder::{
            DecodedFrame,
            Nv12RenderResources,
            VaapiDecoder,
            VideoDecoder,
            new_nv12_render_callback,
        },
        scrcpy::protocol::video::VideoPacket,
    },
    std::{sync::Arc, thread},
    tracing::{debug, error, info, warn},
};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("Starting VAAPI hardware decoder renderer...");

    let serial = get_device_serial()?;
    info!("Device: {}", serial);

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 720.0])
            .with_title("Saide - VAAPI Hardware Renderer"),
        renderer: eframe::Renderer::Wgpu,
        vsync: false, // 🚀 LATENCY: Disable VSync (saves ~16ms)
        ..Default::default()
    };

    eframe::run_native(
        "Saide VAAPI Renderer",
        native_options,
        Box::new(|cc| Ok(Box::new(VaapiRendererApp::new(cc, serial)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {}", e))
}

struct VaapiRendererApp {
    frame_rx: Receiver<Arc<DecodedFrame>>,
    current_frame: Option<Arc<DecodedFrame>>,
    stats: RenderStats,
    #[allow(dead_code)]
    decoder_thread: Option<thread::JoinHandle<()>>,
}

#[derive(Default)]
struct RenderStats {
    total_frames: u64,
}

impl VaapiRendererApp {
    fn new(cc: &eframe::CreationContext, serial: String) -> Self {
        let (frame_tx, frame_rx) = bounded(3);

        // Register NV12 render resources
        let render_state = cc.wgpu_render_state.as_ref().unwrap();
        render_state
            .renderer
            .write()
            .callback_resources
            .insert(Nv12RenderResources::new(
                &render_state.device,
                render_state.target_format,
            ));

        // Spawn decoder thread
        let decoder_thread = Some(thread::spawn(move || {
            if let Err(e) = decoder_worker(serial, frame_tx) {
                error!("Decoder thread error: {}", e);
            }
        }));

        Self {
            frame_rx,
            current_frame: None,
            stats: RenderStats::default(),
            decoder_thread,
        }
    }
}

impl eframe::App for VaapiRendererApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Poll for new frames
        while let Ok(frame) = self.frame_rx.try_recv() {
            self.current_frame = Some(frame);
            self.stats.total_frames += 1;
        }

        // Top panel with stats
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("🚀 VAAPI Hardware Renderer");
                ui.separator();
                ui.label(format!("Frames: {}", self.stats.total_frames));
                if let Some(frame) = &self.current_frame {
                    ui.label(format!(
                        "Resolution: {}x{} {:?}",
                        frame.width, frame.height, frame.format
                    ));
                }
            });
        });

        // Central panel with video
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(frame) = &self.current_frame {
                let available_size = ui.available_size();
                let aspect_ratio = frame.width as f32 / frame.height as f32;
                let display_width = available_size.x;
                let display_height = display_width / aspect_ratio;

                let (width, height) = if display_height > available_size.y {
                    let h = available_size.y;
                    (h * aspect_ratio, h)
                } else {
                    (display_width, display_height)
                };

                let (rect, _response) =
                    ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());

                let callback = egui_wgpu::Callback::new_paint_callback(
                    rect,
                    new_nv12_render_callback(frame.clone()),
                );

                ui.painter().add(callback);
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("Waiting for video stream...");
                });
            }
        });

        // Request continuous repaint
        ctx.request_repaint();
    }
}

fn decoder_worker(serial: String, frame_tx: Sender<Arc<DecodedFrame>>) -> Result<()> {
    info!("Decoder worker starting (VAAPI)...");

    let server_jar = "3rd-party/scrcpy-server-v3.3.3";
    if !std::path::Path::new(server_jar).exists() {
        anyhow::bail!("Server JAR not found: {}", server_jar);
    }

    // Use cached profile (includes encoder + options compatibility test)
    let mut params = ServerParams::for_device(&serial)?;
    params.video = true;
    params.video_codec = "h264".to_string();
    // params.video_encoder is already set from profile
    params.video_bit_rate = 8_000_000;
    params.max_size = 1920;
    params.max_fps = 60;
    params.audio = false;
    params.control = true;
    params.send_device_meta = true;
    params.send_codec_meta = true;

    if let Some(ref encoder) = params.video_encoder {
        info!("Using encoder from profile: {}", encoder);
    } else {
        info!("Using system default encoder");
    }

    info!("Connecting to device...");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let mut conn =
        rt.block_on(async { ScrcpyConnection::connect(&serial, server_jar, params).await })?;

    info!("Connected! Device: {:?}", conn.device_name);

    // Get resolution from codec meta
    let (width, height) = conn.video_resolution.unwrap_or((1920, 1080));
    info!("Video resolution: {}x{}", width, height);

    info!("Initializing VAAPI decoder: {}x{}", width, height);
    let mut decoder = VaapiDecoder::new(width, height)?;
    let mut last_resolution = (width, height);

    // Main decode loop
    loop {
        let packet = match conn.read_video_packet() {
            Ok(p) => p,
            Err(e) => {
                warn!("Failed to read packet: {}", e);
                break Ok(());
            }
        };

        if packet.data.is_empty() {
            continue;
        }

        // Check for resolution change in I-frames
        if packet.is_keyframe {
            if let Some((width_sps, height_sps)) =
                saide::decoder::extract_resolution_from_stream(&packet.data)
            {
                let new_res = (width_sps, height_sps);
                if new_res != last_resolution {
                    info!(
                        "⚡ VAAPI resolution change: {}x{} -> {}x{}",
                        last_resolution.0, last_resolution.1, new_res.0, new_res.1
                    );

                    match VaapiDecoder::new(new_res.0, new_res.1) {
                        Ok(new_decoder) => {
                            decoder = new_decoder;
                            last_resolution = new_res;
                            info!("✅ VAAPI decoder recreated!");
                        }
                        Err(e) => {
                            warn!("❌ Failed to recreate VAAPI decoder: {}", e);
                            continue;
                        }
                    }
                }
            }
        }

        match decoder.decode(&packet.data, packet.pts_us as i64) {
            Ok(Some(frame)) => {
                if let Err(e) = frame_tx.send(Arc::new(frame)) {
                    warn!("Frame channel closed: {}", e);
                }
            }
            Ok(None) => {} // No frame yet
            Err(e) => {
                warn!("VAAPI decode error: {}", e);
            }
        }
    }
}

fn get_device_serial() -> Result<String> {
    if let Some(serial) = std::env::args().nth(1) {
        return Ok(serial);
    }

    let output = std::process::Command::new("adb")
        .args(["devices"])
        .output()
        .context("Failed to run 'adb devices'")?;

    let output_str = String::from_utf8_lossy(&output.stdout);

    for line in output_str.lines().skip(1) {
        if let Some(serial) = line.split_whitespace().next() {
            if !serial.is_empty() {
                return Ok(serial.to_string());
            }
        }
    }

    anyhow::bail!("No Android device found")
}
