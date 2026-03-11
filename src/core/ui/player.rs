//! Stream Player Module
//!
//! Handles video/audio decoding and rendering in the UI

use {
    super::VideoStats,
    crate::{
        avsync::AVSync,
        decoder::{
            AudioDecoder,
            AudioPlayer,
            AutoDecoder,
            DecodedFrame,
            DecoderPreference,
            Nv12RenderResources,
            OpusDecoder,
            Pixel,
            RgbaRenderResources,
            VideoDecoder,
            extract_resolution_from_stream,
            new_nv12_render_callback,
            new_rgba_render_callback,
        },
        error::Result,
        profiler::{LatencyProfiler, LatencyStats, latency::LatencySummary},
        scrcpy::{
            codec_probe::{DecoderProfileDatabase, EncoderProfileDatabase, encoder_fingerprint},
            protocol::video::VideoPacket,
        },
        t,
        tf,
    },
    arc_swap::ArcSwap,
    crossbeam_channel::{Receiver, Sender, bounded},
    eframe::{egui, egui_wgpu},
    parking_lot::Mutex,
    std::{
        io::Read,
        net::TcpStream,
        sync::Arc,
        thread::{self, JoinHandle},
        time::{Duration, Instant},
    },
    tokio_util::sync::CancellationToken,
    tracing::{debug, error, info, warn},
};

// Ultra-low latency mode: single frame buffer
// Decoder uses try_send(), drops frame if UI is slow (non-blocking)
// This ensures absolute minimal latency for real-time screen mirroring
const FRAME_BUFFER_SIZE: usize = 1;

// Statistics buffer size
// Larger buffer to avoid dropping stats updates
// UI can process stats less frequently without losing data
const STATS_BUFFER_SIZE: usize = 64;

/// Maximum consecutive read errors before terminating the stream
const MAX_CONSECUTIVE_READ_ERRORS: u32 = 4;

/// Delay between read retries (ms)
const RETRY_DELAY_MS: u64 = 10;

/// Stream statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct StreamStats {
    pub video_frames: u64,
    pub audio_frames: u64,
    pub dropped_frames: u64,
    pub last_video_pts: i64,
    pub last_audio_pts: i64,
    pub last_decode_ms: f64,
    pub last_upload_ms: f64,
}

const PLAYER_EVENT_BUFFER_SIZE: usize = 5;

/// Player events for initialization and state changes
pub enum PlayerEvent {
    Ready {
        frame_rx: Receiver<Arc<DecodedFrame>>,
        stats_rx: Receiver<StreamStats>,
        width: u32,
        height: u32,
    },
    ResolutionChanged {
        width: u32,
        height: u32,
    },
    Failed {
        error: String,
        is_cancelled: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlayerState {
    Idle,
    Connecting,
    Streaming,
    Failed(String),
}

/// Stream Player - Decodes and renders video/audio streams
///
/// Orchestrates the video/audio decoding pipeline in a background thread and renders
/// decoded frames to the UI. Supports dynamic resolution changes, AV sync, and graceful shutdown.
///
/// # Architecture
/// - **Background thread**: Runs `stream_worker` for decoding (video + audio)
/// - **Main thread**: Polls for frames and renders via egui callbacks
/// - **AV Sync**: Audio is master clock, video drops frames if behind
///
/// # Thread Safety
/// - NOT `Send` or `Sync` (contains egui resources)
/// - Internal channels are thread-safe (`crossbeam_channel`)
/// - Cancellation via shared `CancellationToken`
///
/// # Lifecycle
/// 1. Create via [`StreamPlayer::new()`]
/// 2. Start decoding with [`start()`](StreamPlayer::start)
/// 3. Call [`update()`](StreamPlayer::update) every frame to poll events
/// 4. Call [`draw()`](StreamPlayer::draw) to render video
/// 5. Stop via [`stop()`](StreamPlayer::stop) or automatic on `Drop`
///
/// # Errors
/// - Decoder failures emit [`PlayerEvent::Failed`] via event channel
/// - Network errors trigger graceful shutdown with error message
pub struct StreamPlayer {
    repaint_ctx: egui::Context,

    /// Video frame receiver
    frame_rx: Option<Receiver<Arc<DecodedFrame>>>,
    /// Stats receiver
    stats_rx: Option<Receiver<StreamStats>>,

    /// Latest video frame
    current_frame: Option<Arc<DecodedFrame>>,

    /// Stream statistics
    stats: StreamStats,

    /// Latency summary (lock-free, published from decoder thread)
    latency_summary: Arc<ArcSwap<LatencySummary>>,

    /// Player state
    state: PlayerState,

    /// Stream worker thread
    stream_thread: Option<thread::JoinHandle<()>>,

    /// Video display rectangle
    video_rect: egui::Rect,

    /// Video dimensions
    video_width: u32,
    video_height: u32,

    /// Video rotation (0-3, clockwise 90°)
    video_rotation: u32,

    /// FPS counter
    fps: f32,
    frame_count: u32,
    fps_timer: Instant,

    /// Event channel for initialization
    event_tx: Sender<PlayerEvent>,
    event_rx: Receiver<PlayerEvent>,

    /// Cancellation token owned by Main App
    /// Used to signal thread shutdown
    /// StreamPlayer MUST NOT call cancel() itself
    cancel_token: CancellationToken,

    /// Audio buffer size in frames (configurable)
    audio_buffer_frames: u32,

    /// Audio ring buffer capacity in samples (configurable)
    audio_ring_capacity: usize,

    /// Hardware decoding enabled
    hwdecode: bool,

    /// Session-scoped cancellation token (child of app-level cancel_token).
    /// Cancelled by stop() to signal video/audio workers to exit this session only.
    session_token: Option<CancellationToken>,

    /// Cloned TcpStream handles used to shutdown blocking I/O in stop().
    video_ctrl: Option<TcpStream>,
    audio_ctrl: Option<TcpStream>,
}

impl StreamPlayer {
    pub fn new(
        cc: &eframe::CreationContext,
        cancel_token: CancellationToken,
        audio_buffer_frames: u32,
        audio_ring_capacity: usize,
        hwdecode: bool,
    ) -> Self {
        // Register render resources (both NV12 and RGBA)
        if let Some(wgpu_state) = cc.wgpu_render_state.as_ref() {
            let nv12_resources =
                Nv12RenderResources::new(&wgpu_state.device, wgpu_state.target_format);
            wgpu_state
                .renderer
                .write()
                .callback_resources
                .insert(nv12_resources);

            let rgba_resources =
                RgbaRenderResources::new(&wgpu_state.device, wgpu_state.target_format);
            wgpu_state
                .renderer
                .write()
                .callback_resources
                .insert(rgba_resources);
        }

        let (event_tx, event_rx) = bounded(PLAYER_EVENT_BUFFER_SIZE);

        Self {
            repaint_ctx: cc.egui_ctx.clone(),
            frame_rx: None,
            stats_rx: None,
            current_frame: None,
            stats: StreamStats::default(),
            latency_summary: Arc::new(ArcSwap::new(Arc::new(LatencySummary::default()))),
            state: PlayerState::Idle,
            stream_thread: None,
            video_rect: egui::Rect::NOTHING,
            video_width: 0,
            video_height: 0,
            video_rotation: 0,
            fps: 0.0,
            frame_count: 0,
            fps_timer: Instant::now(),
            event_tx,
            event_rx,
            cancel_token,
            audio_buffer_frames,
            audio_ring_capacity,
            hwdecode,
            session_token: None,
            video_ctrl: None,
            audio_ctrl: None,
        }
    }

    /// Start streaming with already established streams
    pub fn start(
        &mut self,
        video_stream: TcpStream,
        audio_stream: Option<TcpStream>,
        video_resolution: (u32, u32),
        serial: &str,
    ) {
        if self.stream_thread.is_some() {
            warn!(
                "start() called while a session is already active; stopping previous session first"
            );
            self.stop();
        }

        info!(
            "Starting stream: {}x{} for device {}",
            video_resolution.0, video_resolution.1, serial
        );
        self.state = PlayerState::Connecting;

        let event_tx = self.event_tx.clone();
        let repaint_ctx = self.repaint_ctx.clone();
        let session_token = self.cancel_token.child_token();
        self.session_token = Some(session_token.clone());
        self.video_ctrl = video_stream.try_clone().ok();
        self.audio_ctrl = audio_stream.as_ref().and_then(|s| match s.try_clone() {
            Ok(cloned) => Some(cloned),
            Err(e) => {
                warn!("Failed to clone audio TcpStream for shutdown control: {e}; stop() may wait for read timeout");
                None
            }
        });
        let latency_summary = self.latency_summary.clone();
        let audio_buffer_frames = self.audio_buffer_frames;
        let audio_ring_capacity = self.audio_ring_capacity;
        let hwdecode = self.hwdecode;
        let current_encoder_fingerprint = EncoderProfileDatabase::load()
            .ok()
            .and_then(|db| db.get(serial).cloned())
            .and_then(|profile| {
                encoder_fingerprint(
                    profile.video_encoder.as_deref(),
                    profile.optimal_config.as_deref(),
                )
            });
        let preferred_decoder = DecoderProfileDatabase::load()
            .ok()
            .and_then(|db| db.get(serial).cloned())
            .filter(|profile| {
                current_encoder_fingerprint
                    .as_deref()
                    .is_some_and(|fingerprint| profile.encoder_fingerprint == fingerprint)
            })
            .and_then(|profile| DecoderPreference::from_profile_name(&profile.validated_decoder));
        let event_tx_clone = event_tx.clone();
        self.stream_thread = Some(thread::spawn(move || {
            if session_token.is_cancelled() {
                debug!("Stream worker exiting due to cancellation");
                return;
            }

            if let Err(e) = stream_worker(
                video_stream,
                audio_stream,
                video_resolution,
                event_tx,
                session_token.clone(),
                latency_summary,
                StreamWorkerConfig {
                    audio_buffer_frames,
                    audio_ring_capacity,
                    hwdecode,
                    preferred_decoder,
                    repaint_ctx,
                },
            ) {
                let is_cancelled = e.is_cancelled();
                if !is_cancelled {
                    error!("Stream worker error: {e}");
                }
                let _ = event_tx_clone.send(PlayerEvent::Failed {
                    error: e.to_string(),
                    is_cancelled,
                });
            }
        }));
    }

    /// Stop streaming
    pub fn stop(&mut self) {
        use std::net::Shutdown;
        info!("Stopping stream");
        self.state = PlayerState::Idle;

        // 1. Cancel session token — wakes up any token.is_cancelled() check in video/audio loops.
        if let Some(token) = self.session_token.take() {
            token.cancel();
        }

        // 2. Shutdown sockets — unblocks any in-progress read_exact on video/audio streams.
        if let Some(ctrl) = self.video_ctrl.take() {
            let _ = ctrl.shutdown(Shutdown::Both);
        }
        if let Some(ctrl) = self.audio_ctrl.take() {
            let _ = ctrl.shutdown(Shutdown::Both);
        }

        // 3. Join worker thread — by now both loops should be exiting promptly.
        if let Some(handle) = self.stream_thread.take()
            && let Err(e) = handle.join()
        {
            warn!("Stream worker thread panicked: {:?}", e);
        }

        // 4. Drain stale events left in channel from the just-joined session.
        // Without this, a queued PlayerEvent::Ready from the old session could be consumed by
        // update() after stop() returns, writing PlayerState::Streaming over the Idle state —
        // or, on rapid stop→start, corrupting the new session's state machine.
        let drained = {
            let mut count = 0u32;
            while self.event_rx.try_recv().is_ok() {
                count += 1;
            }
            count
        };
        if drained > 0 {
            debug!("Drained {drained} stale event(s) from previous session");
        }

        // 5. Drop remaining state after join to avoid dangling references.
        self.frame_rx = None;
        self.stats_rx = None;
        self.current_frame = None;
    }

    pub fn update(&mut self) -> bool {
        let mut received_new_frame = false;

        // Check for initialization events
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                PlayerEvent::Ready {
                    frame_rx,
                    stats_rx,
                    width,
                    height,
                } => {
                    info!("Stream ready: {width}x{height}");
                    self.frame_rx = Some(frame_rx);
                    self.stats_rx = Some(stats_rx);
                    self.video_width = width;
                    self.video_height = height;
                    self.state = PlayerState::Streaming;
                }
                PlayerEvent::ResolutionChanged { width, height } => {
                    info!("Resolution changed: {width}x{height}");
                    self.video_width = width;
                    self.video_height = height;
                }
                PlayerEvent::Failed {
                    error,
                    is_cancelled,
                } => {
                    if !is_cancelled {
                        error!("Stream failed: {}", error);
                        self.state = PlayerState::Failed(error);
                    }
                    // is_cancelled=true means stop() was called intentionally;
                    // state is already Idle (set in stop()), so no override needed.
                }
            }
        }

        // Poll for new frames
        if let Some(ref frame_rx) = self.frame_rx {
            while let Ok(frame) = frame_rx.try_recv() {
                self.current_frame = Some(frame);
                self.frame_count += 1;
                received_new_frame = true;
            }
        }

        // Update stats
        if let Some(ref stats_rx) = self.stats_rx {
            while let Ok(stats) = stats_rx.try_recv() {
                self.stats = stats;
            }
        }

        // Calculate FPS, update every second
        let elapsed = self.fps_timer.elapsed();
        if elapsed >= Duration::from_secs(1) {
            self.fps = self.frame_count as f32 / elapsed.as_secs_f32();
            self.frame_count = 0;
            self.fps_timer = Instant::now();
        }

        received_new_frame
    }

    /// Draw video frame or placeholder in the UI
    pub fn draw(&mut self, ui: &mut egui::Ui) -> egui::Response {
        let available_rect = ui.available_rect_before_wrap();

        if let Some(ref frame) = self.current_frame {
            // Check if dimensions are valid
            if self.video_width == 0 || self.video_height == 0 {
                return ui
                    .centered_and_justified(|ui| {
                        ui.label(egui::RichText::new(t!("player-status-loading")).size(20.0))
                    })
                    .response;
            }

            // Calculate effective dimensions after rotation
            let (effective_width, effective_height) = if self.video_rotation.is_multiple_of(2) {
                (self.video_width, self.video_height)
            } else {
                // 90° or 270° rotation swaps dimensions
                (self.video_height, self.video_width)
            };

            let aspect_ratio = effective_width as f32 / effective_height as f32;
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

            // Create render callback based on pixel format
            match frame.format {
                Pixel::NV12 => {
                    let callback = new_nv12_render_callback(frame.clone(), self.video_rotation);
                    ui.painter()
                        .add(egui_wgpu::Callback::new_paint_callback(rect, callback));
                }
                Pixel::RGBA => {
                    let callback = new_rgba_render_callback(frame.clone(), self.video_rotation);
                    ui.painter()
                        .add(egui_wgpu::Callback::new_paint_callback(rect, callback));
                }
                _ => {
                    warn!(
                        "Unsupported pixel format: {:?}, falling back to NV12",
                        frame.format
                    );
                    let callback = new_nv12_render_callback(frame.clone(), self.video_rotation);
                    ui.painter()
                        .add(egui_wgpu::Callback::new_paint_callback(rect, callback));
                }
            }

            ui.allocate_rect(rect, egui::Sense::click())
        } else {
            // Show placeholder
            ui.centered_and_justified(|ui| match &self.state {
                PlayerState::Idle => {
                    ui.label(egui::RichText::new(t!("player-status-idle")).size(20.0));
                }
                PlayerState::Connecting => {
                    ui.label(egui::RichText::new(t!("player-status-connecting")).size(20.0));
                }
                PlayerState::Streaming => {
                    ui.label(egui::RichText::new(t!("player-status-loading")).size(20.0));
                }
                PlayerState::Failed(err) => {
                    self.draw_failed_overlay(ui, err);
                }
            })
            .response
        }
    }

    /// Get video display rectangle
    pub fn video_rect(&self) -> egui::Rect { self.video_rect }

    /// Get video rotation (0-3)
    pub fn rotation(&self) -> u32 { self.video_rotation }

    /// Get actual video resolution (NOT considering rotation, for control messages)
    #[allow(dead_code)]
    pub fn video_resolution(&self) -> (u32, u32) { (self.video_width, self.video_height) }

    /// Get effective video dimensions (considering rotation)
    pub fn video_dimensions(&self) -> (u32, u32) {
        if self.video_rotation.is_multiple_of(2) {
            (self.video_width, self.video_height)
        } else {
            (self.video_height, self.video_width)
        }
    }

    pub fn video_stats(&self) -> VideoStats {
        let summary = self.latency_summary.load();

        VideoStats {
            fps: self.fps,
            total_frames: self.stats.video_frames as u32,
            dropped_frames: self.stats.dropped_frames as u32,
            latency_ms: summary.average,
            latency_decode_ms: self.stats.last_decode_ms as f32,
            latency_upload_ms: self.stats.last_upload_ms as f32,
            latency_p95_ms: summary.p95,
        }
    }

    /// Get player state
    pub fn state(&self) -> &PlayerState { &self.state }

    /// Check if player is ready (streaming)
    pub fn ready(&self) -> bool { matches!(self.state, PlayerState::Streaming) }

    /// Set video rotation (0-3, clockwise 90°)
    pub fn set_rotation(&mut self, rotation: u32) { self.video_rotation = rotation % 4; }

    fn draw_failed_overlay(&self, ui: &mut egui::Ui, err_msg: &str) {
        use super::theme::AppColors;

        // General error overlay (decode errors, protocol errors, etc.)
        let ctx = ui.ctx();
        let colors = AppColors::from_context(ctx);

        egui::Area::new(egui::Id::new("error_overlay"))
            .fixed_pos(egui::pos2(0.0, 0.0))
            .show(ui.ctx(), |ui| {
                let screen_rect = ctx.content_rect();
                let mut child_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(screen_rect)
                        .layout(egui::Layout::top_down(egui::Align::Center)),
                );
                {
                    let ui = &mut child_ui;
                    ui.painter()
                        .rect_filled(screen_rect, 0.0, colors.error_overlay_bg);

                    // Center error message
                    ui.vertical_centered(|ui| {
                        ui.add_space(screen_rect.height() / 3.0);

                        ui.label(
                            egui::RichText::new(t!("player-error-title"))
                                .size(36.0)
                                .color(colors.error_overlay_title),
                        );

                        ui.add_space(20.0);

                        ui.label(
                            egui::RichText::new(t!("player-error-message"))
                                .size(20.0)
                                .color(colors.error_overlay_text),
                        );

                        ui.add_space(15.0);

                        ui.label(
                            egui::RichText::new(t!("player-error-restart"))
                                .size(16.0)
                                .color(colors.error_overlay_hint),
                        );

                        ui.add_space(10.0);

                        ui.label(
                            egui::RichText::new(tf!("player-error-details", "error" => err_msg))
                                .size(14.0)
                                .color(colors.error_overlay_details),
                        );
                    });
                }
            });
    }
}

impl Drop for StreamPlayer {
    fn drop(&mut self) { self.stop(); }
}

struct StreamWorkerConfig {
    audio_buffer_frames: u32,
    audio_ring_capacity: usize,
    hwdecode: bool,
    preferred_decoder: Option<DecoderPreference>,
    repaint_ctx: egui::Context,
}

/// Stream worker thread
fn stream_worker(
    mut video_stream: TcpStream,
    audio_stream: Option<TcpStream>,
    video_resolution: (u32, u32),
    event_tx: Sender<PlayerEvent>,
    token: CancellationToken,
    latency_summary: Arc<ArcSwap<LatencySummary>>,
    config: StreamWorkerConfig,
) -> Result<()> {
    let (width, height) = video_resolution;
    info!("Video resolution: {}x{}", width, height);

    let (frame_tx, frame_rx) = bounded(FRAME_BUFFER_SIZE);
    let (stats_tx, stats_rx) = bounded(STATS_BUFFER_SIZE);

    event_tx.send(PlayerEvent::Ready {
        frame_rx,
        stats_rx,
        width,
        height,
    })?;

    let StreamWorkerConfig {
        audio_buffer_frames,
        audio_ring_capacity,
        hwdecode,
        preferred_decoder,
        repaint_ctx,
    } = config;

    let decoder_mgr = DecoderManager::init(
        audio_buffer_frames,
        audio_ring_capacity,
        hwdecode,
        preferred_decoder,
    )?;
    let mut last_resolution = video_resolution;
    let mut video_decoder: Option<AutoDecoder> = None;

    let stats = Arc::new(Mutex::new(StreamStats::default()));
    let av_snapshot = decoder_mgr.av_sync.snapshot();

    let audio_thread = audio_stream.map(|audio_stream| {
        AudioThread::spawn(
            audio_stream,
            decoder_mgr.audio_decoder,
            decoder_mgr.audio_player,
            decoder_mgr.av_sync,
            stats.clone(),
            token.clone(),
        )
    });

    debug!("Starting video decode loop...");
    let mut latency_stats = LatencyStats::new(60);
    let decode_result = VideoLoop::run(
        &mut video_stream,
        &mut video_decoder,
        &frame_tx,
        &stats_tx,
        &event_tx,
        &repaint_ctx,
        &stats,
        &av_snapshot,
        &mut latency_stats,
        &latency_summary,
        &mut last_resolution,
        decoder_mgr.hwdecode,
        decoder_mgr.preferred_decoder,
        &token,
    );

    // Video loop ended — cancel the token to signal the audio thread to exit.
    // This covers all paths including early-return errors (e.g. AutoDecoder::new failure)
    // that bypass the in-loop cancel call. Idempotent if already cancelled.
    token.cancel();

    // Ensure audio thread exits and is joined before returning.
    if let Some(handle) = audio_thread
        && let Err(e) = handle.join()
    {
        warn!("Audio thread panicked: {:?}", e);
    }

    match decode_result {
        Ok(_) => {
            info!("Video decode loop completed normally");
            Ok(())
        }
        Err(e) => {
            let is_cancelled = e.is_cancelled();
            if !is_cancelled {
                error!("Connection error: {e}");
            }
            Err(e)
        }
    }
}

struct DecoderManager {
    audio_decoder: OpusDecoder,
    audio_player: AudioPlayer,
    av_sync: AVSync,
    hwdecode: bool,
    preferred_decoder: Option<DecoderPreference>,
}

impl DecoderManager {
    fn init(
        audio_buffer_frames: u32,
        audio_ring_capacity: usize,
        hwdecode: bool,
        preferred_decoder: Option<DecoderPreference>,
    ) -> Result<Self> {
        let audio_decoder = OpusDecoder::new(48000, 2)?;
        let audio_player = AudioPlayer::new(48000, 2, audio_buffer_frames, audio_ring_capacity)?;
        let av_sync = AVSync::new(20);

        info!(
            "Audio decoder initialized: Opus (video decoder will be created from first keyframe SPS)"
        );

        Ok(Self {
            audio_decoder,
            audio_player,
            av_sync,
            hwdecode,
            preferred_decoder,
        })
    }
}

struct AudioThread;

impl AudioThread {
    fn spawn(
        mut audio_stream: TcpStream,
        mut audio_decoder: OpusDecoder,
        mut audio_player: AudioPlayer,
        mut av_sync: AVSync,
        stats: Arc<Mutex<StreamStats>>,
        token: CancellationToken,
    ) -> JoinHandle<()> {
        thread::spawn(move || {
            match Self::run_audio_loop(
                &mut audio_stream,
                &mut audio_decoder,
                &mut audio_player,
                &mut av_sync,
                &stats,
                &token,
            ) {
                Ok(_) => {}
                Err(e) => debug!("Audio thread terminated: {e}"),
            }
        })
    }

    fn run_audio_loop(
        audio_stream: &mut TcpStream,
        audio_decoder: &mut OpusDecoder,
        audio_player: &mut AudioPlayer,
        av_sync: &mut AVSync,
        stats: &Arc<Mutex<StreamStats>>,
        token: &CancellationToken,
    ) -> Result<()> {
        let mut consecutive_read_errors = 0u32;

        loop {
            if token.is_cancelled() {
                debug!("Audio thread exiting due to cancellation");
                return Ok(());
            }

            let mut header = [0u8; 12];
            if let Err(e) = audio_stream.read_exact(&mut header) {
                if token.is_cancelled() {
                    debug!("Audio thread exiting due to cancellation");
                    return Ok(());
                }

                consecutive_read_errors += 1;
                if consecutive_read_errors >= MAX_CONSECUTIVE_READ_ERRORS {
                    warn!(
                        "Failed to read audio header {consecutive_read_errors} times consecutively"
                    );

                    // Signal main thread to stop the stream on repeated read failures
                    token.cancel();

                    return Err(e.into());
                }

                warn!(
                    "Audio header read error ({consecutive_read_errors}/{MAX_CONSECUTIVE_READ_ERRORS}): {e} - skipping"
                );
                thread::sleep(Duration::from_millis(RETRY_DELAY_MS));
                continue;
            }

            consecutive_read_errors = 0;

            let pts = i64::from_be_bytes(header[0..8].try_into().unwrap());
            let packet_size = u32::from_be_bytes(header[8..12].try_into().unwrap()) as usize;

            if packet_size > crate::constant::MAX_PACKET_SIZE {
                warn!("Audio packet size {packet_size} exceeds MAX_PACKET_SIZE, skipping");
                continue;
            }

            let mut payload = vec![0u8; packet_size];
            if let Err(e) = audio_stream.read_exact(&mut payload) {
                if token.is_cancelled() {
                    debug!("Audio thread exiting due to cancellation");
                    return Ok(());
                }

                consecutive_read_errors += 1;
                if consecutive_read_errors >= MAX_CONSECUTIVE_READ_ERRORS {
                    warn!(
                        "Failed to read audio payload {consecutive_read_errors} times consecutively"
                    );

                    // Signal main thread to stop the stream on repeated read failures
                    token.cancel();

                    return Err(e.into());
                }

                warn!(
                    "Audio payload read error ({consecutive_read_errors}/{MAX_CONSECUTIVE_READ_ERRORS}): {e} - skipping"
                );
                thread::sleep(Duration::from_millis(RETRY_DELAY_MS));
                continue;
            }

            consecutive_read_errors = 0;

            av_sync.update_audio_pts(pts);

            {
                let mut s = stats.lock();
                s.audio_frames += 1;
                s.last_audio_pts = pts;
            }

            if let Ok(Some(decoded)) = audio_decoder.decode(&payload, pts) {
                let _ = audio_player.play(&decoded);
            }
        }
    }
}

struct VideoLoop;

impl VideoLoop {
    #[allow(clippy::too_many_arguments)]
    fn run(
        video_stream: &mut TcpStream,
        video_decoder: &mut Option<AutoDecoder>,
        frame_tx: &Sender<Arc<DecodedFrame>>,
        stats_tx: &Sender<StreamStats>,
        event_tx: &Sender<PlayerEvent>,
        repaint_ctx: &egui::Context,
        stats: &Arc<Mutex<StreamStats>>,
        av_snapshot: &crate::avsync::AVSyncSnapshot,
        latency_stats: &mut LatencyStats,
        latency_summary: &Arc<ArcSwap<LatencySummary>>,
        last_resolution: &mut (u32, u32),
        hwdecode: bool,
        preferred_decoder: Option<DecoderPreference>,
        token: &CancellationToken,
    ) -> Result<()> {
        let mut consecutive_read_errors = 0u32;

        loop {
            if token.is_cancelled() {
                debug!("Video decode loop exiting due to cancellation");
                return Ok(());
            }

            let mut profiler = LatencyProfiler::new();

            let video_packet = match VideoPacket::read_from(video_stream) {
                Ok(packet) => {
                    profiler.mark_receive();
                    consecutive_read_errors = 0;
                    packet
                }
                Err(e) => {
                    if token.is_cancelled() {
                        debug!("Video decode loop exiting due to cancellation");
                        return Ok(());
                    }

                    if e.is_timeout() {
                        continue;
                    }

                    consecutive_read_errors += 1;
                    if consecutive_read_errors >= MAX_CONSECUTIVE_READ_ERRORS {
                        warn!(
                            "Failed to read video packet {consecutive_read_errors} times consecutively"
                        );

                        // Signal main thread to stop the stream on repeated read failures
                        token.cancel();

                        return Err(e);
                    }

                    warn!(
                        "Video packet read error ({consecutive_read_errors}/{MAX_CONSECUTIVE_READ_ERRORS}): {e} - retrying"
                    );
                    thread::sleep(Duration::from_millis(RETRY_DELAY_MS));
                    continue;
                }
            };

            let pts = video_packet.pts_us as i64;
            let estimated_capture = Instant::now() - Duration::from_millis(30);
            profiler.mark_capture(estimated_capture);

            let packet_resolution = extract_resolution_from_stream(&video_packet.data);
            let init_resolution =
                if video_packet.is_config && last_resolution.0 > 32 && last_resolution.1 > 32 {
                    packet_resolution.or(Some(*last_resolution))
                } else {
                    packet_resolution
                };

            if let Some(new_res) = init_resolution
                && new_res.0 > 32
                && new_res.1 > 32
            {
                if video_decoder.is_none() {
                    info!("Creating video decoder: {}x{}", new_res.0, new_res.1);
                    *video_decoder = Some(AutoDecoder::new(
                        new_res.0,
                        new_res.1,
                        hwdecode,
                        preferred_decoder,
                    )?);
                    *last_resolution = new_res;
                    let _ = event_tx.send(PlayerEvent::ResolutionChanged {
                        width: new_res.0,
                        height: new_res.1,
                    });
                } else if new_res != *last_resolution {
                    warn!(
                        "Resolution change detected in SPS: {}x{} -> {}x{}, recreating decoder",
                        last_resolution.0, last_resolution.1, new_res.0, new_res.1
                    );
                    *video_decoder = Some(AutoDecoder::new(
                        new_res.0,
                        new_res.1,
                        hwdecode,
                        preferred_decoder,
                    )?);
                    *last_resolution = new_res;
                    let _ = event_tx.send(PlayerEvent::ResolutionChanged {
                        width: new_res.0,
                        height: new_res.1,
                    });
                }
            }

            let Some(decoder) = video_decoder.as_mut() else {
                debug!("Waiting for first config/SPS packet to initialize decoder");
                continue;
            };

            let frame_opt = match decoder.decode(&video_packet.data, pts) {
                Ok(frame) => {
                    profiler.mark_decode();
                    frame
                }
                Err(e) => {
                    if token.is_cancelled() {
                        debug!("Video decode loop exiting due to cancellation");
                        return Ok(());
                    }
                    warn!("Video decode error: {e} - skipping frame");
                    continue;
                }
            };

            if let Some(frame) = frame_opt {
                profiler.mark_upload();

                let current_stats = {
                    let mut s = stats.lock();
                    s.video_frames += 1;
                    s.last_video_pts = frame.pts;
                    *s
                };

                if av_snapshot.should_drop_video(frame.pts) {
                    let mut s = stats.lock();
                    s.dropped_frames += 1;
                    continue;
                }

                profiler.mark_display();

                if let Some(breakdown) = profiler.breakdown() {
                    latency_stats.add_sample(breakdown.total_ms());

                    let mut s = stats.lock();
                    s.last_decode_ms = breakdown.decode_ms();
                    s.last_upload_ms = breakdown.upload_ms();
                }

                if current_stats.video_frames % 30 == 0 {
                    let summary = latency_stats.summary();
                    latency_summary.store(Arc::new(summary));
                }

                match frame_tx.try_send(Arc::new(frame)) {
                    Ok(()) => repaint_ctx.request_repaint(),
                    Err(crossbeam_channel::TrySendError::Full(_)) => {
                        let mut s = stats.lock();
                        s.dropped_frames += 1;
                    }
                    Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                        debug!("Frame channel disconnected, stopping decode loop");
                        return Ok(());
                    }
                }

                if current_stats.video_frames % 30 == 0 {
                    let _ = stats_tx.try_send(current_stats);
                }
            }
        }
    }
}
