use {
    super::{
        super::utils::{CoordinatesTransformParams, find_nearest_mapping, screen_to_device_coords},
        mapping::{MappingConfigEvent, MappingConfigWindow},
        status_bar::{StatusBar, StatusBarEvent},
        toolbar::{Toolbar, ToolbarEvent},
    },
    crate::{
        config::{
            ConfigManager,
            SAideConfig,
            mapping::{AdbAction, Key, MouseButton, WheelDirection},
        },
        controller::{
            adb::AdbShell,
            keyboard::KeyboardMapper,
            mouse::{MouseMapper, MouseState},
            scrcpy::Scrcpy,
        },
        v4l2::{V4l2Capture, Yu12Frame, YuvRenderResources, new_yuv_render_callback},
    },
    anyhow::anyhow,
    crossbeam_channel::{Receiver, bounded},
    eframe::{
        egui::{self, Color32, RichText},
        egui_wgpu,
    },
    std::{
        ffi::OsStr,
        process::Command,
        sync::Arc,
        thread,
        time::{Duration, Instant},
    },
    sysinfo::{ProcessesToUpdate, System},
    tracing::{debug, error, info, trace, warn},
};

const DEFAULT_WIDTH: u32 = 1280;
const DEFAULT_HEIGHT: u32 = 720;

const BG_COLOR: Color32 = Color32::from_rgb(32, 32, 32);
const FG_COLOR: Color32 = Color32::from_rgb(220, 220, 220);

const DEVICE_MONITOR_POLL_INTERVAL_MS: u64 = 1000;
const DEVICE_MONITOR_CHANNEL_CAPACITY: usize = 1;
enum DeviceMonitorEvent {
    /// Device rotated event with new orientation (0-3), clockwise
    Rotated(u32),
    /// Device input method (IME) state changed, true = shown, false = hidden
    ImStateChanged(bool),
}

const INIT_RESULT_CHANNEL_CAPACITY: usize = 9;

/// Initialization result type
enum InitResult {
    Scrcpy(Scrcpy),
    KeyboardMapper(Option<KeyboardMapper>),
    MouseMapper(Option<MouseMapper>),
    DeviceMonitor(Receiver<DeviceMonitorEvent>),
    FrameReceiver(Receiver<Arc<Yu12Frame>>),
    DeviceId(String),
    PhysicalSize((u32, u32)),
    VideoDimensions((u32, u32)),
    CaptureOrientation(u32),
    Error(anyhow::Error),
}

/// Initialization state enum
#[derive(PartialEq)]
enum InitState {
    NotStarted,
    InProgress,
    Ready,
    Failed,
}

/// Main UI state
pub struct SAideApp {
    toolbar: Toolbar,
    status_bar: StatusBar,

    /// Configuration manager
    config_manager: ConfigManager,

    /// Scrcpy process manager
    scrcpy: Option<Scrcpy>,

    /// Mouse input mapper
    mouse_mapper: Option<MouseMapper>,

    /// Keyboard input mapper
    keyboard_mapper: Option<KeyboardMapper>,

    /// Device monitor receiver
    device_monitor_rx: Option<Receiver<DeviceMonitorEvent>>,

    /// Video frame receiver
    frame_rx: Option<Receiver<Arc<Yu12Frame>>>,
    /// Latest video frame
    frame: Option<Arc<Yu12Frame>>,
    /// Timestamp of last received frame
    last_frame_instant: Option<Instant>,

    // Flag indicating new frame availability
    has_new_frame: bool,

    /// Timestamp of last paint
    last_paint_instant: Option<Instant>,

    /// Device ID
    device_id: Option<String>,

    /// Device physical screen size
    device_physical_size: (u32, u32),

    /// Device orientation (0-3), clockwise
    device_orientation: u32,

    // V4l2 capture orientation (0-3), counter-clockwise
    capture_orientation: u32,

    // Video render rotation state (0-3), clockwise
    video_rotation: u32,

    /// Video display rectangle in egui coordinates
    video_rect: egui::Rect,

    video_original_width: u32,
    video_original_height: u32,

    // Frame rate limiter duration
    frame_rate_limiter: Option<Duration>,

    /// Initialization state machine
    init_state: InitState,
    init_instant: Option<Instant>,
    init_rx: Option<Receiver<InitResult>>,

    /// Keyboard mapping switch
    keyboard_enabled: bool,

    /// Mouse mapping switch
    mouse_enabled: bool,

    /// Last mouse pointer position in video rect
    last_pointer_pos: egui::Pos2,

    /// Keyboard custom mapping switch
    keyboard_custom_mapping_enabled: bool,

    /// Android device input method state
    device_ime_state: bool,

    /// Mapping configuration window
    mapping_config_window: MappingConfigWindow,
}

impl SAideApp {
    pub fn new(cc: &eframe::CreationContext<'_>, config_manager: ConfigManager) -> Self {
        let config = config_manager.config();

        // Register YUV render resources with wgpu renderer
        if let Some(wgpu_state) = cc.wgpu_render_state.as_ref() {
            let resources = YuvRenderResources::new(&wgpu_state.device, wgpu_state.target_format);
            wgpu_state
                .renderer
                .write()
                .callback_resources
                .insert(resources);
        }

        let keyboard_enabled = config.general.keyboard_enabled;
        let mouse_enabled = config.general.mouse_enabled;
        let keyboard_custom_mapping_enabled = config.mappings.initial_state;

        let max_fps = config.scrcpy.video.max_fps;
        let vsync = config.gpu.vsync;

        Self {
            toolbar: Toolbar::new(),
            status_bar: StatusBar::new(max_fps as f32),

            config_manager,

            scrcpy: None,

            mouse_mapper: None,

            keyboard_mapper: None,

            device_monitor_rx: None,

            frame_rx: None,
            frame: None,
            last_frame_instant: None,

            has_new_frame: false,

            last_paint_instant: None,

            device_id: None,

            device_physical_size: (0, 0),

            device_orientation: 0,

            video_original_width: 0,
            video_original_height: 0,

            capture_orientation: 0,

            video_rect: egui::Rect::NOTHING,

            video_rotation: 0,

            frame_rate_limiter: if vsync {
                None
            } else {
                Some(Duration::from_millis(u64::from(1000 / max_fps)))
            },

            init_state: InitState::NotStarted,
            init_instant: None,
            init_rx: None,

            keyboard_enabled,
            mouse_enabled,

            last_pointer_pos: egui::Pos2::ZERO,

            keyboard_custom_mapping_enabled,

            device_ime_state: false,

            mapping_config_window: MappingConfigWindow::new(),
        }
    }

    // Get current configuration
    pub fn config(&self) -> Arc<SAideConfig> { self.config_manager.config() }

    /// Background initialization function
    fn init(&mut self) {
        self.init_state = InitState::InProgress;
        self.init_instant = Some(Instant::now());

        let (tx, rx) = bounded::<InitResult>(INIT_RESULT_CHANNEL_CAPACITY);
        self.init_rx = Some(rx);

        // Scrcpy initialization
        let config = self.config();
        let scrcpy_tx = tx.clone();
        thread::spawn(move || -> Result<(), anyhow::Error> {
            // Ensure no existing scrcpy process is running
            let mut sys = System::new_all();
            sys.refresh_processes(ProcessesToUpdate::All, true);

            if sys.processes().values().any(|process| {
                process.exe().and_then(|path| path.file_name()) == Some(OsStr::new("scrcpy"))
            }) {
                scrcpy_tx.send(InitResult::Error(anyhow!(
                    "Existing scrcpy process detected , please terminate it first",
                )))?;

                // Early return on error
                return Ok(());
            }

            // Initialize scrcpy manager
            let mut scrcpy = Scrcpy::new(Arc::clone(&config.scrcpy));

            if let Err(e) = scrcpy
                .spawn()?
                .wait_for_ready(Duration::from_secs(config.general.init_timeout as u64))
            {
                scrcpy.terminate().ok();
                scrcpy_tx.send(InitResult::Error(anyhow!("Failed to start scrcpy: {}", e)))?;
            }
            debug!("Scrcpy process started and ready");

            scrcpy_tx.send(InitResult::Scrcpy(scrcpy))?;
            Ok(())
        });

        // Delay to allow scrcpy to start ADB server
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(500) {
            // check if adb is responsive
            if Command::new("adb")
                .args(["shell", "echo", "ok"])
                .output()
                .is_ok()
            {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }

        // Device monitor initialization
        let dm_tx = tx.clone();
        thread::spawn(move || -> Result<(), anyhow::Error> {
            // Get device ID
            let device_id = AdbShell::get_device_id()?;
            debug!("Using device ID: {}", device_id);
            dm_tx.send(InitResult::DeviceId(device_id))?;

            // Get device physical screen size
            let physical_size = AdbShell::get_physical_screen_size()?;
            debug!(
                "Device physical screen size: {}x{}",
                physical_size.0, physical_size.1
            );
            dm_tx.send(InitResult::PhysicalSize(physical_size))?;

            // Create channel for rotation events
            let (event_tx, event_rx) =
                bounded::<DeviceMonitorEvent>(DEVICE_MONITOR_CHANNEL_CAPACITY);
            dm_tx.send(InitResult::DeviceMonitor(event_rx))?;

            // Start rotation and im state monitoring
            let mut last_rotation = None;
            loop {
                match AdbShell::get_screen_orientation() {
                    Ok(current_rotation) => {
                        if Some(current_rotation) != last_rotation {
                            debug!(
                                "Rotation changed: {:?} -> {}",
                                last_rotation, current_rotation
                            );
                            last_rotation = Some(current_rotation);

                            // Send rotation event
                            if let Err(e) =
                                event_tx.send(DeviceMonitorEvent::Rotated(current_rotation))
                            {
                                error!("Failed to send rotation event: {}", e);
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to get screen orientation: {}", e);
                    }
                }

                // Poll input method state
                if let Ok(im_state) = AdbShell::get_ime_state() {
                    event_tx
                        .send(DeviceMonitorEvent::ImStateChanged(im_state))
                        .unwrap_or_else(|e| {
                            error!("Failed to send IME state event: {}", e);
                        });
                }

                thread::sleep(Duration::from_millis(DEVICE_MONITOR_POLL_INTERVAL_MS));
            }

            Ok(())
        });

        let kbd_config = self.config();
        let kbd_tx = tx.clone();
        thread::spawn(move || -> Result<(), anyhow::Error> {
            // Initialize keyboard mapper
            let keyboard_mapper = kbd_config
                .general
                .keyboard_enabled
                .then_some(KeyboardMapper::new(kbd_config.mappings.clone()))
                .transpose()?;
            debug!("Keyboard mapper initialized");

            kbd_tx.send(InitResult::KeyboardMapper(keyboard_mapper))?;
            Ok(())
        });

        let mouse_config = self.config();
        let mouse_tx = tx.clone();
        thread::spawn(move || -> Result<(), anyhow::Error> {
            // Initialize mouse mapper
            let mouse_mapper = mouse_config
                .general
                .mouse_enabled
                .then_some(MouseMapper::new())
                .transpose()?;
            debug!("Mouse mapper initialized");

            mouse_tx.send(InitResult::MouseMapper(mouse_mapper))?;
            Ok(())
        });

        // V4L2 capture initialization
        let v4l2_config = self.config();
        let v4l2_tx = tx.clone();
        thread::spawn(move || -> Result<(), anyhow::Error> {
            // Channel for frame transfer
            let (tx, rx) = bounded::<Arc<Yu12Frame>>(1);

            let mut capture = match V4l2Capture::new(
                &v4l2_config.scrcpy.v4l2.device,
                Duration::from_secs(u64::from(v4l2_config.general.init_timeout)),
            ) {
                Ok(c) => c,
                Err(e) => {
                    error!("Failed to initialize V4L2 capture");
                    return Err(e);
                }
            };
            debug!("V4L2 capture initialized");

            // Get capture dimensions and orientation
            let (width, height) = capture.dimensions();
            let capture_orientation = v4l2_config.scrcpy.v4l2.capture_orientation / 90;
            debug!(
                "V4L2 capture dimensions: {}x{}, orientation: {}",
                width,
                height,
                capture_orientation * 90
            );

            // Start capture thread
            let _ = thread::spawn(move || {
                loop {
                    match capture.capture_frame() {
                        Ok(frame) => {
                            let _ = tx.send(Arc::new(frame));
                        }
                        Err(e) => {
                            error!("Capture error: {}", e);
                            break;
                        }
                    }
                }
            });
            debug!("V4L2 capture thread started");

            v4l2_tx.send(InitResult::FrameReceiver(rx))?;
            v4l2_tx.send(InitResult::VideoDimensions((width, height)))?;
            v4l2_tx.send(InitResult::CaptureOrientation(capture_orientation))?;
            Ok(())
        });
    }

    /// Check initialization progress and update state
    fn check_init_stage(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.init_rx {
            while let Ok(result) = rx.try_recv() {
                match result {
                    InitResult::Scrcpy(scrcpy) => {
                        self.scrcpy = Some(scrcpy);
                    }
                    InitResult::KeyboardMapper(keyboard_mapper) => {
                        self.keyboard_mapper = keyboard_mapper;
                    }
                    InitResult::MouseMapper(mouse_mapper) => {
                        self.mouse_mapper = mouse_mapper;
                    }
                    InitResult::DeviceMonitor(_device_monitor_rx) => {
                        self.device_monitor_rx = Some(_device_monitor_rx);
                    }
                    InitResult::FrameReceiver(frame_rx) => {
                        self.frame_rx = Some(frame_rx);
                    }
                    InitResult::DeviceId(device_id) => {
                        self.device_id = Some(device_id);
                    }
                    InitResult::PhysicalSize(size) => self.device_physical_size = size,
                    InitResult::VideoDimensions((width, height)) => {
                        self.video_original_width = width;
                        self.video_original_height = height;
                        self.status_bar.set_video_resolution(width, height);
                    }
                    InitResult::CaptureOrientation(orientation) => {
                        self.capture_orientation = orientation;
                        self.status_bar.set_capture_orientation(orientation);
                    }
                    InitResult::Error(e) => {
                        error!("Initialization error: {}", e);
                        self.init_state = InitState::Failed;
                        return;
                    }
                }
            }

            // Check if all components are initialized
            if self.scrcpy.is_some()
                && self.device_monitor_rx.is_some()
                && self.frame_rx.is_some()
                && self.video_original_width > 0
                && self.video_original_height > 0
            {
                self.init_state = InitState::Ready;
                info!("Initialization completed successfully");

                // Resize window to match video dimensions
                self.resize(ctx);
            }
        }
    }

    /// Get effective video dimensions considering rotation
    fn effective_dimensions(&self) -> (u32, u32) {
        if self.video_rotation & 1 == 0 {
            (self.video_original_width, self.video_original_height)
        } else {
            (self.video_original_height, self.video_original_width)
        }
    }

    /// Resize the application window to match video dimensions
    fn resize(&mut self, ctx: &egui::Context) {
        let (w, h) = self.effective_dimensions();
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
            w as f32 + Toolbar::width(),
            h as f32 + StatusBar::height(),
        )));
    }

    /// Rotate video by 90 degrees clockwise
    fn rotate(&mut self, ctx: &egui::Context) {
        self.video_rotation = (self.video_rotation + 1) % 4;
        self.status_bar.set_video_rotation(self.video_rotation);
        self.resize(ctx);
    }

    /// Toggle mapping configuration window
    fn toggle_mapping_config(&mut self, _ctx: &egui::Context) {
        self.mapping_config_window.toggle();
    }

    pub fn start(config_manager: ConfigManager) -> anyhow::Result<()> {
        // Run main GUI application
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_title("SAide")
                .with_inner_size([
                    DEFAULT_WIDTH as f32 + Toolbar::width(),
                    DEFAULT_HEIGHT as f32 + StatusBar::height(),
                ]),
            renderer: eframe::Renderer::Wgpu,
            wgpu_options: egui_wgpu::WgpuConfiguration {
                // Use AutoVsync/AutoNoVsync based on config
                present_mode: if config_manager.config().gpu.vsync {
                    wgpu::PresentMode::AutoVsync
                } else {
                    wgpu::PresentMode::AutoNoVsync
                },

                wgpu_setup: egui_wgpu::WgpuSetup::from(egui_wgpu::WgpuSetupCreateNew {
                    instance_descriptor: wgpu::InstanceDescriptor {
                        backends: (&config_manager.config().gpu.backend).into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),

                // Request low latency for real-time video
                desired_maximum_frame_latency: Some(1),

                ..Default::default()
            },
            ..Default::default()
        };

        eframe::run_native(
            "SAide",
            options,
            Box::new(move |cc| Ok(Box::new(SAideApp::new(cc, config_manager)))),
        )
        .map_err(|e| anyhow!("eframe error: {}", e))
    }

    // Check if position is within video rectangle
    fn is_in_video_rect(&self, pos: &egui::Pos2) -> bool {
        pos.x >= self.video_rect.left()
            && pos.x <= self.video_rect.right()
            && pos.y >= self.video_rect.top()
            && pos.y <= self.video_rect.bottom()
    }

    /// Process device monitor events
    fn process_device_monitor_events(&mut self) {
        if self.init_state != InitState::Ready {
            debug!("Skipping device monitor processing - not initialized");
            return;
        }

        let mut rotated = false;
        if let Some(rx) = &self.device_monitor_rx {
            while let Ok(event) = rx.try_recv() {
                match event {
                    DeviceMonitorEvent::Rotated(new_orientation) => {
                        debug!("Device rotated to orientation: {}", new_orientation * 90);
                        self.device_orientation = new_orientation;
                        rotated = true;
                    }
                    DeviceMonitorEvent::ImStateChanged(im_state) => {
                        if im_state != self.device_ime_state {
                            debug!("Device IME state changed: {}", im_state);
                            self.device_ime_state = im_state;
                        }
                    }
                }
            }
        }

        // Refresh keyboard profiles if needed
        if rotated {
            self.refresh_mapping_profiles();
            self.status_bar
                .set_device_orientation(self.device_orientation);
        }
    }

    /// Process mapping configuration window events
    fn process_mapping_config_events(&mut self, ctx: &egui::Context) {
        if self.init_state != InitState::Ready || !self.mapping_config_window.is_visible() {
            return;
        }

        // Get current mappings for display
        let mappings = self
            .keyboard_mapper
            .as_ref()
            .unwrap()
            .get_active_profile()
            .map(|profile| profile.mappings.clone())
            .unwrap_or_default();

        // Draw the config window and handle events
        let event =
            self.mapping_config_window
                .draw(ctx, &mappings, &self.coodinates_transform_params());

        match event {
            MappingConfigEvent::Close => {
                self.mapping_config_window.hide();
            }
            MappingConfigEvent::RequestAddMapping(screen_pos) => {
                // Convert screen position to device coordinates
                if let Some(device_pos) =
                    screen_to_device_coords(&screen_pos, &self.coodinates_transform_params())
                {
                    info!("Requesting to add mapping at device pos: {:?}", device_pos);
                    self.mapping_config_window.request_input_dialog(device_pos);
                }
            }
            MappingConfigEvent::RequestDeleteMapping(screen_pos) => {
                // Find nearest mapping to delete
                if let Some((nearest_key, nearest_pos)) =
                    screen_to_device_coords(&screen_pos, &self.coodinates_transform_params())
                        .and_then(|device_pos| find_nearest_mapping(device_pos, &mappings))
                {
                    info!(
                        "Requesting to delete mapping: {:?} at {:?}",
                        nearest_key, nearest_pos
                    );
                    self.mapping_config_window
                        .request_delete_dialog(nearest_key, nearest_pos);
                }
            }
            MappingConfigEvent::None => {}
        }

        // Handle dialogs
        if self.mapping_config_window.is_input_dialog_open()
            && let Some(pending_pos) = self.mapping_config_window.get_pos()
            && let Some(key) = self
                .mapping_config_window
                .show_key_input_dialog(ctx, pending_pos)
        {
            if let Some(action) = self.get_mapping(&key)
                && let AdbAction::Tap { x, y } = action
            {
                self.mapping_config_window
                    .request_override_dialog(key, (x, y), pending_pos);
            } else {
                self.add_mapping(key, pending_pos);
            }
        }
        if self.mapping_config_window.is_delete_dialog_open()
            && let Some((key, pos)) = self.mapping_config_window.get_delete_target()
            && let Some(confirmed) = self
                .mapping_config_window
                .show_delete_confirm_dialog(ctx, key, pos)
            && confirmed
        {
            self.delete_mapping(key);
        }
        if self.mapping_config_window.is_override_dialog_open()
            && let Some((key, pos, new_pos)) = self.mapping_config_window.get_override_target()
            && let Some(confirmed) = self
                .mapping_config_window
                .show_override_confirm_dialog(ctx, key, pos, new_pos)
            && confirmed
        {
            self.add_mapping(key, new_pos);
        }
    }

    /// Add a new mapping
    fn add_mapping(&mut self, key: Key, device_pos: (u32, u32)) {
        info!("Adding mapping: {:?} -> {:?}", key, device_pos);

        let Some(keyboard_mapper) = &self.keyboard_mapper else {
            error!("Keyboard mapper not initialized");
            return;
        };

        if let Some(profile) = keyboard_mapper.get_active_profile() {
            // Create new profile with added mapping
            let action = AdbAction::Tap {
                x: device_pos.0,
                y: device_pos.1,
            };

            profile.add_mapping(key, action);

            // Save to config file
            if let Err(e) = self.config_manager.save() {
                error!("Failed to save config: {}", e);
            } else {
                info!("Mapping saved successfully");
            }
        }
    }

    /// Delete a mapping
    fn delete_mapping(&mut self, key: Key) {
        info!("Deleting mapping: {:?}", key);

        let Some(keyboard_mapper) = &self.keyboard_mapper else {
            error!("Keyboard mapper not initialized");
            return;
        };

        if let Some(profile) = keyboard_mapper.get_active_profile() {
            // Create new profile with removed mapping
            profile.remove_mapping(&key);

            // Save to config file
            if let Err(e) = self.config_manager.save() {
                error!("Failed to save config: {}", e);
            } else {
                info!("Mapping deleted successfully");
            }
        }
    }

    fn get_mapping(&self, key: &Key) -> Option<AdbAction> {
        let Some(keyboard_mapper) = &self.keyboard_mapper else {
            return None;
        };

        if let Some(profile) = keyboard_mapper.get_active_profile() {
            profile.get_mapping(key)
        } else {
            None
        }
    }

    /// Process keyboard event
    fn process_keyboard_event(
        &self,
        keyboard_mapper: &KeyboardMapper,
        key: &egui::Key,
        pressed: bool,
        modifiers: egui::Modifiers,
    ) {
        if !pressed {
            return;
        }

        debug!(
            "Processing keyboard event: key={:?}, modifiers={:?}",
            key, modifiers
        );

        let result = if self.keyboard_custom_mapping_enabled && !self.device_ime_state {
            keyboard_mapper.handle_custom_keymapping_event(key, pressed)
        } else if modifiers.any() {
            keyboard_mapper.handle_keycombo_event(modifiers, key)
        } else {
            keyboard_mapper.handle_standard_key_event(key)
        };

        if let Err(e) = result {
            error!("Failed to handle keyboard event: {}", e);
        }
    }

    /// Process mouse button event
    fn process_mouse_button_event(
        &self,
        mouse_mapper: &MouseMapper,
        button: egui::PointerButton,
        pressed: bool,
        pos: &egui::Pos2,
    ) {
        if !self.is_in_video_rect(pos) {
            return;
        }

        debug!("Processing mouse button event: {:?} at {:?}", button, pos);

        let Some((device_x, device_y)) =
            screen_to_device_coords(pos, &self.coodinates_transform_params())
        else {
            return;
        };

        let button = MouseButton::from(button);
        if let Err(e) = mouse_mapper.handle_button_event(button, pressed, device_x, device_y) {
            error!("Failed to handle mouse button event: {}", e);
        } else {
            debug!(
                "Mouse button event at device coords: ({}, {})",
                device_x, device_y
            );
        }
    }

    /// Process mouse move event
    fn process_mouse_move_event(
        &self,
        mouse_mapper: &MouseMapper,
        pos: &egui::Pos2,
        last_pointer_pos: &egui::Pos2,
    ) -> Option<egui::Pos2> {
        if self.is_in_video_rect(pos) {
            trace!("PointerMoved inside video rect at {:?}", pos);

            if let Some((device_x, device_y)) =
                screen_to_device_coords(pos, &self.coodinates_transform_params())
            {
                if let Err(e) = mouse_mapper.handle_move_event(device_x, device_y) {
                    error!("Failed to handle mouse move event: {}", e);
                }
            } else {
                debug!(
                    "Failed to transform coordinates for PointerMoved at {:?}",
                    pos
                );
            }

            Some(*pos)
        } else {
            trace!("PointerMoved outside video rect at {:?}", pos);

            // If dragging and moved outside, send a button release
            if mouse_mapper.get_button_state() != MouseState::Idle
                && let Some((device_x, device_y)) =
                    screen_to_device_coords(last_pointer_pos, &self.coodinates_transform_params())
                && let Err(e) =
                    mouse_mapper.handle_button_event(MouseButton::Left, false, device_x, device_y)
            {
                error!("Failed to handle mouse button release event: {}", e);
            }

            None
        }
    }

    /// Process mouse wheel event
    fn process_mouse_wheel_event(
        &self,
        mouse_mapper: &MouseMapper,
        delta: &egui::Vec2,
        pointer_pos: egui::Pos2,
    ) {
        if !self.is_in_video_rect(&pointer_pos) {
            return;
        }

        debug!(
            "Processing mouse wheel event: {:?} at {:?}",
            delta, pointer_pos
        );

        let Some((device_x, device_y)) =
            screen_to_device_coords(&pointer_pos, &self.coodinates_transform_params())
        else {
            return;
        };

        let dir = if delta.y < 0.0 {
            WheelDirection::Up
        } else {
            WheelDirection::Down
        };

        if let Err(e) = mouse_mapper.handle_wheel_event(device_x, device_y, &dir) {
            error!("Failed to handle wheel event: {}", e);
        } else {
            debug!(
                "Mouse wheel event at device coords: ({}, {})",
                device_x, device_y
            );
        }
    }

    /// Process input events for mouse and keyboard
    fn process_input_events(&mut self, ctx: &egui::Context) {
        if self.init_state != InitState::Ready {
            trace!("Skipping input processing - not initialized");
            return;
        }

        // Skip normal input processing if mapping config window is open or dialogs are open
        if self.mapping_config_window.visible
            || self.mapping_config_window.is_input_dialog_open()
            || self.mapping_config_window.is_delete_dialog_open()
        {
            return;
        }

        // Update mouse state (check for long press and send drag updates)
        if self.mouse_enabled
            && let Some(mouse_mapper) = self.mouse_mapper.as_ref()
            && let Err(e) = mouse_mapper.update()
        {
            error!("Failed to update mouse mapper: {}", e);
        }

        ctx.input(|input| {
            for event in &input.events {
                // Process keyboard events
                if let egui::Event::Key {
                    key,
                    pressed,
                    modifiers,
                    ..
                } = event
                    && self.keyboard_enabled
                    && let Some(ref keyboard_mapper) = self.keyboard_mapper
                {
                    self.process_keyboard_event(keyboard_mapper, key, *pressed, *modifiers);
                }

                // Process mouse events
                if !self.mouse_enabled {
                    continue;
                }
                let mouse_mapper = match &self.mouse_mapper {
                    Some(m) => m,
                    None => continue,
                };

                match event {
                    egui::Event::PointerButton {
                        button,
                        pressed,
                        pos,
                        ..
                    } => {
                        self.process_mouse_button_event(mouse_mapper, *button, *pressed, pos);
                    }
                    egui::Event::PointerMoved(pos) => {
                        if let Some(new_pos) =
                            self.process_mouse_move_event(mouse_mapper, pos, &self.last_pointer_pos)
                        {
                            self.last_pointer_pos = new_pos;
                        }
                    }
                    egui::Event::MouseWheel { delta, .. } => {
                        let pointer_pos = input.pointer.hover_pos().unwrap_or_default();
                        self.process_mouse_wheel_event(mouse_mapper, delta, pointer_pos);
                    }
                    _ => {}
                }
            }
        });
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
                let fps = 0.95 * self.status_bar.fps() + 0.05 * (1.0 / elapsed);
                self.status_bar.set_fps(fps);
            }
        }

        if self.has_new_frame {
            self.last_frame_instant = Some(Instant::now());
        }
    }

    fn refresh_mapping_profiles(&mut self) {
        let (keyboard_mapper, device_id) =
            match (self.keyboard_mapper.as_mut(), self.device_id.as_ref()) {
                (Some(km), Some(did)) => (km, did),
                _ => {
                    debug!("Keyboard mapper or device ID not available for profile refresh");
                    self.status_bar.reset_profiles();
                    return;
                }
            };

        match keyboard_mapper.refresh_profiles(device_id, self.device_orientation) {
            Ok(_) => {
                let avail_profile_names = keyboard_mapper.get_avail_profiles();
                let active_profile_name = keyboard_mapper.get_active_profile_name();
                debug!(
                    "Keyboard profiles refreshed: active={:?}, available={:?}",
                    active_profile_name, avail_profile_names
                );

                self.status_bar.set_available_profiles(avail_profile_names);
                self.status_bar.set_active_profile(active_profile_name);
            }
            Err(e) => {
                self.status_bar.reset_profiles();
                debug!("Failed to refresh keyboard profiles: {}", e);
            }
        }
    }

    /// Draw the base UI panels (toolbar and status bar)
    fn draw_base_ui(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("Toolbar")
            .frame(egui::Frame::NONE.fill(BG_COLOR))
            .resizable(false)
            .exact_width(Toolbar::width())
            .show(ctx, |ui| match self.toolbar.draw(ui) {
                ToolbarEvent::RotateVideo => {
                    self.rotate(ctx);
                }
                ToolbarEvent::ConfigureMappings => {
                    self.toggle_mapping_config(ctx);
                }
                ToolbarEvent::None => {}
            });

        egui::TopBottomPanel::top("Status Bar")
            .frame(egui::Frame::NONE.fill(egui::Color32::from_gray(50)))
            .resizable(false)
            .exact_height(StatusBar::height())
            .show(ctx, |ui| match self.status_bar.draw(ui) {
                StatusBarEvent::ProfileChanged(profile) => {
                    if let Some(keyboard_mapper) = self.keyboard_mapper.as_mut() {
                        keyboard_mapper.load_profile(&profile).unwrap_or_else(|e| {
                            error!("Failed to change active profile to {}: {}", profile, e);
                        });
                    }
                    self.status_bar.set_active_profile(Some(profile));
                }
                StatusBarEvent::None => {}
            });
    }

    /// Draw the main V4L2 video player area
    fn draw_v4l2_player(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
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

    fn coodinates_transform_params(&self) -> CoordinatesTransformParams {
        CoordinatesTransformParams {
            video_rect: self.video_rect,
            video_rotation: self.video_rotation,
            device_physical_size: self.device_physical_size,
            device_orientation: self.device_orientation,
            capture_orientation: self.capture_orientation,
        }
    }
}

impl eframe::App for SAideApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check if there is any input event
        let has_input = ctx.input(|i| !i.events.is_empty() || i.pointer.any_down());

        // Draw base UI (toolbar and status bar) - always visible
        self.draw_base_ui(ctx);

        // Handle initialization state transitions
        match self.init_state {
            InitState::NotStarted => {
                // UI is ready, now start background initialization
                self.init();
            }
            InitState::InProgress => {
                self.draw_loading_overlay(ctx, "Initializing...");

                // Check initialization progress
                self.check_init_stage(ctx);
            }
            InitState::Ready => {
                // Fully initialized, show main UI
                self.draw_v4l2_player(ctx);

                // Receive new video frames
                self.receive_frame();

                // Process mapping configuration window
                self.process_mapping_config_events(ctx);

                // When mapping config window is open, skip normal input processing
                if !self.mapping_config_window.visible {
                    // Handle input events
                    self.process_input_events(ctx);
                }

                // Check for device monitor events
                self.process_device_monitor_events();
            }
            InitState::Failed => {
                // Failed state, waiting for user action
                self.draw_error_overlay(ctx, "Initialization failed.");
            }
        }

        let config = self.config();

        // Frame rate limiting for non-vsync mode
        // Sleep to limit frame rate if no new frame and no input
        if !config.gpu.vsync
            && !self.has_new_frame
            && !has_input
            && let Some(last_paint) = self.last_paint_instant
            && let Some(limit_next_frame_timer) = self.frame_rate_limiter
        {
            let elapsed = last_paint.elapsed();
            if elapsed < limit_next_frame_timer {
                // limit frame rate
                thread::sleep(limit_next_frame_timer - elapsed);
            }
        }

        if !config.gpu.vsync {
            // Update last paint time for frame rate limiting
            self.last_paint_instant = Some(Instant::now());
            self.has_new_frame = false;
        }

        // Request repaint for next frame
        ctx.request_repaint();
    }
}
