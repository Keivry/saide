//! AV-synced device renderer with AutoDecoder + audio playback
//!
//! Demonstrates scrcpy-style PTS synchronization:
//! - Video: PTS-driven rendering (minimal latency)
//! - Audio: Independent buffering (100-200ms)

use {
    anyhow::Result,
    crossbeam_channel::{Receiver, Sender, bounded},
    eframe::{egui, egui_wgpu},
    saide::{
        ScrcpyConnection,
        ServerParams,
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
        sync::AVSync,
        utils::get_device_serial,
    },
    std::{
        sync::{Arc, Mutex},
        thread,
        time::Duration,
    },
    tracing::{debug, error, info, warn},
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
    current_frame: Option<Arc<DecodedFrame>>,
    stats: AVStats,
    #[allow(dead_code)]
    video_thread: Option<thread::JoinHandle<()>>,
    #[allow(dead_code)]
    audio_thread: Option<thread::JoinHandle<()>>,
}

#[derive(Default)]
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

        // Shared AV sync state
        let av_sync = Arc::new(Mutex::new(AVSync::new(20))); // 20ms threshold

        // Spawn video decoder thread
        let serial_video = serial.clone();
        let av_sync_video = av_sync.clone();
        let video_thread = Some(thread::spawn(move || {
            if let Err(e) = video_worker(serial_video, frame_tx, av_sync_video) {
                error!("Video thread error: {}", e);
            }
        }));

        // Spawn audio player thread
        let serial_audio = serial.clone();
        let av_sync_audio = av_sync.clone();
        let audio_thread = Some(thread::spawn(move || {
            if let Err(e) = audio_worker(serial_audio, av_sync_audio) {
                error!("Audio thread error: {}", e);
            }
        }));

        Self {
            frame_rx,
            current_frame: None,
            stats: AVStats::default(),
            video_thread,
            audio_thread,
        }
    }
}

impl eframe::App for AVSyncApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Poll for new frames (non-blocking)
        while let Ok(frame) = self.frame_rx.try_recv() {
            self.stats.last_video_pts = frame.pts;
            self.current_frame = Some(frame);
            self.stats.video_frames += 1;
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
                    new_nv12_render_callback(frame.clone()),
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

fn video_worker(
    serial: String,
    frame_tx: Sender<Arc<DecodedFrame>>,
    av_sync: Arc<Mutex<AVSync>>,
) -> Result<()> {
    info!("Video worker starting (AutoDecoder + PTS sync)...");

    let server_jar = "3rd-party/scrcpy-server-v3.3.3";
    if !std::path::Path::new(server_jar).exists() {
        anyhow::bail!("Server JAR not found: {}", server_jar);
    }

    // Use cached profile
    let mut params = ServerParams::for_device(&serial)?;
    params.video = true;
    params.video_codec = "h264".to_string();
    params.video_bit_rate = 8_000_000;
    params.max_size = 1920;
    params.max_fps = 60;
    params.audio = false;
    params.control = false;
    params.send_device_meta = true;
    params.send_codec_meta = true;
    params.send_frame_meta = true;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let mut conn =
        rt.block_on(async { ScrcpyConnection::connect(&serial, server_jar, params).await })?;

    if conn.video_stream.is_none() {
        anyhow::bail!("Video stream not available");
    }

    // Get resolution from codec meta
    let (width, height) = conn.video_resolution.unwrap_or((1920, 1080));
    info!("Video resolution: {}x{}", width, height);

    let mut decoder = AutoDecoder::new(width, height)?;
    let mut frame_count = 0u64;
    let mut dropped_count = 0u64;

    info!("Video decoder initialized ({})", decoder.decoder_type());

    loop {
        let video_packet = conn.read_video_packet()?;
        let pts = video_packet.pts_us as i64;

        if let Some(frame) = decoder.decode(&video_packet.data, pts)? {
            frame_count += 1;

            // Initialize AV clock with first frame
            {
                let mut sync = av_sync.lock().unwrap();
                sync.init_clock(frame.pts);
            }

            // PTS-based timing
            let sync = av_sync.lock().unwrap();

            // Check if frame is too late (drop it)
            if sync.should_drop_video(frame.pts) {
                dropped_count += 1;
                debug!("Dropped late frame #{} (PTS: {})", frame_count, frame.pts);
                continue;
            }

            // Calculate when to render
            if let Some(wait_duration) = sync.time_until_pts(frame.pts) {
                // Frame is early, wait until deadline
                if wait_duration > Duration::from_millis(100) {
                    // Sanity check: don't wait too long
                    warn!(
                        "Excessive wait time: {:?}, rendering immediately",
                        wait_duration
                    );
                } else {
                    drop(sync); // Release lock before sleep
                    thread::sleep(wait_duration);
                }
            }
            // else: frame is on-time or slightly late, render immediately

            // Send to UI (non-blocking)
            let arc_frame = Arc::new(frame);
            if frame_tx.try_send(arc_frame).is_err() {
                debug!("Frame buffer full, dropping frame");
                dropped_count += 1;
            }

            if frame_count.is_multiple_of(100) {
                info!(
                    "Video: {} frames rendered, {} dropped",
                    frame_count, dropped_count
                );
            }
        }
    }
}

fn audio_worker(serial: String, av_sync: Arc<Mutex<AVSync>>) -> Result<()> {
    info!("Audio worker starting (Opus + adaptive buffering)...");

    let server_jar = "3rd-party/scrcpy-server-v3.3.3";
    let params = ServerParams {
        video: false,
        audio: true,
        audio_codec: "opus".to_string(),
        control: false,
        send_device_meta: false,
        send_codec_meta: false,
        send_frame_meta: true,
        log_level: "info".to_string(),
        ..Default::default()
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let mut conn =
        rt.block_on(async { ScrcpyConnection::connect(&serial, server_jar, params).await })?;

    if conn.audio_stream.is_none() {
        warn!("Audio not available (Android 11+ required)");
        return Ok(());
    }

    let mut decoder = OpusDecoder::new(48000, 2)?;
    let player = AudioPlayer::new(48000, 2)?;

    info!("Audio decoder initialized (Opus)");

    let mut packet_count = 0u64;

    loop {
        let audio_packet = conn.read_audio_packet()?;
        let pts = audio_packet.pts;
        packet_count += 1;

        if let Some(decoded) = decoder.decode(&audio_packet.payload, pts)? {
            // Initialize AV clock with first audio frame (if video hasn't)
            {
                let mut sync = av_sync.lock().unwrap();
                sync.init_clock(decoded.pts);
            }

            // Play immediately (audio buffer handles timing)
            player.play(&decoded)?;

            if packet_count.is_multiple_of(100) {
                debug!(
                    "Audio: {} packets, buffer: {:.1}%",
                    packet_count,
                    player.buffer_level() * 100.0
                );
            }
        }
    }
}
