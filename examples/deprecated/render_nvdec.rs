//! NVIDIA NVDEC Hardware Decoder Renderer

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
            NvdecDecoder,
            VideoDecoder,
            new_nv12_render_callback,
        },
        utils::get_device_serial,
    },
    std::{sync::Arc, thread},
    tracing::{debug, error, info, warn},
};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    info!("Starting NVIDIA NVDEC hardware decoder renderer...");

    let serial = get_device_serial()?;
    info!("Device: {}", serial);

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 720.0])
            .with_title("Saide - NVIDIA NVDEC Renderer"),
        renderer: eframe::Renderer::Wgpu,
        vsync: false, // 🚀 LATENCY: Disable VSync
        ..Default::default()
    };

    eframe::run_native(
        "Saide NVDEC Renderer",
        native_options,
        Box::new(|cc| Ok(Box::new(NvdecRendererApp::new(cc, serial)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {}", e))
}

struct NvdecRendererApp {
    frame_rx: Receiver<Arc<DecodedFrame>>,

    current_frame: Option<Arc<DecodedFrame>>,

    stats: RenderStats,

    _decoder_thread: Option<thread::JoinHandle<()>>,
}

#[derive(Default)]
struct RenderStats {
    total_frames: u64,
}

impl NvdecRendererApp {
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

impl eframe::App for NvdecRendererApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Poll for new frames
        while let Ok(frame) = self.frame_rx.try_recv() {
            self.current_frame = Some(frame);
            self.stats.total_frames += 1;
        }

        // Top panel with stats
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("NVIDIA NVDEC Hardware Renderer");
                ui.separator();
                ui.label(format!("Frames: {}", self.stats.total_frames));
            });
        });

        // Central panel with video
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(frame) = &self.current_frame {
                let available_size = ui.available_size();
                let aspect_ratio = frame.width as f32 / frame.height as f32;

                let (w, h) = if available_size.x / available_size.y > aspect_ratio {
                    (available_size.y * aspect_ratio, available_size.y)
                } else {
                    (available_size.x, available_size.x / aspect_ratio)
                };

                let rect = egui::Rect::from_center_size(
                    ui.available_rect_before_wrap().center(),
                    egui::vec2(w, h),
                );

                // Use the same pattern as render_vaapi
                let callback = egui_wgpu::Callback::new_paint_callback(
                    rect,
                    new_nv12_render_callback(frame.clone(), 0),
                );
                ui.painter().add(callback);
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("Waiting for video stream...");
                });
            }
        });

        ctx.request_repaint();
    }
}

fn decoder_worker(serial: String, frame_tx: Sender<Arc<DecodedFrame>>) -> Result<()> {
    info!("Decoder worker starting (NVDEC)...");

    let server_jar = "3rd-party/scrcpy-server-v3.3.3";
    if !std::path::Path::new(server_jar).exists() {
        anyhow::bail!("Server JAR not found: {}", server_jar);
    }

    // 🚀 NVDEC-specific codec options (avoid intra-refresh to prevent resolution changes)
    let nvdec_codec_options = Some(
        "profile=65536,\
         i-frame-interval=2,\
         latency=0,\
         priority=0,\
         prepend-sps-pps-to-idr-frames=1,\
         max-bframes=0,\
         intra-refresh-period=60,\
         bitrate-mode=1"
            .to_string(),
    );

    let params = ServerParams {
        video: true,
        video_codec: "h264".to_string(),
        video_bit_rate: 8_000_000,
        max_size: 1600,
        max_fps: 60,
        audio: false,
        control: true,
        send_device_meta: true,
        send_codec_meta: true,
        video_codec_options: nvdec_codec_options, // NVDEC-specific options
        ..Default::default()
    };

    info!("Connecting to device...");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let mut conn =
        rt.block_on(async { ScrcpyConnection::connect(&serial, server_jar, params).await })?;

    info!("Connected! Device: {:?}", conn.device_name);

    let (width, height) = conn.video_resolution.context("No video resolution")?;
    info!("Video resolution: {}x{}", width, height);

    info!("Initializing NVDEC decoder...");
    let mut decoder = NvdecDecoder::new(width, height)?;
    info!("NVDEC decoder initialized!");

    let mut frame_count = 0u64;
    let mut last_resolution = (width, height);

    loop {
        let packet = match conn.read_video_packet() {
            Ok(p) => p,
            Err(e) => {
                warn!("Failed to read packet: {}", e);
                break;
            }
        };

        if packet.data.is_empty() {
            continue;
        }

        // Check for resolution change in I-frames (SPS embedded via prepend-sps-pps-to-idr-frames)
        if packet.is_keyframe {
            // Extract resolution from SPS (embedded in I-frame due to
            // prepend-sps-pps-to-idr-frames=1)
            if let Some((width_sps, height_sps)) =
                saide::decoder::extract_resolution_from_stream(&packet.data)
            {
                let new_res = (width_sps, height_sps);
                if new_res != last_resolution {
                    info!(
                        "⚡ Resolution change detected in SPS: {}x{} -> {}x{}",
                        last_resolution.0, last_resolution.1, new_res.0, new_res.1
                    );

                    match NvdecDecoder::new(new_res.0, new_res.1) {
                        Ok(new_decoder) => {
                            decoder = new_decoder;
                            last_resolution = new_res;
                            info!("✅ Decoder recreated for new resolution!");
                        }
                        Err(e) => {
                            warn!("❌ Failed to recreate decoder: {}", e);
                            continue; // Skip this frame
                        }
                    }
                }
            }
        }

        match decoder.decode(&packet.data, packet.pts_us as i64) {
            Ok(Some(frame)) => {
                frame_count += 1;

                if frame_count.is_multiple_of(60) {
                    debug!("Decoded {} frames", frame_count);
                }

                if frame_tx.send(Arc::new(frame)).is_err() {
                    info!("Receiver dropped, stopping decoder");
                    break;
                }
            }
            Ok(None) => {
                // No frame yet
            }
            Err(e) => {
                warn!("Decode error: {}", e);
            }
        }
    }

    info!("Decoder worker stopped. Total frames: {}", frame_count);
    conn.shutdown()?;
    Ok(())
}
