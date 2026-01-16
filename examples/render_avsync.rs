//! AV-synced device renderer with AutoDecoder + audio playback
//!
//! Demonstrates scrcpy-style PTS synchronization:
//! - Video: PTS-driven rendering (minimal latency)
//! - Audio: Independent buffering (100-200ms)

mod utils;

use {
    anyhow::{Context, Result},
    crossbeam_channel::{Receiver, Sender, bounded},
    eframe::{egui, egui_wgpu},
    saide::{
        ScrcpyConnection,
        ServerParams,
        avsync::AVSync,
        decoder::{
            AudioDecoder,
            AudioPlayer,
            AutoDecoder,
            DecodedFrame,
            Nv12RenderResources,
            OpusDecoder,
            VideoDecoder,
            new_nv12_render_callback,
        },
    },
    std::{io::Read, sync::Arc, thread},
    tracing::{debug, error, info},
    utils::get_device_serial,
};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    info!("Starting AV-synced renderer (AutoDecoder + Audio)...");

    let serial = get_device_serial()?;
    info!("Device: {}", serial);

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 720.0])
            .with_title("Saide - AV Synchronized Renderer"),
        renderer: eframe::Renderer::Wgpu,
        vsync: false, // Disable VSync for low latency
        ..Default::default()
    };

    eframe::run_native(
        "Saide AV Sync",
        native_options,
        Box::new(|cc| Ok(Box::new(AVSyncApp::new(cc, serial)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {}", e))
}

struct AVSyncApp {
    frame_rx: Receiver<Arc<DecodedFrame>>,

    stats_rx: Receiver<AVStats>,

    current_frame: Option<Arc<DecodedFrame>>,

    stats: AVStats,

    _av_thread: Option<thread::JoinHandle<()>>,
}

#[derive(Default, Clone, Copy)]
struct AVStats {
    video_frames: u64,
    audio_frames: u64,
    last_video_pts: i64,
    last_audio_pts: i64,
    dropped_frames: u64,
}

impl AVSyncApp {
    fn new(cc: &eframe::CreationContext, serial: String) -> Self {
        let (frame_tx, frame_rx) = bounded(3);
        let (stats_tx, stats_rx) = bounded(100);

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

        // Spawn single AV worker thread (one connection for both streams)
        let av_thread = Some(thread::spawn(move || {
            if let Err(e) = av_worker(serial, frame_tx, stats_tx) {
                error!("AV worker thread error: {}", e);
            }
        }));

        Self {
            frame_rx,
            stats_rx,
            current_frame: None,
            stats: AVStats::default(),
            _av_thread: av_thread,
        }
    }
}

impl eframe::App for AVSyncApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Poll for new frames (non-blocking)
        while let Ok(frame) = self.frame_rx.try_recv() {
            self.current_frame = Some(frame);
        }

        // Update stats
        while let Ok(stats) = self.stats_rx.try_recv() {
            self.stats = stats;
        }

        // Top panel with stats
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("🎬 AV Synchronized Renderer");
                ui.separator();
                ui.label(format!("Video: {}", self.stats.video_frames));
                ui.label(format!("Audio: {}", self.stats.audio_frames));
                ui.separator();

                let v_pts_ms = self.stats.last_video_pts / 1000;
                let a_pts_ms = self.stats.last_audio_pts / 1000;
                let diff_ms = v_pts_ms - a_pts_ms;

                ui.label(format!("V-PTS: {}ms", v_pts_ms));
                ui.label(format!("A-PTS: {}ms", a_pts_ms));
                ui.label(format!(
                    "Diff: {}ms {}",
                    diff_ms.abs(),
                    if diff_ms.abs() < 20 { "✅" } else { "⚠️" }
                ));

                if self.stats.dropped_frames > 0 {
                    ui.label(format!("Dropped: {}", self.stats.dropped_frames));
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
                    new_nv12_render_callback(frame.clone(), 0),
                );

                ui.painter().add(callback);
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("Waiting for AV streams...");
                });
            }
        });

        ctx.request_repaint();
    }
}

fn av_worker(
    serial: String,
    frame_tx: Sender<Arc<DecodedFrame>>,
    stats_tx: Sender<AVStats>,
) -> Result<()> {
    info!("AV worker starting (single connection, dual threads)...");

    let server_jar = "3rd-party/scrcpy-server-v3.3.3";
    if !std::path::Path::new(server_jar).exists() {
        anyhow::bail!("Server JAR not found: {}", server_jar);
    }

    // Single connection with both video and audio
    let mut params = ServerParams::for_device(&serial)?;
    params.video = true;
    params.video_codec = "h264".to_string();
    params.video_bit_rate = 8_000_000;
    params.max_size = 1920;
    params.max_fps = 60;
    params.audio = true;
    params.audio_codec = "opus".to_string();
    params.control = true; // 启用控制通道，确保完整连接序列
    params.send_device_meta = true;
    params.send_codec_meta = true;
    params.send_frame_meta = true;

    info!("params.control = {}", params.control); // DEBUG

    let _rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let mut conn = ScrcpyConnection::connect(&serial, server_jar, "127.0.0.1", params)?;

    // Get resolution before extracting streams
    let (width, height) = conn.video_resolution.unwrap_or((1920, 1080));
    info!("Video resolution: {}x{}", width, height);

    // Extract streams from connection
    let mut video_stream = conn
        .take_video_stream()
        .context("Video stream not available")?;
    let mut audio_stream = conn
        .take_audio_stream()
        .context("Audio stream not available")?;

    info!("Streams extracted: video + audio ready");

    // Create AVSync (audio = master clock)
    let mut av_sync = AVSync::new(20);
    let av_snapshot = av_sync.snapshot(); // For video thread

    let stats = Arc::new(std::sync::Mutex::new(AVStats::default()));

    // Spawn audio thread (holds mutable AVSync)
    let stats_audio = stats.clone();
    let audio_thread = thread::spawn(move || {
        info!("Audio thread spawned, entering read loop...");
        match (|| -> Result<()> {
            let mut audio_decoder = OpusDecoder::new(48000, 2)?;
            let audio_player = AudioPlayer::new(48000, 2, 64)?;
            info!("Audio thread started (Opus)");

            loop {
                // Read audio packet (blocking)
                debug!("Audio thread: attempting to read header...");
                let mut header = [0u8; 12];
                audio_stream.read_exact(&mut header)?;
                debug!("Audio thread: header read successful");

                let packet_size =
                    u32::from_be_bytes([header[8], header[9], header[10], header[11]]) as usize;
                let pts = i64::from_be_bytes([
                    header[0], header[1], header[2], header[3], header[4], header[5], header[6],
                    header[7],
                ]);

                let mut payload = vec![0u8; packet_size];
                audio_stream.read_exact(&mut payload)?;

                // Update AVSync (audio = master clock)
                av_sync.update_audio_pts(pts);

                // Update stats
                {
                    let mut s = stats_audio.lock().unwrap();
                    s.audio_frames += 1;
                    s.last_audio_pts = pts;

                    if s.audio_frames.is_multiple_of(100) {
                        debug!("Audio: {} packets processed", s.audio_frames);
                    }
                }

                // Decode and play
                match audio_decoder.decode(&payload, pts) {
                    Ok(Some(decoded)) => {
                        if let Err(e) = audio_player.play(&decoded) {
                            debug!("Audio playback error: {}", e);
                        }
                    }
                    Ok(None) => {}
                    Err(e) => debug!("Audio decode error: {}", e),
                }
            }
        })() {
            Ok(_) => {}
            Err(e) => error!("Audio thread error: {}", e),
        }
    });

    // Video decode loop (main thread)
    let mut video_decoder = AutoDecoder::new(width, height)?;
    info!(
        "Video decoder initialized: {}",
        video_decoder.decoder_type()
    );
    info!("Starting video decode loop...");

    // Keep audio thread alive
    let _audio_thread_handle = audio_thread;

    loop {
        // Read video packet (blocking)
        use saide::scrcpy::protocol::video::VideoPacket;
        let video_packet = VideoPacket::read_from(&mut video_stream)?;
        let pts = video_packet.pts_us as i64;

        if let Ok(Some(frame)) = video_decoder.decode(&video_packet.data, pts) {
            // Update stats
            let current_stats = {
                let mut s = stats.lock().unwrap();
                s.video_frames += 1;
                s.last_video_pts = frame.pts;
                *s
            };

            // Check sync (lock-free read from snapshot)
            if av_snapshot.should_drop_video(frame.pts) {
                let mut stats_guard = stats.lock().unwrap();
                stats_guard.dropped_frames += 1;
                continue;
            }

            // Send frame to UI
            if frame_tx.try_send(Arc::new(frame)).is_err() {
                let mut stats_guard = stats.lock().unwrap();
                stats_guard.dropped_frames += 1;
            }

            // Send stats update
            let _ = stats_tx.try_send(current_stats);

            if current_stats.video_frames.is_multiple_of(100) {
                info!(
                    "Video: {} frames, {} dropped",
                    current_stats.video_frames, current_stats.dropped_frames
                );
            }
        }
    }
}
