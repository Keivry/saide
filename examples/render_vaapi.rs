//! Real device rendering with VAAPI hardware decoder and NV12

use {
    anyhow::{Context, Result},
    crossbeam_channel::{Receiver, Sender, bounded},
    eframe::{egui, egui_wgpu},
    saide::{
        decoder::{DecodedFrame, Nv12RenderResources, VaapiDecoder, VideoDecoder, new_nv12_render_callback},
        ScrcpyConnection, ServerParams,
        scrcpy::protocol::video::VideoPacket,
    },
    std::{
        sync::Arc,
        thread,
    },
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
                    ui.label(format!("Resolution: {}x{} {:?}", frame.width, frame.height, frame.format));
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

                let (rect, _response) = ui.allocate_exact_size(
                    egui::vec2(width, height),
                    egui::Sense::hover(),
                );

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

    let params = ServerParams {
        video: true,
        video_codec: "h264".to_string(),
        video_bit_rate: 8_000_000,
        max_size: 1920,
        max_fps: 60,
        audio: false,
        control: true,
        send_device_meta: true,
        send_codec_meta: true,
        ..Default::default()
    };

    info!("Connecting to device...");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let mut conn = rt.block_on(async {
        ScrcpyConnection::connect(&serial, server_jar, params).await
    })?;

    info!("Connected! Device: {:?}", conn.device_name);

    // Get resolution from codec meta
    let (width, height) = conn.video_resolution.unwrap_or((1920, 1080));
    info!("Video resolution: {}x{}", width, height);

    info!("Initializing VAAPI decoder: {}x{}", width, height);
    let mut decoder = VaapiDecoder::new(width, height)?;

    // Read first packet
    let first_packet = conn.read_video_packet()?;
    debug!("First packet: config={}, size={}", first_packet.is_config, first_packet.data.len());

    // Decode first packet
    process_packet(&mut decoder, &first_packet, &frame_tx)?;

    // Main decode loop
    loop {
        match conn.read_video_packet() {
            Ok(packet) => {
                if let Err(e) = process_packet(&mut decoder, &packet, &frame_tx) {
                    warn!("Failed to process packet: {}", e);
                }
            }
            Err(e) => {
                error!("Failed to read video packet: {}", e);
                break;
            }
        }
    }

    info!("Decoder worker exiting");
    Ok(())
}

fn process_packet(
    decoder: &mut VaapiDecoder,
    packet: &VideoPacket,
    frame_tx: &Sender<Arc<DecodedFrame>>,
) -> Result<()> {
    if packet.data.is_empty() {
        return Ok(());
    }

    if packet.is_config {
        debug!("Processing CONFIG packet ({} bytes)", packet.data.len());
    }

    // Decode packet (may not produce frame immediately for CONFIG packets)
    match decoder.decode(&packet.data, packet.pts_us as i64) {
        Ok(Some(frame)) => {
            debug!("Decoded frame: {}x{} {:?} {} bytes", frame.width, frame.height, frame.format, frame.data.len());
            eprintln!("INFO: Sending frame to UI: {}x{} {:?}", frame.width, frame.height, frame.format);
            if let Err(e) = frame_tx.send(Arc::new(frame)) {
                warn!("Frame channel closed: {}", e);
            } else {
                eprintln!("INFO: Frame sent successfully");
            }
        }
        Ok(None) => {
            // No frame yet (normal for CONFIG packets)
            if !packet.is_config {
                debug!("No frame output for non-CONFIG packet");
            }
        }
        Err(e) => {
            warn!("Decode error: {}", e);
        }
    }

    Ok(())
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
