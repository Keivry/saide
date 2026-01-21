//! Main SAide application UI state and logic

use {
    super::{
        super::{
            coords::{MappingPos, VisualPos},
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
        player::{PlayerState, StreamPlayer},
        state::{AppState, ConfigState, UIState},
        toolbar::{Toolbar, ToolbarEvent},
    },
    crate::{
        config::mapping::{Key, MappingAction, MouseButton, WheelDirection},
        controller::mouse::MouseState,
        error::Result,
        t,
    },
    crossbeam_channel::{Receiver, bounded},
    eframe::egui::{self, Color32},
    std::{thread, time::Instant},
    tracing::{debug, error, info, trace, warn},
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

pub struct SAideApp {
    shutdown_rx: Receiver<()>,
    shutdown_requested: bool,

    toolbar: Toolbar,
    indicator: Indicator,
    player: StreamPlayer,
    mapping_config_window: MappingConfigWindow,

    app_state: AppState,
    config_state: ConfigState,
    ui_state: UIState,

    init_state: InitState,
    init_instant: Option<Instant>,
    init_rx: Option<Receiver<InitEvent>>,

    last_paint_instant: Option<Instant>,
}

impl SAideApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        serial: &str,
        config_manager: crate::config::ConfigManager,
        shutdown_rx: Receiver<()>,
    ) -> Self {
        let config = config_manager.config();
        let keyboard_custom_mapping_enabled = config.mappings.initial_state;
        let indicator_position = config.general.indicator_position;
        let max_fps = config.scrcpy.video.max_fps;
        let audio_buffer_frames = config.scrcpy.audio.buffer_frames;
        let audio_ring_capacity = config.scrcpy.audio.ring_capacity;

        let cancel_token = tokio_util::sync::CancellationToken::new();

        let mut toolbar = Toolbar::new();
        toolbar.set_keyboard_mapping_enabled(keyboard_custom_mapping_enabled);

        Self {
            shutdown_rx,
            shutdown_requested: false,
            toolbar,
            indicator: Indicator::new(indicator_position, max_fps as f32),
            player: StreamPlayer::new(
                cc,
                cancel_token.clone(),
                audio_buffer_frames,
                audio_ring_capacity,
            ),
            mapping_config_window: MappingConfigWindow::new(),
            app_state: AppState::new(serial.to_owned(), cancel_token),
            config_state: ConfigState::new(config_manager),
            ui_state: UIState::new(),
            init_state: InitState::NotStarted,
            init_instant: None,
            init_rx: None,
            last_paint_instant: None,
        }
    }

    pub fn config(&self) -> std::sync::Arc<crate::config::SAideConfig> {
        self.config_state.config()
    }

    fn init(&mut self) {
        self.init_state = InitState::InProgress;
        self.init_instant = Some(Instant::now());

        let (tx, rx) = bounded::<InitEvent>(INIT_RESULT_CHANNEL_CAPACITY);
        self.init_rx = Some(rx);

        start_initialization(
            self.app_state.device_serial(),
            self.config_state.config(),
            tx,
            self.app_state.cancel_token.clone(),
        );
    }

    /// Check initialization progress and update state
    fn check_init_stage(&mut self, _ctx: &egui::Context) {
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
                        capture_orientation: corientation,
                    } => {
                        info!(
                            "ScrcpyConnection ready: {}x{}, device: {} ({:?}), capture_orientation: {:?}",
                            video_resolution.0,
                            video_resolution.1,
                            self.app_state.device_serial(),
                            device_name,
                            corientation
                        );

                        // Store audio warning if present
                        self.ui_state.audio_warning = audio_disabled_reason;

                        // Save connection (to keep it alive and prevent server shutdown)
                        self.app_state.connection = Some(connection);

                        // Save control sender
                        self.app_state.control_sender = Some(control_sender);

                        // Start player with streams
                        self.player.start(
                            video_stream,
                            audio_stream,
                            video_resolution,
                            self.app_state.device_serial(),
                        );

                        // Initialize capture orientation for ScrcpyCoordSys
                        self.app_state
                            .scrcpy_coords_mut()
                            .update_capture_orientation(corientation);
                    }
                    InitEvent::KeyboardMapper(keyboard_mapper) => {
                        self.app_state.keyboard_mapper = Some(keyboard_mapper);
                    }
                    InitEvent::MouseMapper(mouse_mapper) => {
                        self.app_state.mouse_mapper = Some(mouse_mapper);
                    }
                    InitEvent::DeviceMonitor(_device_monitor_rx) => {
                        self.app_state.device_monitor_rx = Some(_device_monitor_rx);
                    }
                    InitEvent::Error(e) => {
                        error!("Initialization error: {}", e);
                        self.init_state = InitState::Failed(e.to_string());
                        return;
                    }
                }
            }
        }

        if let Some(_rx) = &self.init_rx {
            // Check if all components are initialized AND video stream is ready with valid
            // dimensions
            let video_rect = self.player.video_rect();
            let stream_ready = self.player.ready()
                && video_rect.width() > 0.0
                && video_rect.height() > 0.0
                && !video_rect.min.x.is_nan();

            if self.app_state.device_monitor_rx.is_some() && stream_ready {
                self.init_state = InitState::Ready;
                info!("Initialization completed successfully");

                // Initialize coordinate systems now that video is ready
                self.ui_state
                    .mapping_coords_mut()
                    .update_device_orientation(self.app_state.device_orientation());
                debug!(
                    "Initial device orientation: {}",
                    self.app_state.device_orientation()
                );
                self.ui_state
                    .visual_coords_mut()
                    .update_rotation(self.player.rotation());
                debug!("Initial video rotation: {}", self.player.rotation());

                // Apply turn_screen_off setting if enabled
                let config = self.config();
                if config.scrcpy.options.turn_screen_off
                    && let Some(sender) = &self.app_state.control_sender
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
        let (w, h) = self.player.video_dimensions();
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
            w as f32 + Toolbar::width(),
            h as f32,
        )));
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
        let (w, h) = self.player.video_dimensions();
        self.indicator.update_video_resolution((w, h));

        // Request repaint to apply changes immediately
        ctx.request_repaint();

        // Update VisualCoordSys (user rotation changed)
        self.ui_state
            .visual_coords_mut()
            .update_rotation(video_rotation);

        debug!("Video rotated to {}", video_rotation);
    }

    /// Toggle mapping configuration window
    fn toggle_mapping_config(&mut self, _ctx: &egui::Context) {
        self.mapping_config_window.toggle();
    }

    /// Toggle keyboard custom mapping
    fn toggle_keyboard_mapping(&mut self) {
        self.config_state.toggle_keyboard_custom_mapping();

        // Update toolbar button state
        self.toolbar
            .set_keyboard_mapping_enabled(self.config_state.keyboard_custom_mapping_enabled());

        info!(
            "Keyboard custom mapping toggled: {}",
            self.config_state.keyboard_custom_mapping_enabled()
        );
    }

    /// Toggle mapping visualization
    fn toggle_mapping_visualization(&mut self) {
        self.ui_state.toggle_mapping_visualization();

        // Update toolbar button state
        self.toolbar
            .set_mapping_visualization_enabled(self.ui_state.mapping_visualization_enabled());

        info!(
            "Mapping visualization toggled: {}",
            self.ui_state.mapping_visualization_enabled()
        );
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

        if let Some(rx) = &self.app_state.device_monitor_rx {
            while let Ok(event) = rx.try_recv() {
                match event {
                    DeviceMonitorEvent::Rotated(new_orientation) => {
                        debug!("Device rotated to orientation: {}", new_orientation * 90);
                        self.app_state.device_orientation = new_orientation % 4;
                        rotated = true;
                    }
                    DeviceMonitorEvent::ImStateChanged(im_state) => {
                        if im_state != self.config_state.device_ime_state() {
                            debug!("Device IME state changed: {}", im_state);
                            self.config_state.device_ime_state = im_state;
                        }
                    }
                    DeviceMonitorEvent::DeviceOffline => {
                        warn!("Device went offline - USB/ADB connection lost");

                        // Request application shutdown
                        self.shutdown_requested = true;
                    }
                }
            }
        }

        // Refresh keyboard profiles if needed
        if rotated {
            self.refresh_mapping_profiles();

            self.indicator
                .update_device_orientation(self.app_state.device_orientation());

            // Update MappingCoordSys (device orientation changed)
            self.ui_state
                .mapping_coords_mut()
                .update_device_orientation(self.app_state.device_orientation());
            debug!(
                "Updated device orientation to {}",
                self.app_state.device_orientation()
            );
        }
    }

    /// Process mapping configuration window events
    fn process_mapping_config_events(&mut self, ctx: &egui::Context) {
        if self.init_state != InitState::Ready || !self.mapping_config_window.is_visible() {
            return;
        }

        let Some(keyboard_mapper) = &self.app_state.keyboard_mapper else {
            warn!("Keyboard mapper not available, skipping mapping config events");
            return;
        };

        // Get current mappings for display
        if let Some(mappings) = keyboard_mapper.get_active_mappings() {
            // Draw the config window and handle events
            let video_rect = self.player.video_rect();
            let event = self.mapping_config_window.draw(
                ctx,
                &mappings,
                video_rect,
                self.ui_state.visual_coords(),
                self.app_state.scrcpy_coords(),
                self.ui_state.mapping_coords(),
            );

            match event {
                MappingConfigEvent::Close => {
                    self.mapping_config_window.hide();
                }
                MappingConfigEvent::RequestAddMapping(screen_pos) => {
                    // Convert screen position to mapping percentage coordinates (0.0-1.0)
                    // Visual -> Scrcpy -> Mapping
                    let video_rect = self.player.video_rect();
                    if let Some(percent_pos) = self.ui_state.visual_coords().to_mapping(
                        &screen_pos,
                        &video_rect,
                        self.app_state.scrcpy_coords(),
                        self.ui_state.mapping_coords(),
                    ) {
                        info!(
                            "Add mapping: screen=({:.1},{:.1}) -> percent=({:.6},{:.6}) [device_orientation={}]",
                            screen_pos.x,
                            screen_pos.y,
                            percent_pos.x,
                            percent_pos.y,
                            self.app_state.device_orientation()
                        );

                        self.mapping_config_window
                            .request_input_dialog(&percent_pos);
                    }
                }
                MappingConfigEvent::RequestDeleteMapping(screen_pos) => {
                    // Find nearest mapping to delete
                    // Visual -> Scrcpy -> Mapping
                    let video_rect = self.player.video_rect();
                    if let Some(percent_pos) = self.ui_state.visual_coords().to_mapping(
                        &screen_pos,
                        &video_rect,
                        self.app_state.scrcpy_coords(),
                        self.ui_state.mapping_coords(),
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

        let Some(keyboard_mapper) = &self.app_state.keyboard_mapper else {
            error!("Keyboard mapper not initialized");
            return;
        };

        let action = MappingAction::Tap { pos: *pos };
        keyboard_mapper.add_mapping(key, action);

        // Save to config file
        if let Err(e) = self.config_state.config_manager.save() {
            error!("Failed to save config: {}", e);
        } else {
            info!("Mapping saved successfully");
        }
    }

    /// Delete a mapping
    fn delete_mapping(&mut self, key: Key) {
        info!("Deleting mapping: {:?}", key);

        let Some(keyboard_mapper) = &self.app_state.keyboard_mapper else {
            error!("Keyboard mapper not initialized");
            return;
        };

        keyboard_mapper.delete_mapping(&key);

        // Save to config file
        if let Err(e) = self.config_state.config_manager.save() {
            error!("Failed to save config: {}", e);
        } else {
            info!("Mapping deleted successfully");
        }
    }

    fn get_mapping(&self, key: &Key) -> Option<MappingAction> {
        let Some(keyboard_mapper) = &self.app_state.keyboard_mapper else {
            return None;
        };

        keyboard_mapper.get_mapping(key)
    }

    /// Process keyboard event
    fn process_keyboard_event(
        &mut self,
        key: &egui::Key,
        pressed: bool,
        modifiers: egui::Modifiers,
    ) -> Result<bool> {
        if !pressed {
            return Ok(false);
        }

        debug!(
            "Processing keyboard event: key={:?}, modifiers={:?}",
            key, modifiers
        );

        if key == &self.config().mappings.toggle {
            self.toggle_keyboard_mapping();
            return Ok(true);
        }

        let Some(keyboard_mapper) = &self.app_state.keyboard_mapper else {
            debug!("Keyboard mapper not available, ignoring key event");
            return Ok(false);
        };

        // Handle custom keymapping first, if enabled and IME is off
        if self.config_state.keyboard_custom_mapping_enabled()
            && !self.config_state.device_ime_state()
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
        let Some(scrcpy_pos) =
            self.ui_state
                .visual_coords
                .to_scrcpy(pos, &video_rect, self.app_state.scrcpy_coords())
        else {
            debug!("Failed to convert screen coords to video coords");
            return;
        };

        debug!(
            "Converted screen ({:.1}, {:.1}) -> scrcpy video ({}, {})",
            pos.x, pos.y, scrcpy_pos.x, scrcpy_pos.y
        );

        // Update ControlSender screen size
        if let Some(sender) = &self.app_state.control_sender {
            sender.update_screen_size(
                self.app_state.scrcpy_coords().video_width,
                self.app_state.scrcpy_coords().video_height,
            );
        }

        let button = MouseButton::from(button);

        let Some(mouse_mapper) = &self.app_state.mouse_mapper else {
            debug!("Mouse mapper not available, ignoring button event");
            return;
        };

        if let Err(e) =
            mouse_mapper.handle_button_event(button, pressed, scrcpy_pos.x, scrcpy_pos.y)
        {
            error!("Failed to handle mouse button event: {}", e);
        }
    }

    /// Process mouse move event
    fn process_mouse_move_event(
        &self,
        pos: &VisualPos,
        last_pointer_pos: &VisualPos,
    ) -> Option<VisualPos> {
        let Some(mouse_mapper) = &self.app_state.mouse_mapper else {
            return None;
        };

        if self.is_in_video_rect(pos) {
            trace!("PointerMoved inside video rect at {:?}", pos);

            let video_rect = self.player.video_rect();
            if let Some(scrcpy_pos) = self.ui_state.visual_coords().to_scrcpy(
                pos,
                &video_rect,
                self.app_state.scrcpy_coords(),
            ) {
                // Update ControlSender screen size
                if let Some(sender) = &self.app_state.control_sender {
                    sender.update_screen_size(
                        self.app_state.scrcpy_coords().video_width,
                        self.app_state.scrcpy_coords().video_height,
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
                && let Some(scrcpy_pos) = self.ui_state.visual_coords().to_scrcpy(
                    last_pointer_pos,
                    &video_rect,
                    self.app_state.scrcpy_coords(),
                )
            {
                // Update ControlSender screen size
                if let Some(sender) = &self.app_state.control_sender {
                    sender.update_screen_size(
                        self.app_state.scrcpy_coords().video_width,
                        self.app_state.scrcpy_coords().video_height,
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
    fn process_mouse_wheel_event(&self, delta: &egui::Vec2, pointer_pos: &VisualPos) {
        if !self.is_in_video_rect(pointer_pos) {
            return;
        }

        debug!(
            "Processing mouse wheel event: {:?} at {:?}",
            delta, pointer_pos
        );

        let video_rect = self.player.video_rect();
        let Some(scrcpy_pos) = self.ui_state.visual_coords().to_scrcpy(
            pointer_pos,
            &video_rect,
            self.app_state.scrcpy_coords(),
        ) else {
            return;
        };

        // Update ControlSender screen size
        if let Some(sender) = &self.app_state.control_sender {
            sender.update_screen_size(
                self.app_state.scrcpy_coords().video_width,
                self.app_state.scrcpy_coords().video_height,
            );
        }

        let dir = if delta.y < 0.0 {
            WheelDirection::Up
        } else {
            WheelDirection::Down
        };

        let Some(mouse_mapper) = &self.app_state.mouse_mapper else {
            debug!("Mouse mapper not available, ignoring wheel event");
            return;
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
        if self.config_state.mouse_enabled()
            && let Err(e) = self.app_state.mouse_mapper.as_ref().unwrap().update()
        {
            error!("Failed to update mouse mapper: {}", e);
        }

        ctx.input(|input| {
            // Flag to ignore text events if egui::Event::Key was processed
            let mut ignore_text_events = false;

            for event in &input.events {
                // Process keyboard events
                if self.config_state.keyboard_enabled() {
                    if let egui::Event::Key {
                        key,
                        pressed,
                        modifiers,
                        ..
                    } = event
                    {
                        match self.process_keyboard_event(key, *pressed, *modifiers) {
                            Ok(handled) => {
                                // If key event was handled, ignore subsequent text events
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
                        && let Err(e) = self
                            .app_state
                            .keyboard_mapper
                            .as_ref()
                            .unwrap()
                            .handle_text_input_event(text)
                    {
                        info!("Failed to handle text input event: {}", e);
                    };
                }

                // Process mouse events
                if !self.config_state.mouse_enabled() {
                    continue;
                }

                match event {
                    egui::Event::PointerButton {
                        button,
                        pressed,
                        pos,
                        ..
                    } => {
                        self.process_mouse_button_event(*button, *pressed, pos);
                    }
                    egui::Event::PointerMoved(pos) => {
                        if let Some(new_pos) =
                            self.process_mouse_move_event(pos, &self.ui_state.last_pointer_pos)
                        {
                            self.ui_state.last_pointer_pos = new_pos;
                        }
                    }
                    egui::Event::MouseWheel { delta, .. } => {
                        let pointer_pos = input.pointer.hover_pos().unwrap_or_default();
                        self.process_mouse_wheel_event(delta, &pointer_pos);
                    }
                    _ => {}
                }
            }
        });
    }

    fn refresh_mapping_profiles(&mut self) {
        let device_orientation = self.app_state.device_orientation();

        let Some(keyboard_mapper) = &self.app_state.keyboard_mapper else {
            debug!("Keyboard mapper not available for profile refresh");
            self.indicator.reset_profiles();
            return;
        };

        keyboard_mapper.refresh_profiles(self.app_state.device_serial(), device_orientation);

        let avail_profile_names = keyboard_mapper.get_avail_profiles();
        let active_profile_name = keyboard_mapper.get_active_profile_name();
        debug!(
            "Keyboard profiles refreshed: active={:?}, available={:?}",
            active_profile_name, avail_profile_names
        );

        self.indicator.update_active_profile(active_profile_name);
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
                    if let Some(sender) = &self.app_state.control_sender {
                        // Only turn OFF screen, wake up with physical power button
                        if let Err(e) = sender.send_set_display_power(false) {
                            error!("Failed to turn off screen: {}", e);
                        } else {
                            info!("Screen OFF (press physical power button to wake up)");
                        }
                    }
                }
                ToolbarEvent::ToggleKeyboardMapping => {
                    self.toggle_keyboard_mapping();
                }
                ToolbarEvent::ToggleMappingVisualization => {
                    self.toggle_mapping_visualization();
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

    /// Draw mapping visualization overlay on video
    fn draw_mapping_overlay(&mut self, ctx: &egui::Context) {
        if !self.ui_state.mapping_visualization_enabled() {
            return;
        }

        let video_rect = self.player.video_rect();
        if video_rect.width() <= 0.0 || video_rect.height() <= 0.0 || video_rect.min.x.is_nan() {
            return;
        }

        let Some(keyboard_mapper) = &self.app_state.keyboard_mapper else {
            return;
        };

        let Some(mappings) = keyboard_mapper.get_active_mappings() else {
            return;
        };

        egui::Area::new(egui::Id::new("mapping_overlay"))
            .fixed_pos(video_rect.min)
            .interactable(false)
            .show(ctx, |ui| {
                let painter = ui.painter();
                let mappings_read = mappings.read();

                for (key, action) in mappings_read.iter() {
                    use crate::config::mapping::MappingAction;

                    let pos = match action {
                        MappingAction::Tap { pos }
                        | MappingAction::TouchDown { pos }
                        | MappingAction::TouchMove { pos }
                        | MappingAction::TouchUp { pos }
                        | MappingAction::Scroll { pos, .. } => Some(pos),
                        MappingAction::Swipe { path, .. } => Some(&path[0]),
                        _ => None,
                    };

                    if let Some(mapping_pos) = pos {
                        let screen_pos = self.ui_state.visual_coords().from_mapping(
                            mapping_pos,
                            &video_rect,
                            self.app_state.scrcpy_coords(),
                            self.ui_state.mapping_coords(),
                        );

                        let key_text = format!("{:?}", key);

                        painter.circle_filled(
                            screen_pos,
                            12.0,
                            egui::Color32::from_rgba_unmultiplied(0, 255, 0, 60),
                        );

                        painter.text(
                            screen_pos,
                            egui::Align2::CENTER_CENTER,
                            &key_text,
                            egui::FontId::proportional(10.0),
                            egui::Color32::from_rgba_unmultiplied(0, 0, 0, 180),
                        );
                    }
                }
            });
    }
}

impl Drop for SAideApp {
    fn drop(&mut self) {
        debug!("SAideApp dropping, cleaning up connection");

        // Explicitly shutdown connection to ensure server process is killed
        if let Some(mut conn) = self.app_state.connection.take()
            && let Err(e) = conn.shutdown()
        {
            debug!("Failed to shutdown connection: {}", e);
        }

        debug!("SAideApp cleanup completed");
    }
}

impl eframe::App for SAideApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        debug!("SAideApp exiting, cancelling background tasks");

        // Cancel background tasks
        self.app_state.cancel_token.cancel();
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.shutdown_requested {
            // Check for shutdown signal
            if self.shutdown_rx.try_recv().is_ok() {
                info!("Shutdown signal received, closing application");
                self.shutdown_requested = true;
            }
        }

        if self.shutdown_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

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
                let old_dimensions = if self.ui_state.is_ui_initialized() {
                    self.player.video_dimensions()
                } else {
                    // First frame after init, force resize
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

                    // Update ScrcpyCoordSys (video resolution changed)
                    self.app_state
                        .scrcpy_coords_mut()
                        .update_video_size(new_dimensions.0 as u16, new_dimensions.1 as u16);

                    // Mark UI as initialized
                    self.ui_state.set_ui_initialized();
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
                self.player.draw(ui);
            });

        // Draw indicator overlay on top of video
        if self.init_state == InitState::Ready && self.config().general.indicator {
            self.indicator.update_video_stats(self.player.video_stats());
            self.draw_indicator(ctx);
        }

        if self.init_state == InitState::Ready {
            self.draw_mapping_overlay(ctx);
        }

        // Show audio warning if present (overlay at top)
        if let Some(warning) = self.ui_state.audio_warning.clone() {
            let mut close_clicked = false;
            egui::Area::new(egui::Id::new("audio_warning"))
                .fixed_pos(egui::pos2(Toolbar::width() + 10.0, 10.0))
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
                                        egui::RichText::new(t!("audio-warning-title"))
                                            .color(egui::Color32::YELLOW)
                                            .strong(),
                                    );
                                    ui.label(
                                        egui::RichText::new(&warning)
                                            .color(egui::Color32::LIGHT_GRAY),
                                    );
                                });
                                if ui.button(t!("audio-warning-close")).clicked() {
                                    close_clicked = true;
                                }
                            });
                        });
                });
            if close_clicked {
                self.ui_state.audio_warning = None;
            }
        }

        // Frame rate limiting (only when streaming)
        if matches!(self.player.state(), PlayerState::Streaming) {
            if let Some(limiter) = self.config_state.frame_rate_limiter() {
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
