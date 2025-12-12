//! 内部音视频流播放器
//!
//! 使用内部 scrcpy 实现替代外部 scrcpy + V4L2

use {
    super::VideoStats,
    crate::{
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
        scrcpy::{connection::ScrcpyConnection, server::ServerParams},
        sync::AVSync,
    },
    anyhow::{Context, Result},
    crossbeam_channel::{Receiver, Sender, bounded},
    eframe::{egui, egui_wgpu},
    std::{
        io::Read,
        sync::{Arc, Mutex},
        thread,
        time::{Duration, Instant},
    },
    tracing::{debug, error, info},
};

const FRAME_BUFFER_SIZE: usize = 3;
const STATS_BUFFER_SIZE: usize = 100;

#[derive(Debug, Clone, Copy, Default)]
pub struct StreamStats {
    pub video_frames: u64,
    pub audio_frames: u64,
    pub dropped_frames: u64,
    pub last_video_pts: i64,
    pub last_audio_pts: i64,
}

pub enum PlayerEvent {
    Ready {
        frame_rx: Receiver<Arc<DecodedFrame>>,
        stats_rx: Receiver<StreamStats>,
        width: u32,
        height: u32,
    },
    Failed(String),
}

#[derive(Debug, PartialEq)]
pub enum PlayerState {
    Idle,
    Connecting,
    Streaming,
    Failed(String),
}

pub struct StreamPlayer {
    /// Video frame receiver
    frame_rx: Option<Receiver<Arc<DecodedFrame>>>,
    /// Stats receiver
    stats_rx: Option<Receiver<StreamStats>>,

    /// Latest video frame
    current_frame: Option<Arc<DecodedFrame>>,

    /// Stream statistics
    stats: StreamStats,

    /// Player state
    state: PlayerState,

    /// Stream worker thread
    _stream_thread: Option<thread::JoinHandle<()>>,

    /// Video display rectangle
    video_rect: egui::Rect,

    /// Video dimensions
    video_width: u32,
    video_height: u32,

    /// FPS counter
    fps: f32,
    frame_count: u32,
    fps_timer: Instant,

    /// Event channel for initialization
    event_tx: Sender<PlayerEvent>,
    event_rx: Receiver<PlayerEvent>,
}

impl StreamPlayer {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        // Register NV12 render resources
        if let Some(wgpu_state) = cc.wgpu_render_state.as_ref() {
            let resources = Nv12RenderResources::new(&wgpu_state.device, wgpu_state.target_format);
            wgpu_state
                .renderer
                .write()
                .callback_resources
                .insert(resources);
        }

        let (event_tx, event_rx) = bounded(10);

        Self {
            frame_rx: None,
            stats_rx: None,
            current_frame: None,
            stats: StreamStats::default(),
            state: PlayerState::Idle,
            _stream_thread: None,
            video_rect: egui::Rect::NOTHING,
            video_width: 0,
            video_height: 0,
            fps: 0.0,
            frame_count: 0,
            fps_timer: Instant::now(),
            event_tx,
            event_rx,
        }
    }

    /// Start streaming from device
    pub fn start(&mut self, serial: String) {
        info!("Starting stream for device: {}", serial);
        self.state = PlayerState::Connecting;

        let event_tx = self.event_tx.clone();

        self._stream_thread = Some(thread::spawn(move || {
            if let Err(e) = stream_worker(serial, event_tx.clone()) {
                error!("Stream worker error: {}", e);
                let _ = event_tx.send(PlayerEvent::Failed(format!("{}", e)));
            }
        }));
    }

    /// Stop streaming
    pub fn stop(&mut self) {
        info!("Stopping stream");
        self.state = PlayerState::Idle;
        self.frame_rx = None;
        self.stats_rx = None;
        self.current_frame = None;
        // Thread will terminate when channels are dropped
    }

    /// Update player state (call every frame)
    pub fn update(&mut self) {
        // Check for initialization events
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                PlayerEvent::Ready {
                    frame_rx,
                    stats_rx,
                    width,
                    height,
                } => {
                    info!("Stream ready: {}x{}", width, height);
                    self.frame_rx = Some(frame_rx);
                    self.stats_rx = Some(stats_rx);
                    self.video_width = width;
                    self.video_height = height;
                    self.state = PlayerState::Streaming;
                }
                PlayerEvent::Failed(err) => {
                    error!("Stream failed: {}", err);
                    self.state = PlayerState::Failed(err);
                }
            }
        }

        // Poll for new frames
        if let Some(ref frame_rx) = self.frame_rx {
            while let Ok(frame) = frame_rx.try_recv() {
                self.current_frame = Some(frame);
                self.frame_count += 1;
            }
        }

        // Update stats
        if let Some(ref stats_rx) = self.stats_rx {
            while let Ok(stats) = stats_rx.try_recv() {
                self.stats = stats;
            }
        }

        // Calculate FPS
        let elapsed = self.fps_timer.elapsed();
        if elapsed >= Duration::from_secs(1) {
            self.fps = self.frame_count as f32 / elapsed.as_secs_f32();
            self.frame_count = 0;
            self.fps_timer = Instant::now();
        }
    }

    /// Render video frame
    pub fn render(&mut self, ui: &mut egui::Ui) -> egui::Response {
        let available_rect = ui.available_rect_before_wrap();

        if let Some(ref frame) = self.current_frame {
            let aspect_ratio = self.video_width as f32 / self.video_height as f32;
            let available_aspect = available_rect.width() / available_rect.height();

            let (display_width, display_height) = if available_aspect > aspect_ratio {
                let height = available_rect.height();
                (height * aspect_ratio, height)
            } else {
                let width = available_rect.width();
                (width, width / aspect_ratio)
            };

            let center_x = available_rect.center().x;
            let center_y = available_rect.center().y;
            let rect = egui::Rect::from_center_size(
                egui::pos2(center_x, center_y),
                egui::vec2(display_width, display_height),
            );

            self.video_rect = rect;

            // Create NV12 render callback
            let callback = new_nv12_render_callback(frame.clone());

            ui.painter()
                .add(egui_wgpu::Callback::new_paint_callback(rect, callback));

            ui.allocate_rect(rect, egui::Sense::click())
        } else {
            // Show placeholder
            ui.centered_and_justified(|ui| {
                ui.label(
                    egui::RichText::new(match &self.state {
                        PlayerState::Idle => "No Device",
                        PlayerState::Connecting => "Connecting...",
                        PlayerState::Streaming => "Loading...",
                        PlayerState::Failed(err) => err,
                    })
                    .size(20.0),
                )
            })
            .response
        }
    }

    /// Get video display rectangle
    pub fn video_rect(&self) -> egui::Rect { self.video_rect }

    /// Get video dimensions
    pub fn video_dimensions(&self) -> (u32, u32) { (self.video_width, self.video_height) }

    /// Get player state
    pub fn state(&self) -> &PlayerState { &self.state }

    /// Check if player is ready (streaming)
    pub fn ready(&self) -> bool { matches!(self.state, PlayerState::Streaming) }

    /// Get video dimensions
    pub fn dimensions(&self) -> (u32, u32) { (self.video_width, self.video_height) }

    /// Get video statistics
    pub fn video_stats(&self) -> VideoStats {
        VideoStats {
            fps: self.fps,
            total_frames: self.stats.video_frames as u32,
            dropped_frames: self.stats.dropped_frames as u32,
            latency_ms: 0.0, // TODO: implement latency measurement
        }
    }

    /// Rotation is handled by device orientation, not player
    /// This is a compatibility method for migration
    pub fn rotation(&self) -> u32 {
        0 // Always 0, rotation handled externally
    }

    /// Set rotation (no-op for compatibility)
    pub fn set_rotation(&mut self, _rotation: u32) {
        // No-op: rotation handled by device orientation
    }
}

impl Drop for StreamPlayer {
    fn drop(&mut self) { self.stop(); }
}

/// Stream worker thread
fn stream_worker(serial: String, event_tx: Sender<PlayerEvent>) -> Result<()> {
    let server_jar = "3rd-party/scrcpy-server-v3.3.3";
    if !std::path::Path::new(server_jar).exists() {
        let err = format!("Server JAR not found: {}", server_jar);
        anyhow::bail!(err);
    }

    // Setup connection parameters
    let mut params = ServerParams::for_device(&serial)?;
    params.video = true;
    params.video_codec = "h264".to_string();
    params.video_bit_rate = 8_000_000;
    params.max_size = 1920;
    params.max_fps = 60;
    params.audio = true;
    params.audio_codec = "opus".to_string();
    params.audio_source = "playback".to_string();
    params.control = true;
    params.send_device_meta = true;
    params.send_codec_meta = true;
    params.send_frame_meta = true;

    // Connect to device
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let mut conn =
        rt.block_on(async { ScrcpyConnection::connect(&serial, server_jar, params).await })?;

    let (width, height) = conn.video_resolution.unwrap_or((1920, 1080));
    info!("Video resolution: {}x{}", width, height);

    // Extract streams
    let mut video_stream = conn.video_stream.take().context("No video stream")?;
    let mut audio_stream = conn.audio_stream.take().context("No audio stream")?;

    // Setup channels
    let (frame_tx, frame_rx) = bounded(FRAME_BUFFER_SIZE);
    let (stats_tx, stats_rx) = bounded(STATS_BUFFER_SIZE);

    // Notify ready
    event_tx.send(PlayerEvent::Ready {
        frame_rx,
        stats_rx,
        width,
        height,
    })?;

    // Initialize decoders
    let mut video_decoder = AutoDecoder::new(width, height)?;
    let mut audio_decoder = OpusDecoder::new(48000, 2)?;
    let audio_player = AudioPlayer::new(48000, 2)?;

    info!(
        "Decoders initialized: {} + Opus",
        video_decoder.decoder_type()
    );

    // Shared state
    let av_sync = Arc::new(Mutex::new(AVSync::new(20)));
    let stats = Arc::new(Mutex::new(StreamStats::default()));

    // Spawn audio thread
    let stats_audio = stats.clone();
    let _audio_thread = thread::spawn(move || {
        match (|| -> Result<()> {
            loop {
                let mut header = [0u8; 12];
                audio_stream.read_exact(&mut header)?;
                let packet_size =
                    u32::from_be_bytes([header[8], header[9], header[10], header[11]]) as usize;
                let pts = i64::from_be_bytes([
                    header[0], header[1], header[2], header[3], header[4], header[5], header[6],
                    header[7],
                ]);

                let mut payload = vec![0u8; packet_size];
                audio_stream.read_exact(&mut payload)?;

                {
                    let mut s = stats_audio.lock().unwrap();
                    s.audio_frames += 1;
                    s.last_audio_pts = pts;
                }

                if let Ok(Some(decoded)) = audio_decoder.decode(&payload, pts) {
                    let _ = audio_player.play(&decoded);
                }
            }
        })() {
            Ok(_) => {}
            Err(e) => debug!("Audio thread terminated: {}", e),
        }
    });

    // Video decode loop (main thread)
    debug!("Starting video decode loop...");
    loop {
        use crate::scrcpy::protocol::video::VideoPacket;
        let video_packet = VideoPacket::read_from(&mut video_stream)?;
        let pts = video_packet.pts_us as i64;

        if let Ok(Some(frame)) = video_decoder.decode(&video_packet.data, pts) {
            let current_stats = {
                let mut s = stats.lock().unwrap();
                s.video_frames += 1;
                s.last_video_pts = frame.pts;
                *s
            };

            // Check sync and update stats
            let should_drop = {
                let sync = av_sync.lock().unwrap();
                sync.should_drop_video(frame.pts)
            };

            if should_drop {
                let mut s = stats.lock().unwrap();
                s.dropped_frames += 1;
                continue;
            }

            // Send frame
            if frame_tx.try_send(Arc::new(frame)).is_err() {
                let mut s = stats.lock().unwrap();
                s.dropped_frames += 1;
            }

            // Send stats snapshot
            let _ = stats_tx.try_send(current_stats);
        }
    }
}
