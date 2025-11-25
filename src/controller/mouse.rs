use {
    super::adb::{AdbInputCommand, AdbShell},
    crate::config::mapping::MouseConfig,
    anyhow::Result,
    tracing::{error, info},
};

pub enum WheelDirection {
    Up,
    Down,
}

/// Mouse mapping state
pub struct MouseMapper {
    config: MouseConfig,
    adb: AdbShell,
    enabled: bool,
    screen_size: (u32, u32),
}

impl MouseMapper {
    /// Create a new mouse mapper
    pub fn new(config: MouseConfig) -> Result<Self> {
        let initial_state = config.initial_state;
        Ok(Self {
            config,
            adb: AdbShell::new(),
            enabled: initial_state,
            screen_size: AdbShell::get_screen_size().map_err(|e| e)?,
        })
    }

    /// Enable or disable mouse mapping
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        info!(
            "Mouse mapping {}",
            if enabled { "enabled" } else { "disabled" }
        );
    }

    /// Check if mouse mapping is enabled
    pub fn is_enabled(&self) -> bool { self.enabled }

    pub fn coordinate_transform(
        &self,
        x: f32,
        y: f32,
        video_width: u32,
        video_height: u32,
        rotation: u32,
    ) -> (u32, u32) {
        let (screen_width, screen_height) = self.screen_size;

        // Apply rotation transform to video coordinates
        let (rotated_x, rotated_y) = match rotation % 4 {
            // 0 degrees - no rotation
            0 => (x, y),
            // 90 degrees clockwise - transpose and flip X
            1 => (video_height as f32 - y, x),
            // 180 degrees - flip both axes
            2 => (video_width as f32 - x, video_height as f32 - y),
            // 270 degrees clockwise - transpose and flip Y
            3 => (y, video_width as f32 - x),
            _ => (x, y),
        };

        // Direct mapping from video space to device space
        let device_x = rotated_x / video_width as f32 * screen_width as f32;
        let device_y = rotated_y / video_height as f32 * screen_height as f32;

        (device_x as u32, device_y as u32)
    }

    /// Handle mouse button event
    pub fn handle_button_event(
        &mut self,
        button: &str,
        pressed: bool,
        x: f32,
        y: f32,
        video_width: u32,
        video_height: u32,
        rotation: u32,
    ) -> Result<()> {
        if !self.is_enabled() {
            return Ok(());
        }

        let (device_x, device_y) =
            self.coordinate_transform(x, y, video_width, video_height, rotation);

        // Find matching mouse mapping
        for mapping in &self.config.mappings {
            if mapping.button == button {
                info!(
                    "Mouse button {} (pressed: {}) at ({}, {}) -> {}",
                    button, pressed, device_x, device_y, mapping.action
                );

                if !pressed {
                    // Only send command on button release for TAP actions
                    continue;
                }

                // Execute action based on mapping
                match mapping.action.as_str() {
                    "TAP" => {
                        if let Err(e) = self.adb.send_input(AdbInputCommand::Tap {
                            x: device_x,
                            y: device_y,
                        }) {
                            error!("Failed to send tap command: {}", e);
                        }
                    }
                    "BACK" => {
                        if let Err(e) = self.adb.send_input(AdbInputCommand::Back) {
                            error!("Failed to send back command: {}", e);
                        }
                    }
                    "HOME" => {
                        if let Err(e) = self.adb.send_input(AdbInputCommand::Home) {
                            error!("Failed to send home command: {}", e);
                        }
                    }
                    "MENU" => {
                        if let Err(e) = self.adb.send_input(AdbInputCommand::Menu) {
                            error!("Failed to send menu command: {}", e);
                        }
                    }
                    "SWIPE" => {
                        if let Some(ref dir) = mapping.dir {
                            let (x2, y2) = match dir.as_str() {
                                "UP" => (device_x, device_y.saturating_sub(200)),
                                "DOWN" => (
                                    device_x,
                                    if device_y <= self.screen_size.1 - 200 {
                                        device_y + 200
                                    } else {
                                        self.screen_size.1
                                    },
                                ),
                                "LEFT" => (device_x.saturating_sub(200), device_y),
                                "RIGHT" => (
                                    if device_x < self.screen_size.0 - 200 {
                                        device_x + 200
                                    } else {
                                        self.screen_size.0
                                    },
                                    device_y,
                                ),
                                _ => (device_x, device_y.saturating_sub(200)),
                            };

                            if let Err(e) = self.adb.send_input(AdbInputCommand::Swipe {
                                x1: device_x,
                                y1: device_y,
                                x2,
                                y2,
                                duration: 100,
                            }) {
                                error!("Failed to send swipe command: {}", e);
                            }
                        }
                    }
                    _ => {
                        error!("Unknown mouse action: {}", mapping.action);
                    }
                }
                break;
            }
        }

        Ok(())
    }

    /// Handle mouse wheel event
    pub fn handle_wheel_event(
        &mut self,
        x: f32,
        y: f32,
        video_width: u32,
        video_height: u32,
        rotation: u32,
        dir: WheelDirection,
    ) -> Result<()> {
        if !self.is_enabled() {
            return Ok(());
        }

        let (device_x, device_y) =
            self.coordinate_transform(x, y, video_width, video_height, rotation);

        match dir {
            WheelDirection::Up => self.adb.send_input(AdbInputCommand::Swipe {
                x1: device_x,
                y1: device_y,
                x2: device_x,
                y2: device_y.saturating_sub(300),
                duration: 100,
            }),
            WheelDirection::Down => self.adb.send_input(AdbInputCommand::Swipe {
                x1: device_x,
                y1: device_y,
                x2: device_x,
                y2: if device_y <= self.screen_size.1 - 300 {
                    device_y + 300
                } else {
                    self.screen_size.1
                },
                duration: 100,
            }),
        };

        Ok(())
    }
}
