use {
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
    /// Timestamp of last received frame
    last_frame_instant: Option<Instant>,

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
            last_frame_instant: None,
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
        // Clean up any existing capture
        self.drop_capture();

        let v4l2_device = self.config.scrcpy.v4l2.device.clone();
        let init_timeout = Duration::from_secs(u64::from(self.config.general.init_timeout));
        let init_tx = self.init_tx.clone();

        // V4L2 capture initialization
        thread::spawn(move || -> Result<(), anyhow::Error> {
            // Channel for frame transfer

            let mut capture = match V4l2Capture::new(&v4l2_device, init_timeout) {
                Ok(c) => c,
                Err(e) => {
                    error!("Failed to initialize V4L2 capture");
                    init_tx.send(InitEvent::Failed(format!(
                        "V4L2 capture initialization error: {}",
                        e
                    )))?;
                    return Err(e);
                }
            };
            debug!("V4L2 capture initialized");

            // Get capture dimensions and orientation
            let (width, height) = capture.dimensions();
            debug!("V4L2 capture dimensions: {}x{}", width, height,);

            // Start capture thread
            // Use a zero-capacity channel to always get the latest frame
            let (frame_tx, frame_rx) = bounded::<Arc<Yu12Frame>>(0);
            let handle = thread::spawn(move || {
                loop {
                    match capture.capture_frame() {
                        Ok(frame) => {
                            if frame_tx.send(Arc::new(frame)).is_err() {
                                // Receiver has been dropped, exit thread
                                debug!("Frame receiver dropped, stopping capture thread");
                                break;
                            }
                        }
                        Err(e) => {
                            error!("Capture error: {}", e);
                            break;
                        }
                    }
                }

                capture.stop_streaming().ok();
                debug!("V4L2 capture thread exiting");
            });
            debug!("V4L2 capture thread started");

            init_tx.send(InitEvent::FrameReceiver(frame_rx))?;
            init_tx.send(InitEvent::Dimensions(width, height))?;
            init_tx.send(InitEvent::Capture(handle))?;

            Ok(())
        });
    }

    // Check if player is ready
    pub fn ready(&self) -> bool { matches!(self.state, PlayerState::Initialized) }

    // Set player state to failed with reason
    pub fn failed(&mut self, reason: &str) { self.state = PlayerState::Failed(reason.into()); }

    pub fn fps(&self) -> f32 { self.fps }

    pub fn rotation(&self) -> u32 { self.video_rotation }

    pub fn set_rotation(&mut self, rotation: u32) { self.video_rotation = rotation % 4; }

    pub fn video_rect(&self) -> egui::Rect { self.video_rect }

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
            // Update frame and timestamp
            self.frame = Some(frame);
            self.has_new_frame = true;
        }

        // Update FPS calculation
        if let Some(last_instant) = self.last_frame_instant {
            let elapsed = last_instant.elapsed().as_secs_f32();
            if elapsed > 0.0 {
                self.fps = 1.0 / elapsed;
            }
        }

        if self.has_new_frame {
            self.last_frame_instant = Some(Instant::now());
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
