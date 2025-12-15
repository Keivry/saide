use {
    super::{
        super::{
            init::{
                DeviceMonitorEvent,
                INIT_RESULT_CHANNEL_CAPACITY,
                InitEvent,
                start_initialization,
            },
            utils::{CoordinatesTransformParams, find_nearest_mapping, screen_to_device_coords},
        },
        indicator::Indicator,
        mapping::{MappingConfigEvent, MappingConfigWindow},
        stream_player::StreamPlayer,
        toolbar::{Toolbar, ToolbarEvent},
    },
    crate::{
        config::{
            ConfigManager,
            SAideConfig,
            mapping::{AdbAction, Key, MouseButton, WheelDirection},
        },
        controller::{
            keyboard::KeyboardMapper,
            mouse::{MouseMapper, MouseState},
        },
    },
    crossbeam_channel::{Receiver, bounded},
    eframe::egui::{self, Color32},
    std::{
        sync::Arc,
        thread,
        time::{Duration, Instant},
    },
    tracing::{debug, error, info, trace},
};

const BG_COLOR: Color32 = Color32::from_rgb(32, 32, 32);

/// Initialization state enum
#[derive(PartialEq)]
enum InitState {
    NotStarted,
    InProgress,
    Ready,
    Failed(String),
}

/// Main UI state
pub struct SAideApp {
    toolbar: Toolbar,

    indicator: Indicator,

    player: StreamPlayer,

    /// Configuration manager
    config_manager: ConfigManager,

    /// Mouse input mapper
    mouse_mapper: Option<MouseMapper>,

    /// Keyboard input mapper
    keyboard_mapper: Option<KeyboardMapper>,

    /// Device monitor receiver
    device_monitor_rx: Option<Receiver<DeviceMonitorEvent>>,

    /// Timestamp of last paint
    last_paint_instant: Option<Instant>,

    /// Device ID
    device_id: Option<String>,

    /// Device physical screen size
    device_physical_size: (u32, u32),

    /// Device orientation (0-3), clockwise
    device_orientation: u32,

    /// Whether window has been initially sized to video
    window_initialized: bool,

    // Frame rate limiter duration
    frame_rate_limiter: Option<Duration>,

    /// Initialization state machine
    init_state: InitState,
    init_instant: Option<Instant>,
    init_rx: Option<Receiver<InitEvent>>,

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

        let keyboard_enabled = config.general.keyboard_enabled;
        let mouse_enabled = config.general.mouse_enabled;
        let keyboard_custom_mapping_enabled = config.mappings.initial_state;

        let indicator_position = config.general.indicator_position;
        let max_fps = config.scrcpy.video.max_fps;
        let vsync = config.gpu.vsync;

        Self {
            toolbar: Toolbar::new(),
            indicator: Indicator::new(indicator_position, max_fps as f32),
            player: StreamPlayer::new(cc),

            config_manager,

            mouse_mapper: None,

            keyboard_mapper: None,

            device_monitor_rx: None,

            last_paint_instant: None,

            device_id: None,

            device_physical_size: (0, 0),

            device_orientation: 0,

            window_initialized: false,

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

        let (tx, rx) = bounded::<InitEvent>(INIT_RESULT_CHANNEL_CAPACITY);
        self.init_rx = Some(rx);

        start_initialization(&self.config_manager, tx);
    }

    /// Check initialization progress and update state
    fn check_init_stage(&mut self, _ctx: &egui::Context) {
        if let Some(rx) = &self.init_rx {
            while let Ok(result) = rx.try_recv() {
                #[allow(deprecated)] // Handle legacy InitEvent::Scrcpy during transition
                match result {
                    InitEvent::Scrcpy(_) => {
                        // Scrcpy no longer needed (using internal implementation)
                        debug!("Ignoring legacy Scrcpy init event");
                    }
                    InitEvent::KeyboardMapper(keyboard_mapper) => {
                        self.keyboard_mapper = keyboard_mapper;
                    }
                    InitEvent::MouseMapper(mouse_mapper) => {
                        self.mouse_mapper = mouse_mapper;
                    }
                    InitEvent::DeviceMonitor(_device_monitor_rx) => {
                        self.device_monitor_rx = Some(_device_monitor_rx);
                    }
                    InitEvent::DeviceId(device_id) => {
                        self.device_id = Some(device_id.clone());
                        // Start streaming when device ID is available
                        let config = self.config();
                        self.player.start(device_id, (*config.scrcpy).clone());
                    }
                    InitEvent::PhysicalSize(size) => self.device_physical_size = size,
                    InitEvent::Error(e) => {
                        error!("Initialization error: {}", e);
                        self.init_state = InitState::Failed(e.to_string());
                        return;
                    }
                }
            }

            // Check if all components are initialized AND video stream is ready with valid
            // dimensions
            let video_rect = self.player.video_rect();
            let stream_ready = self.player.ready()
                && video_rect.width() > 0.0
                && video_rect.height() > 0.0
                && !video_rect.min.x.is_nan();

            if self.device_monitor_rx.is_some() && self.device_id.is_some() && stream_ready {
                self.init_state = InitState::Ready;
                info!("Initialization completed successfully");
            }
        }
    }

    /// Rotate video by 90 degrees clockwise
    fn rotate(&mut self, ctx: &egui::Context) {
        let video_rotation = (self.player.rotation() + 1) % 4;

        // Sync rotation to player and indicator
        self.player.set_rotation(video_rotation);
        self.indicator.update_video_rotation(video_rotation);

        // Get effective dimensions after rotation
        let (w, h) = self.player.dimensions();

        // Resize window to match new video dimensions
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
            w as f32 + Toolbar::width(),
            h as f32,
        )));

        // Lock aspect ratio
        let aspect = (w as f32 + Toolbar::width()) / h as f32;
        ctx.send_viewport_cmd(egui::ViewportCommand::ResizeIncrements(Some(egui::vec2(
            aspect, 1.0,
        ))));

        // Update indicator resolution
        self.indicator.update_video_resolution((w, h));

        // Request repaint to apply changes immediately
        ctx.request_repaint();
    }

    /// Toggle mapping configuration window
    fn toggle_mapping_config(&mut self, _ctx: &egui::Context) {
        self.mapping_config_window.toggle();
    }

    // Check if position is within video rectangle
    fn is_in_video_rect(&self, pos: &egui::Pos2) -> bool {
        let video_rect = self.player.video_rect();
        pos.x >= video_rect.left()
            && pos.x <= video_rect.right()
            && pos.y >= video_rect.top()
            && pos.y <= video_rect.bottom()
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
            self.indicator
                .update_device_orientation(self.device_orientation);
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
    ) -> anyhow::Result<bool> {
        if !pressed {
            return Ok(false);
        }

        debug!(
            "Processing keyboard event: key={:?}, modifiers={:?}",
            key, modifiers
        );

        // Handle custom keymapping first, if enabled and IME is off
        if self.keyboard_custom_mapping_enabled
            && !self.device_ime_state
            && keyboard_mapper.handle_custom_keymapping_event(key)?
        {
            return Ok(true);
        }

        // Handle shift-only key event
        if modifiers.shift_only() && keyboard_mapper.handle_shifted_key_event(key)? {
            return Ok(true);
        }

        // Handle other key combo events
        if modifiers.any() && keyboard_mapper.handle_keycombo_event(modifiers, key)? {
            return Ok(true);
        }

        // Handle standard key event
        keyboard_mapper.handle_standard_key_event(key)
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
            // Flag to ignore text events if egui::Event::Key was processed
            let mut ignore_text_events = false;

            for event in &input.events {
                // Process keyboard events
                if self.keyboard_enabled
                    && let Some(ref keyboard_mapper) = self.keyboard_mapper
                {
                    if let egui::Event::Key {
                        key,
                        pressed,
                        modifiers,
                        ..
                    } = event
                    {
                        match self.process_keyboard_event(
                            keyboard_mapper,
                            key,
                            *pressed,
                            *modifiers,
                        ) {
                            Ok(handled) => {
                                if handled {
                                    ignore_text_events = true;
                                }
                            }
                            Err(e) => {
                                info!("Failed to handle keyboard event: {}", e);
                            }
                        }
                    } else if !ignore_text_events
                        && let egui::Event::Text(text) = event
                        && !text.is_empty()
                        && let Err(e) = keyboard_mapper.handle_text_input_event(text)
                    {
                        info!("Failed to handle text input event: {}", e);
                    };
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

    fn refresh_mapping_profiles(&mut self) {
        let (keyboard_mapper, device_id) =
            match (self.keyboard_mapper.as_mut(), self.device_id.as_ref()) {
                (Some(km), Some(did)) => (km, did),
                _ => {
                    debug!("Keyboard mapper or device ID not available for profile refresh");
                    self.indicator.reset_profiles();
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

                self.indicator.update_active_profile(active_profile_name);
            }
            Err(e) => {
                self.indicator.reset_profiles();
                debug!("Failed to refresh keyboard profiles: {}", e);
            }
        }
    }

    fn draw_toolbar(&mut self, ctx: &egui::Context) {
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
    }

    /// Draw indicator overlay on video
    fn draw_indicator(&mut self, ctx: &egui::Context) {
        let video_rect = self.player.video_rect();

        // Only draw indicator if video rect is valid (has positive dimensions)
        if video_rect.width() > 0.0 && video_rect.height() > 0.0 && !video_rect.min.x.is_nan() {
            egui::Area::new(egui::Id::new("indicator"))
                .fixed_pos(egui::pos2(0.0, 0.0))
                .interactable(false)
                .show(ctx, |ui| self.indicator.draw_indicator(ui, video_rect));
        }
    }

    fn coodinates_transform_params(&self) -> CoordinatesTransformParams {
        CoordinatesTransformParams {
            video_rect: self.player.video_rect(),
            video_rotation: 0, // Rotation handled by device orientation
            device_physical_size: self.device_physical_size,
            device_orientation: self.device_orientation,
            capture_orientation: 0, // No V4L2, always 0
        }
    }
}

impl eframe::App for SAideApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Draw base UI (toolbar) - always visible
        self.draw_toolbar(ctx);

        // Handle initialization state transitions
        match self.init_state {
            InitState::NotStarted => {
                // UI is ready, now start background initialization
                self.init();
            }
            InitState::InProgress => {
                // Update player to receive events
                self.player.update();

                // Check initialization progress
                self.check_init_stage(ctx);

                // Request repaint during initialization
                ctx.request_repaint();
            }
            InitState::Ready => {
                // Update player state
                self.player.update();

                // Update window size and aspect ratio on first frame
                if self.player.ready() && !self.window_initialized {
                    let (w, h) = self.player.video_dimensions();

                    if w > 0 && h > 0 {
                        // Resize window to match video
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                            w as f32 + Toolbar::width(),
                            h as f32,
                        )));

                        // Lock aspect ratio
                        let aspect = (w as f32 + Toolbar::width()) / h as f32;
                        ctx.send_viewport_cmd(egui::ViewportCommand::ResizeIncrements(Some(
                            egui::vec2(aspect, 1.0),
                        )));

                        self.indicator.update_video_resolution((w, h));
                        self.window_initialized = true;

                        info!("Window initialized to {}x{}", w, h);
                    }
                }

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
            InitState::Failed(ref _reason) => {
                // Player will show error state automatically
            }
        }

        // Render player in center panel
        egui::CentralPanel::default().show(ctx, |ui| {
            let _response = self.player.render(ui);
        });

        // Draw indicator overlay on top of video
        if self.init_state == InitState::Ready && self.config().general.indicator {
            self.indicator.update_video_stats(self.player.video_stats());
            self.draw_indicator(ctx);
        }

        // Frame rate limiting (only when streaming)
        if matches!(
            self.player.state(),
            super::stream_player::PlayerState::Streaming
        ) {
            if let Some(limiter) = self.frame_rate_limiter {
                if let Some(last) = self.last_paint_instant {
                    let elapsed = last.elapsed();
                    if elapsed < limiter {
                        thread::sleep(limiter - elapsed);
                    }
                }
                self.last_paint_instant = Some(Instant::now());
            }
            ctx.request_repaint();
        }
    }
}
