use {
    super::adb::{AdbInputCommand, AdbShell},
    crate::config::mapping::MouseConfig,
    anyhow::Result,
    std::sync::{Arc, Mutex},
    tracing::{error, info},
};

/// Mouse mapping state
pub struct MouseMapper {
    config: MouseConfig,
    adb: Arc<Mutex<AdbShell>>,
    enabled: Arc<Mutex<bool>>,
}

impl MouseMapper {
    /// Create a new mouse mapper
    pub fn new(config: MouseConfig, adb: Arc<Mutex<AdbShell>>) -> Self {
        let initial_state = config.initial_state;
        Self {
            config,
            adb,
            enabled: Arc::new(Mutex::new(initial_state)),
        }
    }

    /// Enable or disable mouse mapping
    pub fn set_enabled(&self, enabled: bool) {
        let mut enabled_lock = self.enabled.lock().unwrap();
        *enabled_lock = enabled;
        info!(
            "Mouse mapping {}",
            if enabled { "enabled" } else { "disabled" }
        );
    }

    /// Check if mouse mapping is enabled
    pub fn is_enabled(&self) -> bool {
        let enabled_lock = self.enabled.lock().unwrap();
        *enabled_lock
    }

    /// Handle mouse button event
    pub fn handle_button_event(
        &self,
        button: &str,
        pressed: bool,
        x: f32,
        y: f32,
        video_width: u32,
        video_height: u32,
        rotation: u32,
        capture_orientation: &str,
    ) -> Result<()> {
        if !self.is_enabled() {
            return Ok(());
        }

        // Get device screen size
        let (screen_width, screen_height) = self.adb.lock().unwrap().get_cached_screen_size();

        // Apply rotation transform to video coordinates
        // Input x, y are in video space (0 to video_width/height)
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

        // For "TAP" action, we need to map this to the device screen
        // For devices without rotation or with 0 capture-orientation:
        // Direct mapping from video space to device space
        let (device_x, device_y) = if capture_orientation == "0" || capture_orientation.is_empty() {
            // No capture orientation adjustment
            let dx = (rotated_x / video_width as f32 * screen_width as f32).round() as i32;
            let dy = (rotated_y / video_height as f32 * screen_height as f32).round() as i32;
            (dx, dy)
        } else {
            // Adjust for device capture orientation
            match capture_orientation {
                "90" => {
                    // Device is captured rotated 90 degrees
                    // Video (rotated) -> Device space
                    // For a portrait device (height > width), rotated video needs width/height swap
                    if screen_height > screen_width {
                        // Portrait device
                        let dx =
                            (rotated_x / video_width as f32 * screen_height as f32).round() as i32;
                        let dy =
                            (rotated_y / video_height as f32 * screen_width as f32).round() as i32;
                        (dx, dy)
                    } else {
                        // Landscape device
                        let dx =
                            (rotated_x / video_width as f32 * screen_width as f32).round() as i32;
                        let dy =
                            (rotated_y / video_height as f32 * screen_height as f32).round() as i32;
                        (dx, dy)
                    }
                }
                "180" => {
                    // Device is rotated 180 degrees
                    let dx = (rotated_x / video_width as f32 * screen_width as f32).round() as i32;
                    let dy =
                        (rotated_y / video_height as f32 * screen_height as f32).round() as i32;
                    // Flip both coordinates
                    (screen_width as i32 - dx, screen_height as i32 - dy)
                }
                "270" => {
                    // Device is captured rotated 270 degrees
                    if screen_height > screen_width {
                        // Portrait device
                        let dx =
                            (rotated_x / video_width as f32 * screen_height as f32).round() as i32;
                        let dy =
                            (rotated_y / video_height as f32 * screen_width as f32).round() as i32;
                        // Flip X coordinate
                        (screen_height as i32 - dx, dy)
                    } else {
                        // Landscape device
                        let dx =
                            (rotated_x / video_width as f32 * screen_width as f32).round() as i32;
                        let dy =
                            (rotated_y / video_height as f32 * screen_height as f32).round() as i32;
                        // Flip Y coordinate
                        (dx, screen_width as i32 - dy)
                    }
                }
                _ => {
                    // Unknown orientation, use direct mapping
                    let dx = (rotated_x / video_width as f32 * screen_width as f32).round() as i32;
                    let dy =
                        (rotated_y / video_height as f32 * screen_height as f32).round() as i32;
                    (dx, dy)
                }
            }
        };

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
                        if let Err(e) = self.adb.lock().unwrap().send_input(AdbInputCommand::Tap {
                            x: device_x,
                            y: device_y,
                        }) {
                            error!("Failed to send tap command: {}", e);
                        }
                    }
                    "BACK" => {
                        if let Err(e) = self.adb.lock().unwrap().send_input(AdbInputCommand::Back) {
                            error!("Failed to send back command: {}", e);
                        }
                    }
                    "HOME" => {
                        if let Err(e) = self.adb.lock().unwrap().send_input(AdbInputCommand::Home) {
                            error!("Failed to send home command: {}", e);
                        }
                    }
                    "MENU" => {
                        if let Err(e) = self.adb.lock().unwrap().send_input(AdbInputCommand::Menu) {
                            error!("Failed to send menu command: {}", e);
                        }
                    }
                    "SWIPE" => {
                        if let Some(ref dir) = mapping.dir {
                            let (x2, y2) = match dir.as_str() {
                                "UP" => (device_x, device_y - 200),
                                "DOWN" => (device_x, device_y + 200),
                                "LEFT" => (device_x - 200, device_y),
                                "RIGHT" => (device_x + 200, device_y),
                                _ => (device_x, device_y - 200),
                            };

                            if let Err(e) =
                                self.adb.lock().unwrap().send_input(AdbInputCommand::Swipe {
                                    x1: device_x,
                                    y1: device_y,
                                    x2,
                                    y2,
                                    duration: 100,
                                })
                            {
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
        &self,
        _delta_x: f32,
        delta_y: f32,
        video_width: u32,
        video_height: u32,
        _rotation: u32,
        _capture_orientation: &str,
    ) -> Result<()> {
        if !self.is_enabled() {
            return Ok(());
        }

        // Get device screen size
        let (_screen_width, _screen_height) = self.adb.lock().unwrap().get_cached_screen_size();

        // Use center of screen as reference point
        let center_x = (video_width / 2) as i32;
        let center_y = (video_height / 2) as i32;

        if delta_y > 0.0 {
            // Scroll down
            for mapping in &self.config.mappings {
                if mapping.button == "WHEEL_DOWN" {
                    info!("Mouse wheel DOWN at center ({}, {})", center_x, center_y);
                    if let Some(ref dir) = mapping.dir {
                        let (x1, y1) = (center_x, center_y);
                        let (x2, y2) = match dir.as_str() {
                            "UP" => (center_x, center_y - 300),
                            "DOWN" => (center_x, center_y + 300),
                            _ => (center_x, center_y - 300),
                        };

                        if let Err(e) =
                            self.adb.lock().unwrap().send_input(AdbInputCommand::Swipe {
                                x1,
                                y1,
                                x2,
                                y2,
                                duration: 100,
                            })
                        {
                            error!("Failed to send wheel swipe command: {}", e);
                        }
                    }
                    break;
                }
            }
        } else if delta_y < 0.0 {
            // Scroll up
            for mapping in &self.config.mappings {
                if mapping.button == "WHEEL_UP" {
                    info!("Mouse wheel UP at center ({}, {})", center_x, center_y);
                    if let Some(ref dir) = mapping.dir {
                        let (x1, y1) = (center_x, center_y);
                        let (x2, y2) = match dir.as_str() {
                            "UP" => (center_x, center_y - 300),
                            "DOWN" => (center_x, center_y + 300),
                            _ => (center_x, center_y - 300),
                        };

                        if let Err(e) =
                            self.adb.lock().unwrap().send_input(AdbInputCommand::Swipe {
                                x1,
                                y1,
                                x2,
                                y2,
                                duration: 100,
                            })
                        {
                            error!("Failed to send wheel swipe command: {}", e);
                        }
                    }
                    break;
                }
            }
        }

        Ok(())
    }
}
