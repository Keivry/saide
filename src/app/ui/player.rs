#![allow(dead_code)] // V4l2Player deprecated - using StreamPlayer
#![allow(unused_variables)]
#![allow(unused_imports)]

use {
    super::VideoStats,
    crate::{
        config::SAideConfig,
        v4l2::{V4l2Capture, Yu12Frame, YuvRenderResources, new_yuv_render_callback},
    },
    crossbeam_channel::{Receiver, Sender, bounded},
    eframe::{egui::RichText, egui_wgpu},
    std::{
        sync::Arc,
        thread,
        time::{Duration, Instant},
    },
    tracing::{debug, error, trace},
};

const INIT_EVENT_COUNT: usize = 3;

pub enum InitEvent {
    FrameReceiver(Receiver<Arc<Yu12Frame>>),
    Dimensions(u32, u32),
    Capture(thread::JoinHandle<()>),
    Failed(String),
}

pub enum PlayerState {
    None,
    Initializing,
    Initialized,
    Retrying,
    Failed(String),
}

pub struct V4l2Player {
    config: Arc<SAideConfig>,

    init_tx: Sender<InitEvent>,
    init_rx: Receiver<InitEvent>,

    /// V4L2 capture thread handle
    capture: Option<thread::JoinHandle<()>>,

    /// Video frame receiver
    frame_rx: Option<Receiver<Arc<Yu12Frame>>>,
    /// Latest video frame
    frame: Option<Arc<Yu12Frame>>,

    /// Total number of frames dropped
    dropped_frames: u32,

    /// Latency measurement in milliseconds from V4l2 capture to display
    latency_ms: u32,

    // Flag indicating new frame availability
    has_new_frame: bool,

    // Video render rotation state (0-3), clockwise
    video_rotation: u32,

    /// Video display rectangle in egui coordinates
    video_rect: egui::Rect,

    video_original_width: u32,
    video_original_height: u32,

    fps: f32,

    state: PlayerState,
}

impl V4l2Player {
    pub fn new(cc: &eframe::CreationContext, config: Arc<SAideConfig>) -> Self {
        // Register YUV render resources with wgpu renderer
        if let Some(wgpu_state) = cc.wgpu_render_state.as_ref() {
            let resources = YuvRenderResources::new(&wgpu_state.device, wgpu_state.target_format);
            wgpu_state
                .renderer
                .write()
                .callback_resources
                .insert(resources);
        }

        let (tx, receiver) = bounded::<InitEvent>(INIT_EVENT_COUNT);

        V4l2Player {
            config,

            init_tx: tx,
            init_rx: receiver,

            capture: None,

            frame_rx: None,
            frame: None,

            dropped_frames: 0,
            latency_ms: 0,

            has_new_frame: false,

            video_rotation: 0,
            video_rect: egui::Rect::NOTHING,

            video_original_width: 0,
            video_original_height: 0,

            fps: 0.0,

            state: PlayerState::None,
        }
    }

    fn drop_capture(&mut self) {
        // Drop existing frame receiver
        self.frame_rx.take();

        if let Some(handle) = self.capture.take() {
            // Dropping the handle will stop the capture thread
            handle.join().ok();
            debug!("V4L2 capture thread stopped");
        }
    }

    pub fn initialize(&mut self) {
        // DEPRECATED: V4L2Player no longer used - replaced by StreamPlayer
        // This method kept for compatibility but does nothing
    }

    // Check if player is ready
    pub fn ready(&self) -> bool { matches!(self.state, PlayerState::Initialized) }

    pub fn video_stats(&self) -> VideoStats {
        VideoStats {
            fps: self.fps,
            total_frames: self.last_frame_seq().unwrap_or(0),
            dropped_frames: self.dropped_frames,
            latency_ms: self.latency_ms as f32,
        }
    }

    // Set player state to failed with reason
    pub fn failed(&mut self, reason: &str) { self.state = PlayerState::Failed(reason.into()); }

    pub fn rotation(&self) -> u32 { self.video_rotation }

    pub fn set_rotation(&mut self, rotation: u32) { self.video_rotation = rotation % 4; }

    pub fn video_rect(&self) -> egui::Rect { self.video_rect }

    pub fn last_frame_seq(&self) -> Option<u32> { self.frame.as_ref().map(|f| f.seq) }

    pub fn last_frame_instant(&self) -> Option<Instant> { self.frame.as_ref().map(|f| f.timestamp) }

    pub fn has_new_frame(&self) -> bool { self.has_new_frame }

    /// Get effective video dimensions considering rotation
    pub fn dimensions(&self) -> (u32, u32) {
        if self.video_rotation & 1 == 0 {
            (self.video_original_width, self.video_original_height)
        } else {
            (self.video_original_height, self.video_original_width)
        }
    }

    fn receive_frame(&mut self) {
        let Some(rx) = &self.frame_rx else {
            trace!("Skipping frame receiving - not initialized");
            return;
        };

        // Always receive latest frame
        while let Ok(frame) = rx.try_recv() {
            // Calculate FPS based on time between frames
            if let Some(last_timestamp) = self.last_frame_instant() {
                let elapsed = frame.timestamp.duration_since(last_timestamp).as_secs_f32();
                if elapsed > 0.0 {
                    // Smooth FPS with exponential moving average (alpha = 0.1)
                    let instant_fps = 1.0 / elapsed;
                    self.fps = if self.fps > 0.0 {
                        self.fps * 0.9 + instant_fps * 0.1
                    } else {
                        instant_fps
                    };
                }
            }

            // Calculate dropped frames based on sequence number
            if let Some(last_frame_seq) = self.last_frame_seq() {
                let expected_seq = last_frame_seq + 1;
                if frame.seq > expected_seq {
                    let dropped = frame.seq - expected_seq;
                    self.dropped_frames += dropped;
                    trace!(
                        "Dropped {} frames (seq {} -> {})",
                        dropped, last_frame_seq, frame.seq
                    );
                }
            }

            // Update frame and timestamp
            self.frame = Some(frame);
            self.has_new_frame = true;
        }
    }

    fn check_init_result(&mut self) {
        match self.init_rx.try_recv() {
            Ok(result) => match result {
                InitEvent::FrameReceiver(rx) => {
                    self.frame_rx = Some(rx);
                }
                InitEvent::Dimensions(w, h) => {
                    self.video_original_width = w;
                    self.video_original_height = h;
                }
                InitEvent::Capture(handle) => {
                    self.capture = Some(handle);
                }
                InitEvent::Failed(message) => {
                    self.state = PlayerState::Failed(message);
                }
            },
            Err(_) => {
                // Still initializing
            }
        }

        // Check if all initialization steps are done
        if self.frame_rx.is_some()
            && self.video_original_width > 0
            && self.video_original_height > 0
            && self.capture.is_some()
        {
            self.receive_frame();
            // only set to initialized on first frame received
            if self.frame.is_some() {
                self.state = PlayerState::Initialized;
                debug!("V4L2 Player initialized successfully");
            }
        }
    }

    pub fn draw(&mut self, ctx: &egui::Context) {
        match self.state {
            PlayerState::None => {
                self.state = PlayerState::Initializing;
                self.initialize();
            }
            PlayerState::Initializing => {
                self.draw_loading_overlay(ctx, "Initializing V4L2 Video Capture...");
                self.check_init_result();
            }
            PlayerState::Initialized => {
                self.receive_frame();
                self.draw_player(ctx);
            }
            PlayerState::Retrying => {
                self.draw_loading_overlay(ctx, "Re-initializing V4L2 Video Capture...");
                self.check_init_result();
            }
            PlayerState::Failed(ref e) => {
                self.draw_error_overlay(ctx, &format!("Initialization failed: {e}"));
            }
        }
    }

    /// Draw the main V4L2 video player area
    fn draw_player(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                // Get available rectangle
                let rect = ui.available_size();

                // Always maintain aspect ratio
                let (eff_w, eff_h) = self.dimensions();
                let aspect = eff_w as f32 / eff_h as f32;

                let (width, height) = if rect.x / rect.y > aspect {
                    (rect.y * aspect, rect.y)
                } else {
                    (rect.x, rect.x / aspect)
                };

                // Create centered rectangle
                self.video_rect =
                    egui::Rect::from_center_size(ui.max_rect().center(), egui::vec2(width, height));

                // Draw video frame or placeholder
                if let Some(frame) = &self.frame {
                    let callback = egui_wgpu::Callback::new_paint_callback(
                        self.video_rect,
                        new_yuv_render_callback(Arc::clone(frame), self.video_rotation),
                    );
                    ui.painter().add(callback);

                    // Update latency measurement
                    self.latency_ms = (self.latency_ms as f32 * 0.9
                        + frame.timestamp.elapsed().as_millis() as f32 * 0.1)
                        as u32;
                } else {
                    ui.painter()
                        .rect_filled(self.video_rect, 0.0, egui::Color32::from_gray(32));
                    ui.painter().text(
                        self.video_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Waiting for video...",
                        egui::FontId::proportional(24.0),
                        egui::Color32::GRAY,
                    );
                }
            });

        // Reset new frame flag
        self.has_new_frame = false;
    }

    fn draw_loading_overlay(&mut self, ctx: &egui::Context, message: &str) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.with_layout(
                egui::Layout::top_down_justified(egui::Align::Center),
                |ui| {
                    ui.add_space(100.0);
                    ui.label(
                        RichText::new(message)
                            .size(24.0)
                            .color(egui::Color32::WHITE),
                    );
                    ui.add_space(10.0);
                    ui.label("This may take a few seconds...");
                },
            );
        });
    }

    fn draw_error_overlay(&mut self, ctx: &egui::Context, message: &str) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.with_layout(
                egui::Layout::top_down_justified(egui::Align::Center),
                |ui| {
                    ui.add_space(100.0);
                    ui.label(
                        RichText::new("Initialization Failed")
                            .size(32.0)
                            .color(egui::Color32::RED),
                    );
                    ui.add_space(10.0);
                    ui.label(RichText::new(message).size(16.0));
                    ui.add_space(20.0);

                    if ui.button("Retry").clicked() {
                        self.state = PlayerState::Retrying;
                        self.initialize();
                    }
                },
            )
        });
    }
}
