use {
    crate::{
        config::{
            SAideConfig,
            mapping::{MouseButton, MouseConfig, WheelDirection},
        },
        controller::{adb::AdbShell, keyboard::KeyboardMapper, mouse::MouseMapper, scrcpy::Scrcpy},
        v4l2::{V4l2Capture, Yu12Frame, YuvRenderResources, new_yuv_render_callback},
    },
    anyhow::anyhow,
    crossbeam_channel::{Receiver, Sender, bounded},
    eframe::{
        egui::{self, Button, Color32, RichText},
        egui_wgpu,
    },
    once_cell::sync::Lazy,
    parking_lot::RwLock,
    std::{
        sync::Arc,
        thread,
        time::{Duration, Instant},
    },
    tracing::{error, info},
};

const DEFAULT_WIDTH: u32 = 1280;
const DEFAULT_HEIGHT: u32 = 720;

const BG_COLOR: Color32 = Color32::from_rgb(32, 32, 32);
const FG_COLOR: Color32 = Color32::from_rgb(220, 220, 220);

const TOOLBAR_WIDTH: f32 = 42.0;
const STATUSBAR_HEIGHT: f32 = 32.0;

const TOOLBAR_BTN_COUNT: usize = 1;
const TOOLBAR_BTN_SIZE: [f32; 2] = [36.0, 36.0];
const TOOLBAR_BTN_SPACING: f32 = 2.0;

/// Initialization result type
type InitResult = Result<
    (
        Scrcpy,
        Arc<RwLock<AdbShell>>,
        Option<KeyboardMapper>,
        Option<MouseMapper>,
        Receiver<Arc<Yu12Frame>>,
        (u32, u32),
        u32,
        u32,
    ),
    anyhow::Error,
>;

/// Initialization state enum
#[derive(PartialEq)]
enum InitState {
    NotStarted,
    InProgress,
    Ready,
    Failed,
}

struct ToolbarButton {
    lable: &'static str,
    tooltip: &'static str,
    callback: fn(&mut SAideApp, &egui::Context),
}

static TOOLBAR_BUTTONS: Lazy<Vec<ToolbarButton>> = Lazy::new(|| {
    vec![ToolbarButton {
        lable: "⟳",
        tooltip: "Rotate Video",
        callback: SAideApp::rotate,
    }]
});

/// Main UI state
pub struct SAideApp {
    config: Arc<SAideConfig>,
    scrcpy: Option<Scrcpy>,

    adb_shell: Option<Arc<RwLock<AdbShell>>>,
    mouse_mapper: Option<MouseMapper>,
    keyboard_mapper: Option<KeyboardMapper>,

    frame_rx: Option<Receiver<Arc<Yu12Frame>>>,
    frame: Option<Arc<Yu12Frame>>,

    last_frame_instant: Option<Instant>,

    phisical_size: Option<(u32, u32)>,

    video_width: u32,
    video_height: u32,

    /// Video display rectangle
    video_rect: Option<egui::Rect>,

    rotation: u32,
    fps: f32,

    /// Initialization state machine
    init_state: InitState,
    init_rx: Option<Receiver<InitResult>>,

    /// Keyboard mapping switch
    keyboard_mapping_enabled: bool,

    /// Mouse mapping switch
    mouse_mapping_enabled: bool,
}

impl SAideApp {
    pub fn new(cc: &eframe::CreationContext<'_>, config: SAideConfig) -> Self {
        if let Some(wgpu_state) = cc.wgpu_render_state.as_ref() {
            let resources = YuvRenderResources::new(&wgpu_state.device, wgpu_state.target_format);
            wgpu_state
                .renderer
                .write()
                .callback_resources
                .insert(resources);
        }

        let keyboard_mapping_enabled = config
            .mappings
            .keyboard
            .as_ref()
            .map_or(false, |k| k.initial_state);

        let mouse_mapping_enabled = config
            .mappings
            .mouse
            .as_ref()
            .map_or(true, |m| m.initial_state);

        Self {
            config: Arc::new(config),
            scrcpy: None,
            adb_shell: None,
            mouse_mapper: None,
            keyboard_mapper: None,
            frame_rx: None,
            frame: None,
            last_frame_instant: None,
            video_width: 0,
            video_height: 0,
            phisical_size: None,
            video_rect: None,
            rotation: 0,
            fps: 0.0,
            init_state: InitState::NotStarted,
            init_rx: None,
            keyboard_mapping_enabled,
            mouse_mapping_enabled,
        }
    }

    fn effective_dimensions(&self) -> (u32, u32) {
        if self.rotation & 1 == 0 {
            (self.video_width, self.video_height)
        } else {
            (self.video_height, self.video_width)
        }
    }

    /// Resize the application window to match video dimensions
    fn resize(&mut self, ctx: &egui::Context) {
        let (w, h) = self.effective_dimensions();
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
            w as f32 + TOOLBAR_WIDTH,
            h as f32 + STATUSBAR_HEIGHT,
        )));
    }

    /// Rotate video by 90 degrees clockwise
    fn rotate(&mut self, ctx: &egui::Context) {
        self.rotation = (self.rotation + 1) % 4;
        self.resize(ctx);
    }

    /// Draw the toolbar on the left side
    fn draw_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.spacing_mut().item_spacing.y = TOOLBAR_BTN_SPACING;

            // Center buttons vertically
            let rect = ui.available_rect_before_wrap();
            let desired_height = (TOOLBAR_BTN_SIZE[1] + TOOLBAR_BTN_SPACING)
                * TOOLBAR_BTN_COUNT as f32
                + TOOLBAR_BTN_SPACING;
            let top_padding = (rect.height() - desired_height) / 2.0;
            ui.add_space(top_padding);

            ui.add_space(TOOLBAR_BTN_SPACING);
            for btn in TOOLBAR_BUTTONS.iter() {
                if ui
                    .add_sized(
                        TOOLBAR_BTN_SIZE,
                        Button::new(RichText::new(btn.lable).color(FG_COLOR).size(16.0)),
                    )
                    .on_hover_text(btn.tooltip)
                    .clicked()
                {
                    (btn.callback)(self, ui.ctx());
                }
                ui.add_space(TOOLBAR_BTN_SPACING);
            }
        });
    }

    /// Draw the status bar at the top
    fn draw_statusbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.label(format!(
                "Resolution: {}x{} | FPS: {} |Rotation: {}°",
                self.video_width,
                self.video_height,
                self.fps.min(self.config.scrcpy.video.max_fps as f32) as u32,
                self.rotation * 90
            ));
        });
    }

    /// Draw the main v4l2 player area
    fn draw_v4l2_player(&mut self, ui: &mut egui::Ui) {
        // Receive latest frame
        while let Some(rx) = self.frame_rx.as_ref()
            && let Ok(frame) = rx.try_recv()
        {
            self.frame = Some(frame);
        }

        // Update FPS
        if let Some(last_instant) = self.last_frame_instant {
            let delta = Instant::now().duration_since(last_instant).as_secs_f32();
            if delta > 0.0 {
                self.fps = 0.95 * self.fps + 0.05 * (1.0 / delta);
            }
        }

        self.last_frame_instant = Some(Instant::now());

        // Get available rectangle
        let rect = ui.available_size();

        // Always maintain aspect ratio
        let (eff_w, eff_h) = self.effective_dimensions();
        let aspect = eff_w as f32 / eff_h as f32;

        let (width, height) = if rect.x / rect.y > aspect {
            (rect.y * aspect, rect.y)
        } else {
            (rect.x, rect.x / aspect)
        };

        // Create centered rectangle
        let rect = egui::Rect::from_center_size(ui.max_rect().center(), egui::vec2(width, height));
        self.video_rect = Some(rect);

        // Draw video frame or placeholder
        if let Some(frame) = &self.frame {
            let callback = egui_wgpu::Callback::new_paint_callback(
                rect,
                new_yuv_render_callback(Arc::clone(frame), self.rotation),
            );
            ui.painter().add(callback);
        } else {
            ui.painter()
                .rect_filled(rect, 0.0, egui::Color32::from_gray(32));
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Waiting for video...",
                egui::FontId::proportional(24.0),
                egui::Color32::GRAY,
            );
        }
    }

    /// Start background initialization
    fn start_init(&mut self) {
        self.init_state = InitState::InProgress;

        let config = self.config.clone();
        let (tx, rx) = bounded::<InitResult>(1);
        self.init_rx = Some(rx);

        thread::spawn(move || {
            Self::background_init(config, tx);
        });
    }

    /// Background initialization function
    fn background_init(config: Arc<SAideConfig>, result_tx: Sender<InitResult>) {
        let result = (|| -> anyhow::Result<_> {
            // Initialize scrcpy manager
            let mut scrcpy = Scrcpy::new(config.scrcpy.clone());

            if let Err(e) = scrcpy
                .spawn()?
                .wait_for_ready(Duration::from_secs(config.timeout))
            {
                scrcpy.terminate().ok();
                return Err(e);
            }

            // Initialize ADB shell
            let mut adb_shell = AdbShell::new();
            adb_shell.connect()?;
            let adb_shell = Arc::new(RwLock::new(adb_shell));

            // Initialize keyboard mapper
            let keyboard_mapper = if let Some(keyboard_config) = config.mappings.keyboard.clone() {
                Some(KeyboardMapper::new(
                    keyboard_config.clone(),
                    adb_shell.clone(),
                ))
            } else {
                None
            };

            // Initialize mouse mapper
            let mouse_mapper = MouseMapper::new(
                config
                    .mappings
                    .mouse
                    .clone()
                    .unwrap_or(Arc::new(MouseConfig::default())),
                adb_shell.clone(),
            );

            let physical_size = AdbShell::get_physical_screen_size()?;

            // Channel for frame transfer
            let (tx, rx) = bounded::<Arc<Yu12Frame>>(2);

            let mut capture = match V4l2Capture::new(
                &config.scrcpy.v4l2.device,
                Duration::from_secs(config.timeout),
            ) {
                Ok(c) => c,
                Err(e) => {
                    error!("Failed to initialize V4L2 capture");

                    // Terminate scrcpy process
                    if let Err(te) = scrcpy.terminate() {
                        error!("Failed to terminate scrcpy process: {}", te);
                    }

                    return Err(e);
                }
            };

            let (width, height) = capture.dimensions();
            info!("Capture started: {}x{}", width, height);

            // Start capture thread
            let _ = thread::spawn(move || {
                loop {
                    match capture.capture_frame() {
                        Ok(frame) => {
                            let _ = tx.try_send(Arc::new(frame));
                        }
                        Err(e) => {
                            error!("Capture error: {}", e);
                            break;
                        }
                    }
                }
            });

            Ok((
                scrcpy,
                adb_shell,
                keyboard_mapper,
                Some(mouse_mapper),
                rx,
                physical_size,
                width,
                height,
            ))
        })();

        let _ = result_tx.send(result);
    }

    /// Draw the base UI panels (toolbar and status bar)
    fn draw_base_ui(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("Toolbar")
            .frame(egui::Frame::NONE.fill(BG_COLOR))
            .resizable(false)
            .exact_width(TOOLBAR_WIDTH)
            .show(ctx, |ui| {
                self.draw_toolbar(ui);
            });

        egui::TopBottomPanel::top("Status Bar")
            .frame(egui::Frame::NONE.fill(egui::Color32::from_gray(50)))
            .resizable(false)
            .exact_height(STATUSBAR_HEIGHT)
            .show(ctx, |ui| {
                self.draw_statusbar(ui);
            });
    }

    /// Draw the main UI with video content
    fn draw_main_ui(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                self.draw_v4l2_player(ui);
            });
    }

    pub fn start(config: &SAideConfig) -> anyhow::Result<()> {
        // Run main GUI application
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_title("SAide")
                .with_inner_size([
                    DEFAULT_WIDTH as f32 + TOOLBAR_WIDTH,
                    DEFAULT_HEIGHT as f32 + STATUSBAR_HEIGHT,
                ]),
            renderer: eframe::Renderer::Wgpu,
            wgpu_options: egui_wgpu::WgpuConfiguration {
                // Use AutoVsync to reduce CPU/GPU usage
                present_mode: wgpu::PresentMode::AutoVsync,
                // Request low latency for real-time video
                desired_maximum_frame_latency: Some(1),
                ..Default::default()
            },
            ..Default::default()
        };

        eframe::run_native(
            "SAide",
            options,
            Box::new(move |cc| Ok(Box::new(SAideApp::new(cc, config.clone())))),
        )
        .map_err(|e| anyhow!("eframe error: {}", e))
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
                        // Reset to NotStarted to allow retry
                        self.init_state = InitState::NotStarted;
                    }
                },
            )
        });
    }

    // Check if position is within video rectangle
    fn in_video_rect(&self, pos: &egui::Pos2) -> bool {
        if let Some(video_rect) = &self.video_rect {
            pos.x >= video_rect.left()
                && pos.x <= video_rect.right()
                && pos.y >= video_rect.top()
                && pos.y <= video_rect.bottom()
        } else {
            false
        }
    }

    // Convert absolute position to video-relative coordinates
    fn video_relative_coords(&self, pos: egui::Pos2) -> Option<(f32, f32)> {
        if let Some(video_rect) = &self.video_rect {
            let rel_x = pos.x - video_rect.left();
            let rel_y = pos.y - video_rect.top();
            Some((rel_x, rel_y))
        } else {
            None
        }
    }

    pub fn coordinate_transform(
        &self,
        rel_x: f32,
        rel_y: f32,
        screen_size: (u32, u32),
    ) -> Option<(u32, u32)> {
        let video_width = self.video_width as f32;
        let video_height = self.video_height as f32;

        // Apply rotation transform to video coordinates
        let (rotated_x, rotated_y) = match self.rotation % 4 {
            // 0 degrees - no rotation
            0 => (rel_x, rel_y),
            // 90 degrees clockwise - transpose and flip X
            1 => (video_height - rel_y, rel_x),
            // 180 degrees - flip both axes
            2 => (video_width - rel_x, video_height - rel_y),
            // 270 degrees clockwise - transpose and flip Y
            3 => (rel_y, video_width - rel_x),
            _ => (rel_x, rel_y),
        };

        // Direct mapping from video space to device space
        let device_x = rotated_x as f32 / video_width as f32 * screen_size.0 as f32;
        let device_y = rotated_y as f32 / video_height as f32 * screen_size.1 as f32;

        Some((device_x as u32, device_y as u32))
    }

    /// Process input events for mouse and keyboard
    fn process_input_events(&mut self, ctx: &egui::Context) {
        if self.init_state != InitState::Ready {
            return;
        }

        ctx.input(|input| {
            // Handle mouse button events
            for event in &input.events {
                if let Some(adb_shell) = &self.adb_shell
                    && let Some(phisical_size) = self.phisical_size
                    && let Some(mouse_mapper) = self.mouse_mapper.as_ref()
                    && let Some(keyboard_mapper) = self.keyboard_mapper.as_ref()
                    && self.video_rect.is_some()
                {
                    if let egui::Event::PointerButton {
                        button,
                        pressed,
                        pos,
                        ..
                    } = event
                    {
                        if !self.mouse_mapping_enabled {
                            continue;
                        }
                        if !self.in_video_rect(pos) {
                            continue;
                        }

                        let screen_size = match self.phisical_size {
                            Some(size) => size,
                            None => continue,
                        };

                        let (rel_x, rel_y) = self.video_relative_coords(*pos).unwrap();
                        if let Some((device_x, device_y)) =
                            self.coordinate_transform(rel_x, rel_y, screen_size)
                        {
                            let button = MouseButton::from(*button);
                            mouse_mapper.handle_button_event(button, *pressed, device_x, device_y);
                        }
                    } else if let egui::Event::MouseWheel {
                        modifiers: _,
                        unit: _,
                        delta,
                        ..
                    } = event
                    {
                        if !self.mouse_mapping_enabled {
                            continue;
                        }

                        // get mouse position
                        let pos = input.pointer.hover_pos().unwrap_or_default();

                        if !self.in_video_rect(&pos) {
                            continue;
                        }

                        let screen_size = match self.phisical_size {
                            Some(size) => size,
                            None => continue,
                        };
                        let (rel_x, rel_y) = self.video_relative_coords(pos).unwrap();
                        let (device_x, device_y) =
                            match self.coordinate_transform(rel_x, rel_y, screen_size) {
                                Some(coords) => coords,
                                None => continue,
                            };

                        let dir = match delta {
                            egui::Vec2 { x: _, y } if *y > 0.0 => WheelDirection::Up,
                            _ => WheelDirection::Down,
                        };

                        if let Err(e) = mouse_mapper.handle_wheel_event(device_x, device_y, dir) {
                            error!("Failed to handle wheel event: {}", e);
                        }
                    } else if let egui::Event::Key {
                        key,
                        pressed,
                        modifiers: _,
                        repeat: _,
                        physical_key: _,
                    } = event
                    {
                        if let Err(e) = keyboard_mapper.handle_key_event(key, *pressed) {
                            error!("Failed to handle keyboard event: {}", e);
                        }
                    }
                }
            }
        });

        // Keep ADB connection alive
        // if let Some(adb) = &self.adb {
        //     adb.keep_alive();
        // }
    }
}

impl eframe::App for SAideApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Draw base UI (toolbar and status bar) - always visible
        self.draw_base_ui(ctx);

        // Handle initialization state transitions
        match self.init_state {
            InitState::NotStarted => {
                // UI is ready, now start background initialization
                self.start_init();
            }
            InitState::InProgress => {
                self.draw_loading_overlay(ctx, "Initializing...");
            }
            InitState::Ready => {
                // Fully initialized, show main UI
                self.draw_main_ui(ctx);
            }
            InitState::Failed => {
                // Failed state, waiting for user action
                self.draw_error_overlay(ctx, "Initialization failed.");
            }
        }

        // Check for initialization results
        if let Some(rx) = &self.init_rx
            && let Ok(init_result) = rx.try_recv()
        {
            match init_result {
                Ok((scrcpy, adb, frame_rx, width, height)) => {
                    // Initialization successful
                    self.scrcpy = Some(scrcpy);

                    self.mouse_mapper = Some(mouse_mapper);

                    self.keyboard_mapper = Some(keyboard_mapper);

                    self.frame_rx = Some(frame_rx);
                    self.video_width = width;
                    self.video_height = height;

                    self.init_state = InitState::Ready;

                    self.resize(ctx);
                }
                Err(e) => {
                    // Initialization failed
                    error!("Failed to initialize application: {}", e);
                    self.init_state = InitState::Failed;
                }
            }
        }

        // Handle input events
        self.process_input_events(ctx);

        ctx.request_repaint();
    }
}

impl Drop for SAideApp {
    fn drop(&mut self) {
        if let Some(scrcpy) = &mut self.scrcpy {
            if let Err(e) = scrcpy.terminate() {
                error!("Failed to terminate scrcpy process: {}", e);
            } else {
                info!("Scrcpy process terminated");
            }
        }

        // Disconnect ADB shell
        if let Some(adb) = &self.adb
            && let Err(e) = adb.lock().unwrap().disconnect()
        {
            error!("Failed to disconnect ADB shell: {}", e);
        }
    }
}
