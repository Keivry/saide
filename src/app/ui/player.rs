//! Stream Player Module
//!
//! Handles video/audio decoding and rendering in the UI

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
            extract_resolution_from_stream,
            new_nv12_render_callback,
        },
        error::{Result, SAideError},
        scrcpy::protocol::video::VideoPacket,
        sync::{AVSync, AudioAction, DriftCorrection},
    },
    crossbeam_channel::{Receiver, Sender, bounded},
    eframe::{egui, egui_wgpu},
    parking_lot::Mutex,
    std::{
        io::Read,
        net::TcpStream,
        sync::Arc,
        thread,
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
    Failed(SAideError),
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlayerState {
    Idle,
    Connecting,
    Streaming,
    Failed(String),
}

/// Stream Player
/// Handles video/audio decoding and rendering in the UI
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
}

impl StreamPlayer {
    pub fn new(cc: &eframe::CreationContext, cancel_token: CancellationToken) -> Self {
        // Register NV12 render resources
        if let Some(wgpu_state) = cc.wgpu_render_state.as_ref() {
            let resources = Nv12RenderResources::new(&wgpu_state.device, wgpu_state.target_format);
            wgpu_state
                .renderer
                .write()
                .callback_resources
                .insert(resources);
        }

        let (event_tx, event_rx) = bounded(PLAYER_EVENT_BUFFER_SIZE);

        Self {
            frame_rx: None,
            stats_rx: None,
            current_frame: None,
            stats: StreamStats::default(),
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
        info!(
            "Starting stream: {}x{} for device {}",
            video_resolution.0, video_resolution.1, serial
        );
        self.state = PlayerState::Connecting;

        let event_tx = self.event_tx.clone();
        let cancel_token = self.cancel_token.clone();
        self.stream_thread = Some(thread::spawn(move || {
            if cancel_token.is_cancelled() {
                debug!("Stream worker exiting due to cancellation");
                return;
            }

            if let Err(e) = stream_worker(
                video_stream,
                audio_stream,
                video_resolution,
                event_tx.clone(),
                cancel_token,
            ) {
                if !e.is_cancelled() {
                    error!("Stream worker error: {e}");
                }
                event_tx.send(PlayerEvent::Failed(e)).unwrap_or_else(|err| {
                    error!("Failed to send PlayerEvent::Failed: {err}");
                });
            }
        }));
    }

    /// Stop streaming
    pub fn stop(&mut self) {
        info!("Stopping stream");
        self.state = PlayerState::Idle;

        // Drop channels first to signal thread to exit
        self.frame_rx = None;
        self.stats_rx = None;
        self.current_frame = None;

        // Detach thread handle
        self.stream_thread.take();
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
                PlayerEvent::Failed(err) => {
                    if !err.is_cancelled() {
                        error!("Stream failed: {}", err);
                    }

                    self.state = PlayerState::Failed(err.to_string());
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

        // Calculate FPS, update every second
        let elapsed = self.fps_timer.elapsed();
        if elapsed >= Duration::from_secs(1) {
            self.fps = self.frame_count as f32 / elapsed.as_secs_f32();
            self.frame_count = 0;
            self.fps_timer = Instant::now();
        }
    }

    /// Draw video frame or placeholder in the UI
    pub fn draw(&mut self, ui: &mut egui::Ui) -> egui::Response {
        // Update state first (process events)
        self.update();

        let available_rect = ui.available_rect_before_wrap();

        if let Some(ref frame) = self.current_frame {
            // Check if dimensions are valid
            if self.video_width == 0 || self.video_height == 0 {
                return ui
                    .centered_and_justified(|ui| {
                        ui.label(egui::RichText::new("Loading...").size(20.0))
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

            // Create NV12 render callback with rotation
            let callback = new_nv12_render_callback(frame.clone(), self.video_rotation);

            ui.painter()
                .add(egui_wgpu::Callback::new_paint_callback(rect, callback));

            ui.allocate_rect(rect, egui::Sense::click())
        } else {
            // Show placeholder
            ui.centered_and_justified(|ui| match &self.state {
                PlayerState::Idle => {
                    ui.label(egui::RichText::new("No Device").size(20.0));
                }
                PlayerState::Connecting => {
                    ui.label(egui::RichText::new("Connecting...").size(20.0));
                }
                PlayerState::Streaming => {
                    ui.label(egui::RichText::new("Loading...").size(20.0));
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

    /// Get video statistics
    pub fn video_stats(&self) -> VideoStats {
        VideoStats {
            fps: self.fps,
            total_frames: self.stats.video_frames as u32,
            dropped_frames: self.stats.dropped_frames as u32,
            latency_ms: 0.0, // TODO: implement latency measurement
        }
    }

    /// Get player state
    pub fn state(&self) -> &PlayerState { &self.state }

    /// Check if player is ready (streaming)
    pub fn ready(&self) -> bool { matches!(self.state, PlayerState::Streaming) }

    /// Set video rotation (0-3, clockwise 90°)
    pub fn set_rotation(&mut self, rotation: u32) { self.video_rotation = rotation % 4; }

    fn draw_failed_overlay(&self, ui: &mut egui::Ui, err_msg: &str) {
        // General error overlay (decode errors, protocol errors, etc.)
        let ctx = ui.ctx();
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
                    // Semi-transparent background
                    ui.painter().rect_filled(
                        screen_rect,
                        0.0,
                        egui::Color32::from_black_alpha(200),
                    );

                    // Center error message
                    ui.vertical_centered(|ui| {
                        ui.add_space(screen_rect.height() / 3.0);

                        ui.label(
                            egui::RichText::new("⚠️ Stream Error")
                                .size(36.0)
                                .color(egui::Color32::from_rgb(255, 100, 100)),
                        );

                        ui.add_space(20.0);

                        ui.label(
                            egui::RichText::new("An error occurred during streaming")
                                .size(20.0)
                                .color(egui::Color32::WHITE),
                        );

                        ui.add_space(15.0);

                        ui.label(
                            egui::RichText::new("Please restart the application")
                                .size(16.0)
                                .color(egui::Color32::GRAY),
                        );

                        ui.add_space(10.0);

                        ui.label(
                            egui::RichText::new(format!("Details: {}", err_msg))
                                .size(14.0)
                                .color(egui::Color32::DARK_GRAY),
                        );
                    });
                }
            });
    }
}

impl Drop for StreamPlayer {
    fn drop(&mut self) { self.stop(); }
}

/// Stream worker thread
/// Worker function that uses already-established streams (new implementation)
fn stream_worker(
    mut video_stream: TcpStream,
    audio_stream: Option<TcpStream>,
    video_resolution: (u32, u32),
    event_tx: Sender<PlayerEvent>,
    token: CancellationToken,
) -> Result<()> {
    let (width, height) = video_resolution;
    info!("Video resolution: {}x{}", width, height);

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

    // Initialize decoders (use Option for clean drop/rebuild lifecycle)
    let mut video_decoder = AutoDecoder::new(width, height)?;
    let mut audio_decoder = OpusDecoder::new(48000, 2)?;
    let audio_player = AudioPlayer::new(48000, 2)?;

    // Track last resolution for change detection
    let mut last_resolution = (width, height);

    info!(
        "Decoders initialized: {} + Opus",
        video_decoder.decoder_type()
    );

    // Create AVSync (audio = master clock)
    let mut av_sync = AVSync::new(20);
    let av_snapshot = av_sync.snapshot(); // For video thread

    let stats = Arc::new(Mutex::new(StreamStats::default()));

    // Spawn audio thread (if audio stream available) - holds mutable AVSync
    let audio_token = token.clone();
    let _audio_thread = if let Some(mut audio_stream) = audio_stream {
        let stats_audio = stats.clone();
        Some(thread::spawn(move || {
            match (|| -> Result<()> {
                let mut consecutive_read_errors = 0u32;

                loop {
                    if audio_token.is_cancelled() {
                        debug!("Audio thread exiting due to cancellation");
                        return Ok(());
                    }

                    // Read header with error tolerance
                    let mut header = [0u8; 12];
                    if let Err(e) = audio_stream.read_exact(&mut header) {
                        if audio_token.is_cancelled() {
                            debug!("Audio thread exiting due to cancellation");

                            // Graceful exit
                            return Ok(());
                        }

                        consecutive_read_errors += 1;
                        if consecutive_read_errors >= MAX_CONSECUTIVE_READ_ERRORS {
                            warn!(
                                "Failed to read audio header {consecutive_read_errors} times consecutively",
                            );
                            return Err(e.into());
                        }

                        warn!(
                            "Audio header read error ({consecutive_read_errors}/{MAX_CONSECUTIVE_READ_ERRORS}): {e} - skipping"
                        );

                        thread::sleep(Duration::from_millis(RETRY_DELAY_MS));
                        continue;
                    }

                    consecutive_read_errors = 0; // Reset on success

                    let packet_size =
                        u32::from_be_bytes([header[8], header[9], header[10], header[11]]) as usize;
                    let pts = i64::from_be_bytes([
                        header[0], header[1], header[2], header[3], header[4], header[5],
                        header[6], header[7],
                    ]);

                    // Read payload with error tolerance
                    let mut payload = vec![0u8; packet_size];
                    if let Err(e) = audio_stream.read_exact(&mut payload) {
                        if audio_token.is_cancelled() {
                            debug!("Audio thread exiting due to cancellation");

                            // Graceful exit
                            return Ok(());
                        }

                        consecutive_read_errors += 1;

                        if consecutive_read_errors >= MAX_CONSECUTIVE_READ_ERRORS {
                            warn!(
                                "Failed to read audio payload {consecutive_read_errors} times consecutively"
                            );
                            return Err(e.into());
                        }

                        warn!(
                            "Audio payload read error ({consecutive_read_errors}/{MAX_CONSECUTIVE_READ_ERRORS}): {e} - skipping",
                        );

                        thread::sleep(Duration::from_millis(RETRY_DELAY_MS));
                        continue;
                    }

                    consecutive_read_errors = 0; // Reset on success

                    // Update AVSync (audio = master clock)
                    av_sync.update_audio_pts(pts);

                    {
                        let mut s = stats_audio.lock();
                        s.audio_frames += 1;
                        s.last_audio_pts = pts;
                    }

                    // Decode audio frame
                    if let Ok(Some(decoded)) = audio_decoder.decode(&payload, pts) {
                        // Phase 3: PTS-driven audio playback
                        let action = av_sync.check_audio_pts(decoded.pts);

                        match action {
                            AudioAction::Play => {
                                // On-time, play immediately
                                let _ = audio_player.play(&decoded);
                            }
                            AudioAction::Drop => {
                                // Too late, drop frame
                                debug!("Dropping late audio frame: pts={}", decoded.pts);
                            }
                            AudioAction::Wait(_) => {
                                // DO NOTHING
                                // Audio player will handle underrun naturally
                            }
                        }

                        // Phase 4: Drift correction (every 50 frames ~= 1 second)
                        let current_audio_frames = {
                            let s = stats_audio.lock();
                            s.audio_frames
                        };

                        if current_audio_frames % 50 == 0 {
                            let correction = av_sync.update_drift(decoded.pts);
                            match correction {
                                DriftCorrection::DropFrame => {
                                    debug!(
                                        "Drift correction: dropping frame (audio ahead by {} us)",
                                        av_sync.average_drift_us()
                                    );
                                    // Frame already decoded but skip playing
                                }
                                DriftCorrection::InsertSilence => {
                                    debug!(
                                        "Drift correction: audio behind by {} us",
                                        av_sync.average_drift_us()
                                    );
                                    // Let natural underrun handle it
                                }
                                DriftCorrection::None => {}
                            }
                        }
                    }
                }
            })() {
                Ok(_) => {}
                Err(e) => debug!("Audio thread terminated: {e}"),
            }
        }))
    } else {
        None
    };

    // Video decode loop (main thread)
    debug!("Starting video decode loop...");
    let decode_result = (|| -> Result<()> {
        let mut consecutive_read_errors = 0u32;

        loop {
            if token.is_cancelled() {
                debug!("Video decode loop exiting due to cancellation");
                return Ok(());
            }

            // Read video packet with error tolerance
            let video_packet = match VideoPacket::read_from(&mut video_stream) {
                Ok(packet) => {
                    consecutive_read_errors = 0; // Reset on success
                    packet
                }
                Err(e) => {
                    if token.is_cancelled() {
                        debug!("Video decode loop exiting due to cancellation");

                        // Graceful exit
                        return Ok(());
                    }

                    if e.is_timeout() {
                        // Ignore timeouts, continue reading
                        // For example, no video packets will be sent while the device is locked
                        continue;
                    }

                    consecutive_read_errors += 1;
                    if consecutive_read_errors >= MAX_CONSECUTIVE_READ_ERRORS {
                        warn!(
                            "Failed to read video packet {consecutive_read_errors} times consecutively"
                        );
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

            // Check for resolution change in keyframes (SPS embedded)
            if video_packet.is_keyframe
                && let Some((width_sps, height_sps)) =
                    extract_resolution_from_stream(&video_packet.data)
            {
                let new_res = (width_sps, height_sps);
                // Ignore obviously invalid resolutions (Android encoder init artifacts)
                if new_res != last_resolution && new_res.0 > 32 && new_res.1 > 32 {
                    info!(
                        "Resolution change detected in SPS: {}x{} -> {}x{}",
                        last_resolution.0, last_resolution.1, new_res.0, new_res.1
                    );
                    let _ = event_tx.send(PlayerEvent::ResolutionChanged {
                        width: new_res.0,
                        height: new_res.1,
                    });

                    last_resolution = new_res;
                }
            }

            // Decode frame
            let frame_opt = match video_decoder.decode(&video_packet.data, pts) {
                Ok(frame) => frame,
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
                let current_stats = {
                    let mut s = stats.lock();
                    s.video_frames += 1;
                    s.last_video_pts = frame.pts;
                    *s
                };

                // Check sync and update stats (lock-free read from snapshot)
                if av_snapshot.should_drop_video(frame.pts) {
                    let mut s = stats.lock();
                    s.dropped_frames += 1;
                    continue;
                }

                // Send frame - if channel is closed, receiver dropped, exit gracefully
                if frame_tx.try_send(Arc::new(frame)).is_err() {
                    if frame_tx.is_full() {
                        let mut s = stats.lock();
                        s.dropped_frames += 1;
                    } else {
                        // Channel disconnected, exit gracefully
                        debug!("Frame channel disconnected, stopping decode loop");
                        return Ok(());
                    }
                }

                // Send stats periodically (every 30 frames to reduce overhead)
                if current_stats.video_frames % 30 == 0 {
                    let _ = stats_tx.try_send(current_stats);
                }
            }
        }
    })();

    match decode_result {
        Ok(_) => {
            info!("Video decode loop completed normally");
            Ok(())
        }
        Err(e) => {
            error!("Connection error: {e}");
            event_tx
                .send(PlayerEvent::Failed(e.clone()))
                .unwrap_or_else(|err| {
                    error!("Failed to send PlayerEvent::Failed: {err}");
                });

            Err(e)
        }
    }
}
