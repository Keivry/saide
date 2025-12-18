use {
    super::{
        super::{
            coords::{MappingCoordSys, MappingPos, ScrcpyCoordSys, VisualCoordSys, VisualPos},
            init::{
                DeviceMonitorEvent,
                INIT_RESULT_CHANNEL_CAPACITY,
                InitEvent,
                start_initialization,
            },
            utils::find_nearest_mapping,
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
            mapping::{Key, MappingAction, MouseButton, WheelDirection},
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

    /// ScrcpyConnection (kept alive to prevent server shutdown)
    connection: Option<crate::scrcpy::connection::ScrcpyConnection>,

    /// Control sender (for sending input commands to device)
    control_sender: Option<crate::controller::control_sender::ControlSender>,

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

    /// Device orientation (0-3), clockwise
    device_orientation: u32,

    /// Coordinate systems
    mapping_coords: MappingCoordSys,
    scrcpy_coords: ScrcpyCoordSys,
    visual_coords: VisualCoordSys,

    /// Audio disabled warning message (if audio was requested but unavailable)
    audio_warning: Option<String>,

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
    last_pointer_pos: VisualPos,

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

            connection: None,

            control_sender: None,

            mouse_mapper: None,

            keyboard_mapper: None,

            device_monitor_rx: None,

            last_paint_instant: None,

            device_id: None,

            device_orientation: 0,

            audio_warning: None,

            // Initialize coordinate systems with default values
            mapping_coords: MappingCoordSys::new(0),
            scrcpy_coords: ScrcpyCoordSys::new(1, 1, None),
            visual_coords: VisualCoordSys::new(0), // Only stores rotation, rect passed at runtime

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

            last_pointer_pos: VisualPos::ZERO,

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
        let mut capture_locked = None;

        if let Some(rx) = &self.init_rx {
            while let Ok(result) = rx.try_recv() {
                match result {
                    InitEvent::ConnectionReady {
                        connection,
                        control_sender,
                        video_stream,
                        audio_stream,
                        video_resolution,
                        device_name,
                        audio_disabled_reason,
                        capture_orientation_locked,
                    } => {
                        info!(
                            "ScrcpyConnection ready: {}x{}, device: {:?}, capture_locked: {}",
                            video_resolution.0,
                            video_resolution.1,
                            device_name,
                            capture_orientation_locked
                        );

                        // Store audio warning if present
                        self.audio_warning = audio_disabled_reason;

                        // Save connection (to keep it alive and prevent server shutdown)
                        self.connection = Some(connection);

                        // Save control sender
                        self.control_sender = Some(control_sender);

                        // Start player with streams
                        let config = self.config();
                        self.player.start_with_streams(
                            video_stream,
                            audio_stream,
                            video_resolution,
                            (*config.scrcpy).clone(),
                        );

                        // Save capture_locked for later use (after borrow ends)
                        capture_locked = Some(capture_orientation_locked);
                    }
                    InitEvent::KeyboardMapper(keyboard_mapper) => {
                        self.keyboard_mapper = Some(keyboard_mapper);
                    }
                    InitEvent::MouseMapper(mouse_mapper) => {
                        self.mouse_mapper = Some(mouse_mapper);
                    }
                    InitEvent::DeviceMonitor(_device_monitor_rx) => {
                        self.device_monitor_rx = Some(_device_monitor_rx);
                    }
                    InitEvent::DeviceId(device_id) => {
                        self.device_id = Some(device_id);
                    }
                    InitEvent::Error(e) => {
                        error!("Initialization error: {}", e);
                        self.init_state = InitState::Failed(e.to_string());
                        return;
                    }
                }
            }
        }

        // Initialize ScrcpyCoordSys after event processing (to avoid borrow conflicts)
        if let Some(locked) = capture_locked {
            self.update_scrcpy_coords(locked);
        }

        if let Some(_rx) = &self.init_rx {
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

                // Initialize coordinate systems now that video is ready
                self.update_mapping_coords(); // Device orientation known
                self.update_visual_coords(); // Video rotation available
                // ScrcpyCoordSys already initialized above

                // Apply turn_screen_off setting if enabled
                let config = self.config();
                if config.scrcpy.options.turn_screen_off
                    && let Some(sender) = &self.control_sender
                {
                    if let Err(e) = sender.send_screen_off_with_brightness_save() {
                        error!("Failed to turn screen off on init: {}", e);
                    } else {
                        info!("Screen turned off as per config");
                    }
                }
            }
        }
    }

    /// Resize the application window to match video dimensions
    fn resize(&mut self, ctx: &egui::Context) {
        let (w, h) = self.player.dimensions();
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
            w as f32 + Toolbar::width(),
            h as f32,
        )));
    }

    /// Update MappingCoordSys when device orientation changes
    fn update_mapping_coords(&mut self) {
        self.mapping_coords = MappingCoordSys::new(self.device_orientation);
        debug!(
            "MappingCoordSys updated: device_orientation={}",
            self.device_orientation
        );
    }

    /// Check if capture orientation is locked (NVDEC mode)
    fn is_capture_locked(&self) -> bool { self.scrcpy_coords.capture_orientation.is_some() }

    /// Update ScrcpyCoordSys when video resolution or capture orientation changes
    fn update_scrcpy_coords(&mut self, capture_orientation_locked: bool) {
        let video_resolution = self.player.video_resolution();
        let capture_orientation = if capture_orientation_locked {
            Some(0) // Locked to portrait
        } else {
            None
        };
        self.scrcpy_coords = ScrcpyCoordSys::new(
            video_resolution.0 as u16,
            video_resolution.1 as u16,
            capture_orientation,
        );
        debug!(
            "ScrcpyCoordSys updated: video_resolution={}x{}, capture_orientation={:?}",
            video_resolution.0, video_resolution.1, capture_orientation
        );
    }

    /// Update VisualCoordSys when user rotation changes
    fn update_visual_coords(&mut self) {
        let video_rotation = self.player.rotation();
        self.visual_coords = VisualCoordSys::new(video_rotation);
        debug!("VisualCoordSys updated: rotation={}", video_rotation);
    }

    /// Rotate video by 90 degrees clockwise
    fn rotate(&mut self, ctx: &egui::Context) {
        let video_rotation = (self.player.rotation() + 1) % 4;

        // Sync rotation to player and indicator
        self.player.set_rotation(video_rotation);
        self.indicator.update_video_rotation(video_rotation);

        // Resize window to match new video dimensions
        // No needed to update VisualCoordSys here, resize() will call it
        self.resize(ctx);

        // Update indicator resolution
        let (w, h) = self.player.dimensions();
        self.indicator.update_video_resolution((w, h));

        // Request repaint to apply changes immediately
        ctx.request_repaint();

        // Update VisualCoordSys (user rotation changed)
        self.update_visual_coords();
    }

    /// Toggle mapping configuration window
    fn toggle_mapping_config(&mut self, _ctx: &egui::Context) {
        self.mapping_config_window.toggle();
    }

    // Check if position is within video rectangle
    fn is_in_video_rect(&self, pos: &VisualPos) -> bool {
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
            // Update MappingCoordSys (device orientation changed)
            self.update_mapping_coords();
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
        let video_rect = self.player.video_rect();
        let event = self.mapping_config_window.draw(
            ctx,
            &mappings,
            video_rect,
            &self.visual_coords,
            &self.scrcpy_coords,
            &self.mapping_coords,
        );

        match event {
            MappingConfigEvent::Close => {
                self.mapping_config_window.hide();
            }
            MappingConfigEvent::RequestAddMapping(screen_pos) => {
                // Convert screen position to mapping percentage coordinates (0.0-1.0)
                // Visual -> Scrcpy -> Mapping
                let video_rect = self.player.video_rect();
                if let Some(percent_pos) = self.visual_coords.to_mapping(
                    &screen_pos,
                    &video_rect,
                    &self.scrcpy_coords,
                    &self.mapping_coords,
                ) {
                    info!(
                        "Add mapping: screen=({:.1},{:.1}) -> percent=({:.6},{:.6}) [device_orientation={}]",
                        screen_pos.x,
                        screen_pos.y,
                        percent_pos.x,
                        percent_pos.y,
                        self.device_orientation
                    );

                    self.mapping_config_window
                        .request_input_dialog(&percent_pos);
                }
            }
            MappingConfigEvent::RequestDeleteMapping(screen_pos) => {
                // Find nearest mapping to delete
                // Visual -> Scrcpy -> Mapping
                let video_rect = self.player.video_rect();
                if let Some(percent_pos) = self.visual_coords.to_mapping(
                    &screen_pos,
                    &video_rect,
                    &self.scrcpy_coords,
                    &self.mapping_coords,
                ) && let Some((nearest_key, nearest_pos)) =
                    find_nearest_mapping(&percent_pos, &mappings)
                {
                    info!(
                        "Delete mapping: {:?} at ({:.6}, {:.6})",
                        nearest_key, nearest_pos.x, nearest_pos.y
                    );
                    self.mapping_config_window
                        .request_delete_dialog(nearest_key, &nearest_pos);
                }
            }
            MappingConfigEvent::None => {}
        }

        // Handle dialogs
        if self.mapping_config_window.is_input_dialog_open()
            && let Some(pending_pos) = self.mapping_config_window.get_pos()
            && let Some(key) = self
                .mapping_config_window
                .show_key_input_dialog(ctx, &pending_pos)
        {
            if let Some(action) = self.get_mapping(&key)
                && let MappingAction::Tap { pos } = action
            {
                self.mapping_config_window
                    .request_override_dialog(key, &pos, &pending_pos);
            } else {
                self.add_mapping(key, &pending_pos);
            }
        }
        if self.mapping_config_window.is_delete_dialog_open()
            && let Some((key, pos)) = self.mapping_config_window.get_delete_target()
            && let Some(confirmed) = self
                .mapping_config_window
                .show_delete_confirm_dialog(ctx, key, &pos)
            && confirmed
        {
            self.delete_mapping(key);
        }
        if self.mapping_config_window.is_override_dialog_open()
            && let Some((key, pos, new_pos)) = self.mapping_config_window.get_override_target()
            && let Some(confirmed) = self
                .mapping_config_window
                .show_override_confirm_dialog(ctx, key, &pos, &new_pos)
            && confirmed
        {
            self.add_mapping(key, &new_pos);
        }
    }

    /// Add a new mapping (expects percentage coordinates 0.0-1.0)
    fn add_mapping(&mut self, key: Key, pos: &MappingPos) {
        info!("Adding mapping: {:?} -> ({:.4}, {:.4})", key, pos.x, pos.y);

        let Some(keyboard_mapper) = &self.keyboard_mapper else {
            error!("Keyboard mapper not initialized");
            return;
        };

        if let Some(profile) = keyboard_mapper.get_active_profile() {
            // Create new action with percentage coordinates
            let action = MappingAction::Tap { pos: *pos };

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

    fn get_mapping(&self, key: &Key) -> Option<MappingAction> {
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
        pos: &VisualPos,
    ) {
        if !self.is_in_video_rect(pos) {
            return;
        }

        trace!("Processing mouse button event: {:?} at {:?}", button, pos);

        // Use video coordinates for scrcpy control channel
        let video_rect = self.player.video_rect();
        let Some(scrcpy_pos) = self
            .visual_coords
            .to_scrcpy(pos, &video_rect, &self.scrcpy_coords)
        else {
            debug!("Failed to convert screen coords to video coords");
            return;
        };

        debug!(
            "Converted screen ({:.1}, {:.1}) -> scrcpy video ({}, {})",
            pos.x, pos.y, scrcpy_pos.x, scrcpy_pos.y
        );

        // Update ControlSender screen size
        if let Some(sender) = &self.control_sender {
            sender.update_screen_size(
                self.scrcpy_coords.video_width,
                self.scrcpy_coords.video_height,
            );
        }

        let button = MouseButton::from(button);
        if let Err(e) =
            mouse_mapper.handle_button_event(button, pressed, scrcpy_pos.x, scrcpy_pos.y)
        {
            error!("Failed to handle mouse button event: {}", e);
        }
    }

    /// Process mouse move event
    fn process_mouse_move_event(
        &self,
        mouse_mapper: &MouseMapper,
        pos: &VisualPos,
        last_pointer_pos: &VisualPos,
    ) -> Option<VisualPos> {
        if self.is_in_video_rect(pos) {
            trace!("PointerMoved inside video rect at {:?}", pos);

            let video_rect = self.player.video_rect();
            if let Some(scrcpy_pos) =
                self.visual_coords
                    .to_scrcpy(pos, &video_rect, &self.scrcpy_coords)
            {
                // Update ControlSender screen size
                if let Some(sender) = &self.control_sender {
                    sender.update_screen_size(
                        self.scrcpy_coords.video_width,
                        self.scrcpy_coords.video_height,
                    );
                }

                if let Err(e) = mouse_mapper.handle_move_event(scrcpy_pos.x, scrcpy_pos.y) {
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
            let video_rect = self.player.video_rect();
            if mouse_mapper.get_button_state() != MouseState::Idle
                && let Some(scrcpy_pos) =
                    self.visual_coords
                        .to_scrcpy(last_pointer_pos, &video_rect, &self.scrcpy_coords)
            {
                // Update ControlSender screen size
                if let Some(sender) = &self.control_sender {
                    sender.update_screen_size(
                        self.scrcpy_coords.video_width,
                        self.scrcpy_coords.video_height,
                    );
                }

                if let Err(e) = mouse_mapper.handle_button_event(
                    MouseButton::Left,
                    false,
                    scrcpy_pos.x,
                    scrcpy_pos.y,
                ) {
                    error!("Failed to handle mouse button release event: {}", e);
                }
            }

            None
        }
    }

    /// Process mouse wheel event
    fn process_mouse_wheel_event(
        &self,
        mouse_mapper: &MouseMapper,
        delta: &egui::Vec2,
        pointer_pos: &VisualPos,
    ) {
        if !self.is_in_video_rect(pointer_pos) {
            return;
        }

        debug!(
            "Processing mouse wheel event: {:?} at {:?}",
            delta, pointer_pos
        );

        let video_rect = self.player.video_rect();
        let Some(scrcpy_pos) =
            self.visual_coords
                .to_scrcpy(pointer_pos, &video_rect, &self.scrcpy_coords)
        else {
            return;
        };

        // Update ControlSender screen size
        if let Some(sender) = &self.control_sender {
            sender.update_screen_size(
                self.scrcpy_coords.video_width,
                self.scrcpy_coords.video_height,
            );
        }

        let dir = if delta.y < 0.0 {
            WheelDirection::Up
        } else {
            WheelDirection::Down
        };

        if let Err(e) = mouse_mapper.handle_wheel_event(scrcpy_pos.x, scrcpy_pos.y, &dir) {
            error!("Failed to handle wheel event: {}", e);
        } else {
            debug!(
                "Mouse wheel event at scrcpy video coords: ({}, {})",
                scrcpy_pos.x, scrcpy_pos.y
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
                        self.process_mouse_wheel_event(mouse_mapper, delta, &pointer_pos);
                    }
                    _ => {}
                }
            }
        });
    }

    fn refresh_mapping_profiles(&mut self) {
        let capture_locked = self.is_capture_locked();
        let device_orientation = self.device_orientation;

        let (keyboard_mapper, device_id) =
            match (self.keyboard_mapper.as_mut(), self.device_id.as_ref()) {
                (Some(km), Some(did)) => (km, did),
                _ => {
                    debug!("Keyboard mapper or device ID not available for profile refresh");
                    self.indicator.reset_profiles();
                    return;
                }
            };

        match keyboard_mapper.refresh_profiles(device_id, device_orientation, capture_locked) {
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
                ToolbarEvent::ToggleScreenPower => {
                    debug!("Turning off screen from toolbar");
                    if let Some(sender) = &self.control_sender {
                        // Only turn OFF screen, wake up with physical power button
                        if let Err(e) = sender.send_set_display_power(false) {
                            error!("Failed to turn off screen: {}", e);
                        } else {
                            info!("Screen OFF (press physical power button to wake up)");
                        }
                    }
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
}

impl Drop for SAideApp {
    fn drop(&mut self) {
        debug!("SAideApp dropping, cleaning up connection");

        // Explicitly shutdown connection to ensure server process is killed
        if let Some(mut conn) = self.connection.take()
            && let Err(e) = conn.shutdown()
        {
            debug!("Failed to shutdown connection: {}", e);
        }

        debug!("SAideApp cleanup completed");
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
                // Store dimensions before update
                let old_dimensions = if self.window_initialized {
                    self.player.video_dimensions()
                } else {
                    (0, 0)
                };

                // Update player state
                self.player.update();

                // Get new dimensions after update
                let new_dimensions = self.player.video_dimensions();

                // Check if dimensions changed (device rotation or first frame)
                if new_dimensions != old_dimensions && new_dimensions.0 > 0 {
                    info!(
                        "Video dimensions changed: {:?} -> {:?}",
                        old_dimensions, new_dimensions
                    );

                    // Resize window to match new video dimensions
                    self.resize(ctx);

                    // Update indicator
                    self.indicator.update_video_resolution(new_dimensions);
                    self.window_initialized = true;

                    // Update ScrcpyCoordSys (video resolution changed)
                    let capture_locked = self.is_capture_locked();
                    self.update_scrcpy_coords(capture_locked);
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

        // Render player in center panel (no margin to maximize video area)
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                let _response = self.player.render(ui);

                // Error overlay if stream failed
                if let crate::app::ui::stream_player::PlayerState::Failed(err) = self.player.state()
                {
                    egui::Area::new(egui::Id::new("error_overlay"))
                        .fixed_pos(egui::pos2(0.0, 0.0))
                        .show(ctx, |ui| {
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
                                        egui::RichText::new("⚠️ Connection Lost")
                                            .size(36.0)
                                            .color(egui::Color32::from_rgb(255, 100, 100)),
                                    );

                                    ui.add_space(20.0);

                                    let msg = if err.contains("read") || err.contains("timeout") {
                                        "USB disconnected or device offline"
                                    } else {
                                        "Stream error occurred"
                                    };

                                    ui.label(
                                        egui::RichText::new(msg)
                                            .size(20.0)
                                            .color(egui::Color32::WHITE),
                                    );

                                    ui.add_space(15.0);

                                    ui.label(
                                        egui::RichText::new("Please restart the application")
                                            .size(16.0)
                                            .color(egui::Color32::GRAY),
                                    );
                                });
                            }
                        });
                }
            });

        // Draw indicator overlay on top of video
        if self.init_state == InitState::Ready && self.config().general.indicator {
            self.indicator.update_video_stats(self.player.video_stats());
            self.draw_indicator(ctx);
        }

        // Show audio warning if present (overlay at top)
        if let Some(warning) = self.audio_warning.clone() {
            let mut close_clicked = false;
            egui::Area::new(egui::Id::new("audio_warning"))
                .fixed_pos(egui::pos2(10.0, 10.0))
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgba_unmultiplied(40, 40, 40, 220))
                        .corner_radius(5.0)
                        .inner_margin(10.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("⚠")
                                        .color(egui::Color32::YELLOW)
                                        .size(20.0),
                                );
                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new("Audio Not Available")
                                            .color(egui::Color32::YELLOW)
                                            .strong(),
                                    );
                                    ui.label(
                                        egui::RichText::new(&warning)
                                            .color(egui::Color32::LIGHT_GRAY),
                                    );
                                });
                                if ui.button("✖").clicked() {
                                    close_clicked = true;
                                }
                            });
                        });
                });
            if close_clicked {
                self.audio_warning = None;
            }
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
