//! SAide application state structures

use {
    super::{
        coords::{MappingCoordSys, ScrcpyCoordSys, VisualCoordSys, VisualPos},
        init::DeviceMonitorEvent,
    },
    crate::{
        config::ConfigManager,
        controller::{control_sender::ControlSender, keyboard::KeyboardMapper, mouse::MouseMapper},
        scrcpy::connection::ScrcpyConnection,
    },
    crossbeam_channel::Receiver,
    std::{sync::Arc, time::Duration},
    tokio_util::sync::CancellationToken,
};

/// Application connection and device state
pub struct AppState {
    /// ScrcpyConnection (kept alive to prevent server shutdown)
    pub connection: Option<ScrcpyConnection>,

    /// Control sender (for sending input commands to device)
    pub control_sender: Option<ControlSender>,

    /// Mouse input mapper
    pub mouse_mapper: Option<MouseMapper>,

    /// Keyboard input mapper
    pub keyboard_mapper: Option<KeyboardMapper>,

    /// Device monitor receiver
    pub device_monitor_rx: Option<Receiver<DeviceMonitorEvent>>,

    /// Device serial
    pub device_serial: String,

    /// Device orientation (0-3), clockwise
    pub device_orientation: u32,

    /// Scrcpy coordinate system
    pub scrcpy_coords: ScrcpyCoordSys,

    /// Cancellation token for background tasks
    pub cancel_token: CancellationToken,
}

impl AppState {
    pub fn new(device_serial: String, cancel_token: CancellationToken) -> Self {
        Self {
            connection: None,
            control_sender: None,
            mouse_mapper: None,
            keyboard_mapper: None,
            device_monitor_rx: None,
            device_serial,
            device_orientation: 0,
            scrcpy_coords: ScrcpyCoordSys::new(1, 1, None),
            cancel_token,
        }
    }

    pub fn device_serial(&self) -> &str { &self.device_serial }

    pub fn device_orientation(&self) -> u32 { self.device_orientation }

    pub fn scrcpy_coords(&self) -> &ScrcpyCoordSys { &self.scrcpy_coords }

    pub fn scrcpy_coords_mut(&mut self) -> &mut ScrcpyCoordSys { &mut self.scrcpy_coords }
}

/// Configuration and settings state
pub struct ConfigState {
    /// Configuration manager
    pub config_manager: ConfigManager,

    /// Keyboard mapping switch
    pub keyboard_enabled: bool,

    /// Mouse mapping switch
    pub mouse_enabled: bool,

    /// Keyboard custom mapping switch
    pub keyboard_custom_mapping_enabled: bool,

    /// Android device input method state
    pub device_ime_state: bool,

    /// Frame rate limiter duration
    pub frame_rate_limiter: Option<Duration>,
}

impl ConfigState {
    pub fn new(config_manager: ConfigManager) -> Self {
        let config = config_manager.config();

        let keyboard_enabled = config.general.keyboard_enabled;
        let mouse_enabled = config.general.mouse_enabled;
        let keyboard_custom_mapping_enabled = config.mappings.initial_state;

        let max_fps = config.scrcpy.video.max_fps;
        let vsync = config.gpu.vsync;

        let frame_rate_limiter = if vsync {
            None
        } else {
            // max_fps is validated as 1..=240 by config, but guard against 0 to prevent
            // +Infinity which would panic in Duration::from_secs_f64.
            Some(Duration::from_secs_f64(1.0 / max_fps.max(1) as f64))
        };

        Self {
            config_manager,
            keyboard_enabled,
            mouse_enabled,
            keyboard_custom_mapping_enabled,
            device_ime_state: false,
            frame_rate_limiter,
        }
    }

    pub fn config(&self) -> Arc<crate::config::SAideConfig> { self.config_manager.config() }

    pub fn keyboard_enabled(&self) -> bool { self.keyboard_enabled }

    pub fn mouse_enabled(&self) -> bool { self.mouse_enabled }

    pub fn keyboard_custom_mapping_enabled(&self) -> bool { self.keyboard_custom_mapping_enabled }

    pub fn toggle_keyboard_custom_mapping(&mut self) {
        self.keyboard_custom_mapping_enabled = !self.keyboard_custom_mapping_enabled;
    }

    pub fn device_ime_state(&self) -> bool { self.device_ime_state }

    pub fn frame_rate_limiter(&self) -> Option<Duration> { self.frame_rate_limiter }
}

/// UI state (visual components and transient UI data)
pub struct UIState {
    /// Audio disabled warning message (if audio was requested but unavailable)
    pub audio_warning: Option<String>,

    /// Last mouse pointer position in video rect
    pub last_pointer_pos: VisualPos,

    /// Mapping coordinate system
    pub mapping_coords: MappingCoordSys,

    /// Visual coordinate system
    pub visual_coords: VisualCoordSys,

    /// Ui initialization state, to trigger first-time setups
    /// (e.g. window resize)
    pub ui_initialized: bool,

    /// Mapping visualization enabled state
    pub mapping_visualization_enabled: bool,
}

impl UIState {
    pub fn new() -> Self {
        Self {
            audio_warning: None,
            last_pointer_pos: VisualPos::ZERO,
            mapping_coords: MappingCoordSys::new(0),
            visual_coords: VisualCoordSys::new(0),
            ui_initialized: false,
            mapping_visualization_enabled: false,
        }
    }

    pub fn mapping_coords(&self) -> &MappingCoordSys { &self.mapping_coords }

    pub fn mapping_coords_mut(&mut self) -> &mut MappingCoordSys { &mut self.mapping_coords }

    pub fn visual_coords(&self) -> &VisualCoordSys { &self.visual_coords }

    pub fn visual_coords_mut(&mut self) -> &mut VisualCoordSys { &mut self.visual_coords }

    pub fn is_ui_initialized(&self) -> bool { self.ui_initialized }

    pub fn set_ui_initialized(&mut self) { self.ui_initialized = true; }

    pub fn mapping_visualization_enabled(&self) -> bool { self.mapping_visualization_enabled }

    pub fn toggle_mapping_visualization(&mut self) {
        self.mapping_visualization_enabled = !self.mapping_visualization_enabled;
    }
}

impl Default for UIState {
    fn default() -> Self { Self::new() }
}
