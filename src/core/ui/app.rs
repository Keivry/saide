// SPDX-License-Identifier: MIT OR Apache-2.0

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
            profile_manager::ProfileManager,
            state::{AppState, ConfigState, UIState},
        },
        AppCommand,
        EditorRequest,
        PendingCommand,
        SHORTCUT_MANAGER,
        editor::{MAPPING_EDITOR_SHORTCUTS, MappingEditor},
        indicator::Indicator,
        notifier::Notifier,
        player::{PlayerState, StreamPlayer},
        theme::AppColors,
        toolbar::{Toolbar, ToolbarEvent, ToolbarViewState},
    },
    crate::{
        capture::{
            CaptureEvent,
            recorder::{RecorderConfig, RecorderHandle},
            screenshot::take_screenshot,
        },
        config::{
            SAideConfig,
            mapping::{Key, MappingAction, MouseButton, ScrcpyAction, WheelDirection},
        },
        controller::mouse::MouseState,
        core::{
            coords::{MappingCoordSys, ScrcpyCoordSys, VisualCoordSys},
            state::ToolbarMode,
            ui::editor::EditorParams,
            utils::find_nearest_mapping,
        },
        error::Result,
        modal::{DialogState, ModalDialog},
        scrcpy::{
            codec_probe::{EncoderProfileDatabase, ProbeStep},
            connection::AudioDisabledReason,
        },
        t,
        tf,
    },
    crossbeam_channel::{Receiver, Sender, bounded},
    std::{sync::Arc, thread, time::Instant},
    tokio_util::sync::CancellationToken,
    tracing::{debug, error, info, trace, warn},
};

/// Initialization state enum
#[derive(PartialEq)]
pub(crate) enum InitState {
    NotStarted,
    Probing,
    InProgress,
    Ready,
    /// A fatal error occurred before or during initialization.
    /// Each frame, `update()` calls `show_fatal_init_error_dialog()` until the
    /// dialog is acknowledged, after which the app exits via `shutdown_requested`.
    Failed(String),
}

pub struct SAideApp {
    pub(super) shutdown_rx: Receiver<()>,
    pub(super) shutdown_requested: bool,

    pub(super) toolbar: Toolbar,
    pub(super) indicator: Indicator,
    pub(super) player: StreamPlayer,

    pub(super) dialog: Option<ModalDialog>,
    pub(super) mapping_editor: Option<MappingEditor>,
    pub(super) pending_command: Option<PendingCommand>,

    pub(super) app_state: AppState,
    pub(super) config_state: ConfigState,
    pub(super) ui_state: UIState,

    pub(super) init_state: InitState,
    pub(super) init_instant: Option<Instant>,
    pub(super) init_rx: Option<Receiver<InitEvent>>,

    pub(super) probe_rx: Option<Receiver<ProbeStep>>,
    pub(super) probe_current_step: Option<ProbeStep>,

    pub(super) notifier: Notifier,
    pub(super) profile_manager: ProfileManager,

    pub(super) recorder: Option<RecorderHandle>,
    pub(super) capture_event_tx: Sender<CaptureEvent>,
    pub(super) capture_event_rx: Receiver<CaptureEvent>,

    pub(super) startup_warnings: Vec<String>,
}

const FLOATING_TOOLBAR_EDGE_TRIGGER_WIDTH: f32 = 4.0;

fn locked_capture_video_rotation(capture_orient: u32, display_rotation: u32) -> u32 {
    (4 - ((capture_orient + display_rotation) % 4)) % 4
}

impl SAideApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        serial: &str,
        config_manager: crate::config::ConfigManager,
        shutdown_rx: Receiver<()>,
        startup_error: Option<String>,
        startup_warnings: Vec<String>,
    ) -> Self {
        let config = config_manager.config();
        let indicator_position = config.general.indicator_position;
        let max_fps = config.scrcpy.video.max_fps;
        let audio_buffer_frames = config.scrcpy.audio.buffer_frames;
        let audio_ring_capacity = config.scrcpy.audio.ring_capacity;
        let hwdecode = config.gpu.hwdecode;

        let cancel_token = CancellationToken::new();

        let (capture_event_tx, capture_event_rx) = bounded::<CaptureEvent>(16);

        let init_state = match startup_error {
            Some(msg) => InitState::Failed(msg),
            None => InitState::NotStarted,
        };

        Self {
            shutdown_rx,
            shutdown_requested: false,

            toolbar: Toolbar::new(),
            indicator: Indicator::new(indicator_position, max_fps as f32),
            player: StreamPlayer::new(
                cc,
                cancel_token.clone(),
                audio_buffer_frames,
                audio_ring_capacity,
                hwdecode,
            ),

            dialog: None,
            mapping_editor: None,
            pending_command: None,

            app_state: AppState::new(serial.to_owned(), cancel_token),
            config_state: ConfigState::new(config_manager),
            ui_state: UIState::new(),

            init_state,
            init_instant: None,
            init_rx: None,

            probe_rx: None,
            probe_current_step: None,

            notifier: Notifier::new(),
            profile_manager: ProfileManager::new(&config.mappings.profiles),

            recorder: None,
            capture_event_tx,
            capture_event_rx,

            startup_warnings,
        }
    }

    pub fn config(&self) -> Arc<SAideConfig> { self.config_state.config() }

    fn toolbar_mode(&self) -> ToolbarMode { self.config_state.toolbar_mode() }

    fn toolbar_layout_width(&self) -> f32 {
        if self.toolbar_mode().is_floating() {
            0.0
        } else {
            Toolbar::width()
        }
    }

    fn audio_warning_offset_x(&self) -> f32 {
        if self.toolbar_mode().is_docked() || self.ui_state.floating_toolbar_visible() {
            Toolbar::width() + 10.0
        } else {
            10.0
        }
    }

    fn visible_floating_toolbar_rect(&self, content_rect: egui::Rect) -> Option<egui::Rect> {
        if self.toolbar_mode().is_floating() && self.ui_state.floating_toolbar_visible() {
            Some(egui::Rect::from_min_max(
                content_rect.left_top(),
                egui::pos2(
                    content_rect.left() + Toolbar::width(),
                    content_rect.bottom(),
                ),
            ))
        } else {
            None
        }
    }

    fn is_in_toolbar_input_block_rect(
        &self,
        floating_toolbar_rect: Option<egui::Rect>,
        pos: &VisualPos,
    ) -> bool {
        floating_toolbar_rect.is_some_and(|rect| rect.contains(*pos))
    }

    fn has_active_mappings(&self) -> bool {
        self.profile_manager
            .get_active_profile()
            .map(|profile| !profile.read().is_empty())
            .unwrap_or(false)
    }

    fn toolbar_view_state(&self) -> ToolbarViewState {
        ToolbarViewState {
            keyboard_mapping_enabled: self.config_state.keyboard_custom_mapping_enabled(),
            mapping_visualization_enabled: self.ui_state.mapping_visualization_enabled(),
            has_active_mappings: self.has_active_mappings(),
            mode: self.toolbar_mode(),
            is_recording: self.recorder.is_some(),
        }
    }

    fn set_toolbar_mode(&mut self, ctx: &egui::Context, mode: ToolbarMode) {
        let previous_mode = self.toolbar_mode();
        if previous_mode == mode {
            return;
        }

        self.config_state.set_toolbar_mode(mode);
        self.ui_state.pending_toggle_float = true;
        self.ui_state
            .set_floating_toolbar_visible(mode.is_floating());
        self.resize(ctx);
        ctx.request_repaint();

        info!("Toolbar mode toggled: {}", mode.as_str());
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

    fn start_probe(&mut self) {
        self.init_state = InitState::Probing;
        self.probe_current_step = None;

        let (tx, rx) = bounded::<ProbeStep>(32);
        self.probe_rx = Some(rx);

        let serial = self.app_state.device_serial().to_owned();
        let server_jar = self.config().general.scrcpy_server.clone();

        thread::spawn(move || {
            let result = crate::scrcpy::codec_probe::probe_device(&serial, &server_jar, Some(&tx));
            if let Err(ref e) = result {
                let _ = tx.send(ProbeStep::Done(Err(e.to_string())));
            }
        });
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
                        self.ui_state.audio_warning =
                            audio_disabled_reason.map(localize_audio_warning);

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

                        {
                            let coords = self.app_state.scrcpy_coords_mut();
                            coords.update_capture_orientation(corientation);
                            coords.update_video_size(
                                video_resolution.0 as u16,
                                video_resolution.1 as u16,
                            );
                        }

                        self.indicator.update_capture_orientation(corientation);
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
                    .update_display_rotation(self.app_state.display_rotation());
                debug!(
                    "Initial display rotation: {}",
                    self.app_state.display_rotation()
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
    /// Intelligently scales down when video exceeds screen bounds
    fn resize(&mut self, ctx: &egui::Context) {
        let (video_w, video_h) = self.player.video_dimensions();

        if video_w == 0 || video_h == 0 {
            return;
        }

        let config = self.config_state.config();
        let smart_resize = config.general.smart_window_resize;

        let (window_w, window_h) = if smart_resize {
            let screen_rect = ctx.input(|i| i.viewport().monitor_size);

            if let Some(monitor_size) = screen_rect {
                let screen_w = monitor_size.x;
                let screen_h = monitor_size.y;

                calculate_window_size(video_w, video_h, screen_w, screen_h)
            } else {
                (video_w, video_h)
            }
        } else {
            (video_w, video_h)
        };

        debug!(
            "Resizing window to {}x{} (video: {}x{}, smart_resize: {})",
            window_w, window_h, video_w, video_h, smart_resize
        );

        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
            window_w as f32 + self.toolbar_layout_width(),
            window_h as f32,
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

    /// Toggle mapping editor
    fn toggle_editor(&mut self, _ctx: &egui::Context) {
        if self.mapping_editor.is_some() {
            self.mapping_editor = None;
        } else {
            self.mapping_editor = Some(MappingEditor::new());
        }
    }

    /// Toggle keyboard custom mapping
    fn toggle_custom_keyboard_mapping(&mut self) {
        self.config_state.toggle_keyboard_custom_mapping();

        info!(
            "Keyboard custom mapping toggled: {}",
            self.config_state.keyboard_custom_mapping_enabled()
        );
    }

    /// Toggle mapping visualization overlay
    fn toggle_mapping_overlay(&mut self) {
        self.ui_state.toggle_mapping_visualization();

        info!(
            "Mapping visualization toggled: {}",
            self.ui_state.mapping_visualization_enabled()
        );
    }

    /// Check if a given position is within the video rectangle
    fn is_in_video_rect(&self, pos: &VisualPos) -> bool {
        let video_rect = self.player.video_rect();
        pos.x >= video_rect.left()
            && pos.x <= video_rect.right()
            && pos.y >= video_rect.top()
            && pos.y <= video_rect.bottom()
    }

    /// Process device monitor events
    fn process_device_monitor_events(&mut self, ctx: &egui::Context) {
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
                        self.app_state.display_rotation = new_orientation % 4;
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

        if rotated {
            self.indicator
                .update_display_rotation(self.app_state.display_rotation());

            self.ui_state
                .mapping_coords_mut()
                .update_display_rotation(self.app_state.display_rotation());
            debug!(
                "Updated display rotation to {}",
                self.app_state.display_rotation()
            );

            self.apply_auto_rotation(ctx);
            self.refresh_mapping_profiles();
        }
    }

    /// Apply automatic video rotation compensation when capture_orientation is locked
    ///
    /// When capture_orientation is set (video locked to a specific orientation),
    /// adjust video_rotation to cancel the total clockwise rotation already present
    /// in the encoded video.
    ///
    /// `display_rotation` comes from `Display.getRotation()`/`Surface.ROTATION_*`,
    /// which describes the current Android display-content rotation. In locked-capture
    /// mode, the encoded frame orientation is affected by both the capture lock and the
    /// display rotation, so the player must compensate their combined rotation instead
    /// of treating them as two absolute orientations to subtract.
    fn apply_auto_rotation(&mut self, ctx: &egui::Context) {
        if let Some(capture_orient) = self.app_state.scrcpy_coords().capture_orientation {
            let display_rotation = self.app_state.display_rotation();

            let target_rotation = locked_capture_video_rotation(capture_orient, display_rotation);

            self.player.set_rotation(target_rotation);
            self.indicator.update_video_rotation(target_rotation);
            self.ui_state
                .visual_coords_mut()
                .update_rotation(target_rotation);

            self.resize(ctx);
            self.indicator
                .update_video_resolution(self.player.video_dimensions());

            info!(
                "Auto-rotation: display={} ({}°), capture={} ({}°), applying video_rotation={}",
                display_rotation,
                display_rotation * 90,
                capture_orient,
                capture_orient * 90,
                target_rotation
            );
        }
    }

    pub(super) fn add_mapping(&mut self, key: Key, pos: &MappingPos) {
        let Some(keyboard_mapper) = &self.app_state.keyboard_mapper else {
            error!("Keyboard mapper not initialized");
            return;
        };

        let mapping_action = MappingAction::Tap { pos: *pos };
        let scrcpy_action = ScrcpyAction::from_mapping_action(
            &mapping_action,
            self.app_state.scrcpy_coords(),
            self.ui_state.mapping_coords(),
        );

        match self.profile_manager.add_mapping(key, mapping_action) {
            Ok(_) => {
                info!(
                    "Mapping added to profile: {:?} -> ({:.4}, {:.4})",
                    key, pos.x, pos.y
                );

                keyboard_mapper.add_mapping(key, scrcpy_action);
            }
            Err(e) => {
                error!("Failed to add mapping to profile: {}", e);
                self.notify(&t!("notification-add-mapping-failed"));
            }
        }

        info!("Adding mapping: {:?} -> ({:.4}, {:.4})", key, pos.x, pos.y);

        self.save_config();
    }

    pub(super) fn remove_mapping(&mut self, key: Key) {
        let Some(keyboard_mapper) = &self.app_state.keyboard_mapper else {
            error!("Keyboard mapper not initialized");
            return;
        };

        match self.profile_manager.remove_mapping(&key) {
            Ok(_) => {
                info!("Mapping deleted from profile: {:?}", key);
                keyboard_mapper.remove_mapping(&key);
            }
            Err(e) => {
                error!("Failed to delete mapping from profile: {}", e);
                self.notify(&t!("notification-delete-mapping-failed"));
            }
        }

        self.save_config();
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
            self.toggle_custom_keyboard_mapping();
            return Ok(true);
        }

        let Some(keyboard_mapper) = &self.app_state.keyboard_mapper else {
            debug!("Keyboard mapper not available, ignoring key event");
            return Ok(false);
        };

        if self.config_state.keyboard_custom_mapping_enabled()
            && !self.config_state.device_ime_state()
            && keyboard_mapper.handle_custom_keymapping_event(key)?
        {
            return Ok(true);
        }

        if modifiers.shift_only() && keyboard_mapper.handle_shifted_key_event(key)? {
            return Ok(true);
        }

        if modifiers.any() && keyboard_mapper.handle_keycombo_event(modifiers, key)? {
            return Ok(true);
        }

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

        // Skip normal input processing if mapping editor is open or dialogs are open
        if self.mapping_editor.is_some() || self.dialog.is_some() {
            return;
        }

        // Update mouse state (check for long press and send drag updates)
        if self.config_state.mouse_enabled()
            && let Some(mapper) = self.app_state.mouse_mapper.as_ref()
            && let Err(e) = mapper.update()
        {
            error!("Failed to update mouse mapper: {}", e);
        }

        let floating_toolbar_rect = self.visible_floating_toolbar_rect(ctx.content_rect());
        let skip_pointer_events = std::mem::take(&mut self.ui_state.pending_toggle_float);

        ctx.input(|input| {
            // Flag to ignore text events if egui::Event::Key was processed
            let mut ignore_text_events = false;

            for event in &input.events {
                // Process keyboard events
                if self.config_state.keyboard_enabled() {
                    match event {
                        egui::Event::Key {
                            key,
                            pressed,
                            modifiers,
                            ..
                        } => match self.process_keyboard_event(key, *pressed, *modifiers) {
                            Ok(handled) => {
                                if handled {
                                    ignore_text_events = true;
                                }
                            }
                            Err(e) => {
                                info!("Failed to handle keyboard event: {}", e);
                            }
                        },
                        egui::Event::Text(text) if !ignore_text_events && !text.is_empty() => {
                            if let Some(mapper) = self.app_state.keyboard_mapper.as_ref()
                                && let Err(e) = mapper.handle_text_input_event(text)
                            {
                                info!("Failed to handle text input event: {}", e);
                            }
                        }
                        _ => {}
                    }
                }

                // Process mouse events
                if !self.config_state.mouse_enabled() {
                    continue;
                }

                // Skip pointer/wheel events on the frame when toolbar mode was toggled
                // to prevent click-through to the video layer after layout changes.
                if skip_pointer_events {
                    match event {
                        egui::Event::PointerButton { .. }
                        | egui::Event::PointerMoved(_)
                        | egui::Event::MouseWheel { .. } => continue,
                        _ => {}
                    }
                }

                match event {
                    egui::Event::PointerButton {
                        button,
                        pressed,
                        pos,
                        ..
                    } => {
                        if self.is_in_toolbar_input_block_rect(floating_toolbar_rect, pos)
                            && (*pressed || !self.has_active_pointer_interaction())
                        {
                            continue;
                        }
                        self.process_mouse_button_event(*button, *pressed, pos);
                    }
                    egui::Event::PointerMoved(pos) => {
                        if self.is_in_toolbar_input_block_rect(floating_toolbar_rect, pos)
                            && !self.has_active_pointer_interaction()
                        {
                            continue;
                        }
                        if let Some(new_pos) =
                            self.process_mouse_move_event(pos, &self.ui_state.last_pointer_pos)
                        {
                            self.ui_state.last_pointer_pos = new_pos;
                        }
                    }
                    egui::Event::MouseWheel { delta, .. } => {
                        let pointer_pos = input.pointer.hover_pos().unwrap_or_default();
                        if self.is_in_toolbar_input_block_rect(floating_toolbar_rect, &pointer_pos)
                        {
                            continue;
                        }
                        self.process_mouse_wheel_event(delta, &pointer_pos);
                    }
                    _ => {}
                }
            }
        });
    }

    fn has_active_pointer_interaction(&self) -> bool {
        self.app_state
            .mouse_mapper
            .as_ref()
            .is_some_and(|mouse_mapper| mouse_mapper.get_button_state() != MouseState::Idle)
    }

    fn schedule_streaming_repaint(
        &self,
        ctx: &egui::Context,
        received_new_frame: bool,
        saw_input_event: bool,
    ) {
        let is_active =
            received_new_frame || saw_input_event || self.has_active_pointer_interaction();
        let limiter = if is_active {
            self.config_state.active_frame_rate_limiter()
        } else {
            self.config_state.idle_frame_rate_limiter()
        };

        match limiter {
            Some(duration) => ctx.request_repaint_after(duration),
            None => ctx.request_repaint(),
        }
    }

    pub(super) fn refresh_mapping_profiles(&mut self) {
        let Some(keyboard_mapper) = &self.app_state.keyboard_mapper else {
            debug!("Keyboard mapper not available for profile refresh");
            self.indicator.reset_profiles();
            return;
        };

        self.profile_manager.update(
            &self.app_state.device_serial,
            self.app_state.display_rotation,
        );

        let mut active_profile_name = None;
        if let Some(active_profile) = self.profile_manager.get_active_profile() {
            active_profile_name = Some(active_profile.read().name().to_string());
            keyboard_mapper.update_mappings(
                &active_profile,
                self.scrcpy_coords(),
                self.mapping_coords(),
            );
        } else {
            keyboard_mapper.clear_mappings();
        }
        let avail_profile_names = self.profile_manager.get_avail_profile_names();
        debug!(
            "Keyboard profiles refreshed: active={:?}, available={:?}",
            active_profile_name, avail_profile_names
        );

        self.indicator.update_active_profile(active_profile_name);
    }

    pub(super) fn save_config(&mut self) {
        if self.config_state.config_manager.is_degraded() {
            self.notify(&t!("notification-save-skipped-degraded"));
            return;
        }
        if let Err(e) = self.config_state.config_manager.save() {
            error!("Failed to save config: {}", e);
            self.notify(&t!("notification-save-config-failed"));
        } else {
            info!("Config saved successfully");
        }
    }

    fn handle_toolbar_event(&mut self, ctx: &egui::Context, event: ToolbarEvent) {
        if event != ToolbarEvent::None && self.init_state != InitState::Ready {
            debug!("Ignoring toolbar event while app is not ready: {:?}", event);
            return;
        }

        match event {
            ToolbarEvent::RotateVideo => {
                self.rotate(ctx);
            }
            ToolbarEvent::ToggleMappingEditor => {
                self.toggle_editor(ctx);
            }
            ToolbarEvent::ToggleScreenPower => {
                debug!("Turning off screen from toolbar");
                if let Some(sender) = &self.app_state.control_sender {
                    if let Err(e) = sender.send_set_display_power(false) {
                        error!("Failed to turn off screen: {}", e);
                    } else {
                        info!("Screen OFF (press physical power button to wake up)");
                    }
                }
            }
            ToolbarEvent::ToggleKeyboardMapping => {
                self.toggle_custom_keyboard_mapping();
            }
            ToolbarEvent::ToggleMappingVisualization => {
                self.toggle_mapping_overlay();
            }
            ToolbarEvent::ToggleFloat => {
                self.set_toolbar_mode(ctx, self.toolbar_mode().toggled());
            }
            ToolbarEvent::TakeScreenshot => {
                self.take_screenshot();
            }
            ToolbarEvent::ToggleRecording => {
                self.toggle_recording();
            }
            ToolbarEvent::None => {}
        }
    }

    fn take_screenshot(&mut self) {
        let frame = self.player.current_frame();
        let Some(frame) = frame else {
            warn!("TakeScreenshot: no current frame available");
            return;
        };

        let save_dir = std::path::PathBuf::from(&self.config().general.screenshot_path);

        let rotation = self.player.rotation();
        take_screenshot(frame, save_dir, self.capture_event_tx.clone(), rotation);
    }

    fn toggle_recording(&mut self) {
        if let Some(handle) = self.recorder.take() {
            self.player.unregister_recorder();
            handle.stop();
            return;
        }

        let (width, height) = self.player.video_resolution();
        if width == 0 || height == 0 {
            warn!("ToggleRecording: video dimensions not available yet");
            return;
        }

        let config = self.config();
        let save_dir = std::path::PathBuf::from(&config.general.recording_path);

        let has_audio = self.ui_state.audio_warning.is_none();

        let recorder_config = RecorderConfig {
            width,
            height,
            save_dir,
            rotation: self.player.rotation(),
        };

        let handle =
            RecorderHandle::start(recorder_config, self.capture_event_tx.clone(), has_audio);
        let video_tx = handle.video_sender();
        let audio_tx = handle.audio_sender();
        self.player.register_recorder(video_tx, audio_tx);
        self.recorder = Some(handle);
        info!("Recording started");
        self.notifier.notify(&t!("notification-recording-started"));
    }

    fn poll_capture_events(&mut self) {
        let events: Vec<CaptureEvent> =
            std::iter::from_fn(|| self.capture_event_rx.try_recv().ok()).collect();

        for event in events {
            match event {
                CaptureEvent::ScreenshotSaved(path) => {
                    let path_str = path.display().to_string();
                    info!("Screenshot saved: {path_str}");
                    let msg = tf!("notification-screenshot-saved", "path" => path_str.as_str());
                    self.notifier.notify(&msg);
                }
                CaptureEvent::RecordingSaved(path) => {
                    self.player.unregister_recorder();
                    self.recorder = None;
                    let path_str = path.display().to_string();
                    info!("Recording saved: {path_str}");
                    let msg = tf!("notification-recording-stopped", "path" => path_str.as_str());
                    self.notifier.notify(&msg);
                }
                CaptureEvent::ScreenshotError(e) => {
                    error!("Screenshot error: {e}");
                    let msg = tf!("notification-capture-error", "error" => e.as_str());
                    self.notifier.notify(&msg);
                }
                CaptureEvent::RecordingError(e) => {
                    error!("Recording error: {e}");
                    self.player.unregister_recorder();
                    self.recorder = None;
                    let msg = tf!("notification-capture-error", "error" => e.as_str());
                    self.notifier.notify(&msg);
                }
            }
        }
    }

    fn update_floating_toolbar_visibility(&mut self, ctx: &egui::Context) {
        if self.toolbar_mode().is_docked() {
            self.ui_state.set_floating_toolbar_visible(false);
            return;
        }

        let content_rect = ctx.content_rect();
        let reveal_width = if self.ui_state.floating_toolbar_visible() {
            Toolbar::width() + FLOATING_TOOLBAR_EDGE_TRIGGER_WIDTH
        } else {
            FLOATING_TOOLBAR_EDGE_TRIGGER_WIDTH
        };
        let should_show = ctx.input(|input| {
            input.pointer.hover_pos().is_some_and(|pos| {
                pos.x <= content_rect.left() + reveal_width
                    && pos.y >= content_rect.top()
                    && pos.y <= content_rect.bottom()
            })
        });

        if should_show != self.ui_state.floating_toolbar_visible() {
            self.ui_state.set_floating_toolbar_visible(should_show);
            ctx.request_repaint();
        }
    }

    fn draw_docked_toolbar(&mut self, ctx: &egui::Context) {
        let toolbar_enabled = self.init_state == InitState::Ready;
        let toolbar_state = self.toolbar_view_state();

        let colors = AppColors::from_context(ctx);
        let event = egui::SidePanel::left("Toolbar")
            .frame(egui::Frame::NONE.fill(colors.toolbar_bg))
            .resizable(false)
            .exact_width(Toolbar::width())
            .show(ctx, |ui| {
                ui.add_enabled_ui(toolbar_enabled, |ui| self.toolbar.draw(ui, toolbar_state))
                    .inner
            })
            .inner;
        self.handle_toolbar_event(ctx, event);
    }

    fn draw_floating_toolbar(&mut self, ctx: &egui::Context) {
        if !self.ui_state.floating_toolbar_visible() {
            return;
        }

        let toolbar_enabled = self.init_state == InitState::Ready;
        let toolbar_state = self.toolbar_view_state();

        let colors = AppColors::from_context(ctx);
        let content_rect = ctx.content_rect();
        let toolbar_rect = self
            .visible_floating_toolbar_rect(content_rect)
            .unwrap_or_else(|| {
                egui::Rect::from_min_max(
                    content_rect.left_top(),
                    egui::pos2(
                        content_rect.left() + Toolbar::width(),
                        content_rect.bottom(),
                    ),
                )
            });
        let event = egui::Area::new(egui::Id::new("floating_toolbar"))
            .order(egui::Order::Foreground)
            .fixed_pos(toolbar_rect.left_top())
            .show(ctx, |ui| {
                egui::Frame::NONE.fill(colors.toolbar_bg).show(ui, |ui| {
                    ui.set_min_size(toolbar_rect.size());
                    ui.add_enabled_ui(toolbar_enabled, |ui| self.toolbar.draw(ui, toolbar_state))
                        .inner
                })
            })
            .inner
            .inner;
        self.handle_toolbar_event(ctx, event);
    }

    fn draw_toolbar(&mut self, ctx: &egui::Context) {
        if self.toolbar_mode().is_floating() {
            self.draw_floating_toolbar(ctx);
        } else {
            self.draw_docked_toolbar(ctx);
        }
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

        let Some(profile) = self.profile_manager.get_active_profile() else {
            debug!("No active profile, skipping mapping visualization overlay");
            return;
        };

        egui::Area::new(egui::Id::new("mapping_overlay"))
            .fixed_pos(video_rect.min)
            .interactable(false)
            .show(ctx, |ui| {
                let painter = ui.painter();

                for (key, action) in profile.read().iter() {
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
                        let colors = AppColors::from_context(ctx);

                        painter.circle_filled(screen_pos, 12.0, colors.mapping_overlay_fill);

                        painter.text(
                            screen_pos,
                            egui::Align2::CENTER_CENTER,
                            &key_text,
                            egui::FontId::proportional(10.0),
                            colors.mapping_overlay_text,
                        );
                    }
                }
            });
    }

    fn process_commands(&mut self, commands: Vec<AppCommand>) {
        for cmd in commands {
            match cmd {
                AppCommand::ShowHelp => self.show_help_dialog(),
                AppCommand::ShowProfileSelection => {
                    let had_dialog = self.dialog.is_some();
                    self.show_profile_selection_dialog();
                    if !had_dialog && self.dialog.is_some() {
                        self.pending_command = Some(PendingCommand::SwitchProfile);
                    }
                }
                AppCommand::PrevProfile => self.prev_profile(),
                AppCommand::NextProfile => self.next_profile(),
                AppCommand::ShowRenameDialog => {
                    if self.profile_manager.get_active_profile().is_none() {
                        let had_dialog = self.dialog.is_some();
                        self.show_create_profile_dialog();
                        if !had_dialog && self.dialog.is_some() {
                            self.pending_command = Some(PendingCommand::CreateProfile);
                        }
                    } else {
                        let had_dialog = self.dialog.is_some();
                        self.show_rename_profile_dialog();
                        if !had_dialog && self.dialog.is_some() {
                            self.pending_command = Some(PendingCommand::RenameProfile);
                        }
                    }
                }
                AppCommand::ShowCreateDialog => {
                    let had_dialog = self.dialog.is_some();
                    self.show_create_profile_dialog();
                    if !had_dialog && self.dialog.is_some() {
                        self.pending_command = Some(PendingCommand::CreateProfile);
                    }
                }
                AppCommand::ShowDeleteDialog => {
                    let had_dialog = self.dialog.is_some();
                    self.show_delete_profile_dialog();
                    if !had_dialog && self.dialog.is_some() {
                        self.pending_command = Some(PendingCommand::DeleteProfile);
                    }
                }
                AppCommand::ShowSaveAsDialog => {
                    let had_dialog = self.dialog.is_some();
                    self.show_save_profile_as_dialog();
                    if !had_dialog && self.dialog.is_some() {
                        self.pending_command = Some(PendingCommand::SaveProfileAs);
                    }
                }
                AppCommand::CloseEditor => self.close_mapping_editor(),
            }
        }
    }

    fn handle_editor_request(&mut self, request: EditorRequest) {
        if self.dialog.is_some() {
            return;
        }
        let video_rect = self.player.video_rect();
        match request {
            EditorRequest::AddMapping(screen_pos) => {
                let Some(mapping_pos) = self.ui_state.visual_coords().to_mapping(
                    &screen_pos,
                    &video_rect,
                    self.app_state.scrcpy_coords(),
                    self.ui_state.mapping_coords(),
                ) else {
                    return;
                };
                if self.profile_manager.get_active_profile().is_none() {
                    self.create_profile("Default");
                    if self.profile_manager.get_active_profile().is_none() {
                        return;
                    }
                }
                let had_dialog = self.dialog.is_some();
                self.show_add_mapping_dialog(&mapping_pos);
                if !had_dialog && self.dialog.is_some() {
                    self.pending_command = Some(PendingCommand::AddMapping(mapping_pos));
                }
            }
            EditorRequest::DeleteMapping(screen_pos) => {
                let had_dialog = self.dialog.is_some();
                let Some(mapping_pos) = self.ui_state.visual_coords().to_mapping(
                    &screen_pos,
                    &video_rect,
                    self.app_state.scrcpy_coords(),
                    self.ui_state.mapping_coords(),
                ) else {
                    return;
                };
                let Some(profile) = self.profile_manager.get_active_profile() else {
                    return;
                };
                let profile_lock = profile.read();
                let mappings = profile_lock.mappings();
                if let Some((nearest_key, nearest_pos, dist)) =
                    find_nearest_mapping(&mapping_pos, mappings)
                    && dist <= 0.04
                {
                    let key_text = format!("{nearest_key:?}");
                    self.show_delete_mapping_dialog(&nearest_pos, &key_text);
                    if !had_dialog && self.dialog.is_some() {
                        self.pending_command = Some(PendingCommand::DeleteMapping(nearest_key));
                    }
                }
            }
        }
    }

    fn apply_dialog_result(&mut self, result: DialogState) {
        if matches!(result, DialogState::None | DialogState::NoAction) {
            return;
        }

        let Some(pending) = self.pending_command.take() else {
            return;
        };

        match (pending, result) {
            (PendingCommand::RenameProfile, DialogState::String(name)) => {
                self.rename_profile(&name)
            }
            (PendingCommand::CreateProfile, DialogState::String(name)) => {
                self.create_profile(&name)
            }
            (PendingCommand::SaveProfileAs, DialogState::String(name)) => {
                self.save_profile_as(&name)
            }
            (PendingCommand::DeleteProfile, DialogState::Confirmed) => self.delete_profile(),
            (PendingCommand::SwitchProfile, DialogState::Usize(idx)) => self.switch_profile(idx),
            (PendingCommand::AddMapping(mapping_pos), DialogState::CapturedKey(key)) => {
                self.add_mapping(key, &mapping_pos);
            }
            (PendingCommand::DeleteMapping(key), DialogState::Confirmed) => {
                self.remove_mapping(key);
            }
            (PendingCommand::ProbeCodec, DialogState::Confirmed) => {
                self.start_probe();
            }
            (PendingCommand::ProbeCodec, DialogState::Cancelled) => {
                self.init();
            }
            (PendingCommand::FatalError, DialogState::Confirmed) => {
                self.shutdown_requested = true;
            }
            (_, DialogState::Cancelled) => {}
            (cmd, result) => {
                warn!(
                    "apply_dialog_result: unhandled (pending={cmd:?}, result={result:?}) — command dropped"
                );
            }
        }
    }

    pub fn draw_editor(&mut self, ui: &mut egui::Ui) -> Option<EditorRequest> {
        let Some(editor) = &self.mapping_editor else {
            return None;
        };

        let video_rect = self.player.video_rect();
        if video_rect.width() <= 0.0 || video_rect.height() <= 0.0 {
            return None;
        }

        let profile = self.profile_manager.get_active_profile();
        let visual_coords = self.visual_coords();
        let scrcpy_coords = self.scrcpy_coords();
        let mapping_coords = self.mapping_coords();
        let editor_params = EditorParams {
            profile,
            video_rect: &video_rect,
            visual_coords,
            scrcpy_coords,
            mapping_coords,
        };

        let mut overlay_ui = ui.new_child(egui::UiBuilder::new().max_rect(video_rect));
        editor.draw(&mut overlay_ui, editor_params)
    }

    /// Draw dialog, and close it if requested by the dialog state
    pub fn draw_dialog(&mut self, ctx: &egui::Context) -> DialogState {
        let Some(dialog) = &mut self.dialog else {
            return DialogState::None;
        };

        let state = dialog.draw(ctx);
        if state.is_closed() {
            self.dialog.take();
        }
        state
    }

    pub fn notify(&self, notification: &str) { self.notifier.notify(notification); }

    pub(super) fn scrcpy_coords(&self) -> &ScrcpyCoordSys { self.app_state.scrcpy_coords() }

    pub(super) fn mapping_coords(&self) -> &MappingCoordSys { self.ui_state.mapping_coords() }

    pub(super) fn visual_coords(&self) -> &VisualCoordSys { self.ui_state.visual_coords() }

    pub fn toolbar_width() -> f32 { Toolbar::width() }
}

impl Drop for SAideApp {
    fn drop(&mut self) {
        debug!("SAideApp dropping, cleaning up connection");

        if let Some(handle) = self.recorder.take() {
            handle.stop();
        }

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

        if let Some(handle) = self.recorder.take() {
            self.player.unregister_recorder();
            handle.stop();
        }

        self.app_state.cancel_token.cancel();
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut received_new_frame = false;

        self.update_floating_toolbar_visibility(ctx);
        self.poll_capture_events();

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

        if !self.startup_warnings.is_empty() {
            let warnings: Vec<String> = self.startup_warnings.drain(..).collect();
            for w in warnings {
                self.notify(&w);
            }
        }

        // Handle initialization state transitions
        match self.init_state {
            InitState::NotStarted => {
                let serial = self.app_state.device_serial().to_owned();
                let needs_probe = EncoderProfileDatabase::load()
                    .map(|db| db.get(&serial).is_none())
                    .unwrap_or(true);
                if needs_probe {
                    self.show_probe_codec_dialog();
                    self.pending_command = Some(PendingCommand::ProbeCodec);
                } else {
                    self.init();
                }
            }
            InitState::Probing => {
                if let Some(rx) = &self.probe_rx {
                    while let Ok(step) = rx.try_recv() {
                        let done = matches!(step, ProbeStep::Done(_));
                        if let ProbeStep::Done(Err(ref e)) = step {
                            let msg = tf!("probe-codec-done-failed", "error" => e.as_str());
                            self.notifier.notify(msg.as_str());
                        }
                        self.probe_current_step = Some(step);
                        if done {
                            self.probe_rx = None;
                            self.init();
                            break;
                        }
                    }
                }

                self.draw_probe_codec_progress(ctx);
                ctx.request_repaint();
            }
            InitState::InProgress => {
                // Update player to receive events
                received_new_frame = self.player.update();

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
                received_new_frame = self.player.update();

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

                    if let Some(sender) = &self.app_state.control_sender {
                        sender.update_screen_size(new_dimensions.0 as u16, new_dimensions.1 as u16);
                        debug!(
                            "Updated ControlSender screen size to {}x{}",
                            new_dimensions.0, new_dimensions.1
                        );
                    }

                    self.profile_manager.update(
                        &self.app_state.device_serial,
                        self.app_state.display_rotation,
                    );

                    if let Some(keyboard_mapper) = &self.app_state.keyboard_mapper
                        && let Some(active_profile) = self.profile_manager.get_active_profile()
                    {
                        keyboard_mapper.update_mappings(
                            &active_profile,
                            self.scrcpy_coords(),
                            self.mapping_coords(),
                        );
                        debug!(
                            "Reapplied keyboard mappings for new video resolution: {}x{}",
                            new_dimensions.0, new_dimensions.1
                        );
                    }

                    // Mark UI as initialized
                    self.ui_state.set_ui_initialized();
                }

                let editor_scope = self
                    .mapping_editor
                    .as_ref()
                    .filter(|_| self.dialog.is_none())
                    .map(|_| &*MAPPING_EDITOR_SHORTCUTS);
                let commands = SHORTCUT_MANAGER.dispatch_raw_with_extra(ctx, editor_scope);
                self.process_commands(commands);

                if self.dialog.is_none() {
                    self.process_input_events(ctx);
                }

                self.process_device_monitor_events(ctx);
            }
            InitState::Failed(ref reason) => {
                let reason = reason.clone();
                self.show_fatal_init_error_dialog(&reason);
            }
        }

        let mut editor_request = None;

        // Render player in center panel (no margin to maximize video area)
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                self.player.draw(ui);

                if self.init_state == InitState::Ready {
                    editor_request = self.draw_editor(ui);
                }
            });

        if let Some(editor_request) = editor_request {
            self.handle_editor_request(editor_request);
        }

        let dialog_result = self.draw_dialog(ctx);
        self.apply_dialog_result(dialog_result);

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
            let colors = AppColors::from_context(ctx);
            let mut close_clicked = false;
            egui::Area::new(egui::Id::new("audio_warning"))
                .fixed_pos(egui::pos2(self.audio_warning_offset_x(), 10.0))
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(colors.audio_warning_bg)
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

        // Toast notifications overlay
        self.notifier.draw(ctx, ctx.content_rect());

        // Frame rate limiting (only when streaming)
        if matches!(self.player.state(), PlayerState::Streaming) {
            let saw_input_event = ctx.input(|input| !input.events.is_empty());
            self.schedule_streaming_repaint(ctx, received_new_frame, saw_input_event);
        }
    }
}

/// Calculate appropriate window size based on video and screen dimensions
/// Returns (width, height) that fits within screen bounds
fn calculate_window_size(video_w: u32, video_h: u32, screen_w: f32, screen_h: f32) -> (u32, u32) {
    // Use 90% of screen dimensions as usable area
    // to leave some margin for taskbars, docks, etc.
    const SCREEN_MARGIN_RATIO: f32 = 0.9;

    let usable_w = (screen_w * SCREEN_MARGIN_RATIO) as u32;
    let usable_h = (screen_h * SCREEN_MARGIN_RATIO) as u32;

    if video_w <= usable_w && video_h <= usable_h {
        return (video_w, video_h);
    }

    let video_long = video_w.max(video_h);
    let video_short = video_w.min(video_h);
    let is_landscape = video_w >= video_h;

    for &tier in crate::constant::VIDEO_RESOLUTION_TIERS {
        if tier >= video_long {
            continue;
        }

        let scale = tier as f32 / video_long as f32;
        let scaled_long = tier;
        let scaled_short = (video_short as f32 * scale) as u32;

        let (candidate_w, candidate_h) = if is_landscape {
            (scaled_long, scaled_short)
        } else {
            (scaled_short, scaled_long)
        };

        if candidate_w <= usable_w && candidate_h <= usable_h {
            return (candidate_w, candidate_h);
        }
    }

    let smallest_tier = crate::constant::VIDEO_RESOLUTION_TIERS
        .last()
        .copied()
        .unwrap_or(640);
    let scale = smallest_tier as f32 / video_long as f32;
    let scaled_long = smallest_tier;
    let scaled_short = (video_short as f32 * scale) as u32;

    if is_landscape {
        (scaled_long, scaled_short)
    } else {
        (scaled_short, scaled_long)
    }
}

fn localize_audio_warning(reason: AudioDisabledReason) -> String {
    match reason {
        AudioDisabledReason::UnsupportedAndroidVersion { api_level } => {
            tf!("audio-warning-unsupported-android", "api_level" => api_level)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::locked_capture_video_rotation;

    #[test]
    fn locked_capture_rotation_matches_runtime_repro() {
        assert_eq!(locked_capture_video_rotation(0, 1), 3);
        assert_eq!(locked_capture_video_rotation(0, 3), 1);
        assert_eq!(locked_capture_video_rotation(1, 0), 3);
        assert_eq!(locked_capture_video_rotation(1, 1), 2);
        assert_eq!(locked_capture_video_rotation(2, 1), 1);
        assert_eq!(locked_capture_video_rotation(3, 1), 0);
    }
}
