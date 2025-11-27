use {
    crate::{
        config::mapping::{AdbAction, MouseButton, WheelDirection},
        controller::adb::AdbShell,
    },
    anyhow::Result,
    tracing::debug,
};

/// Mouse mapping state
pub struct MouseMapper {
    adb_shell: AdbShell,
}

impl MouseMapper {
    /// Create a new mouse mapper
    pub fn new() -> Self {
        Self {
            adb_shell: AdbShell::new(),
        }
    }

    /// Handle mouse button event
    pub fn handle_button_event(
        &self,
        button: MouseButton,
        pressed: bool,
        x: u32,
        y: u32,
    ) -> Result<()> {
        // TODO: handle button hold actions
        if !pressed {
            return Ok(());
        }

        // Execute action based on mapping
        let action = match button {
            MouseButton::Left => AdbAction::Tap { x, y },
            MouseButton::Right => AdbAction::Back,
            MouseButton::Middle => AdbAction::Home,
        };
        self.adb_shell.send_input(&action)?;

        debug!(
            "Mouse button {} (pressed: {}) at ({}, {}) -> {:?}",
            button, pressed, x, y, action
        );

        Ok(())
    }

    /// Handle mouse wheel event
    pub fn handle_wheel_event(&self, x: u32, y: u32, dir: WheelDirection) -> Result<()> {
        if let Some(screen_size) = self.adb_shell.get_screen_size() {
            let action = match dir {
                WheelDirection::Up => AdbAction::Swipe {
                    x1: x,
                    y1: y,
                    x2: x,
                    y2: y.saturating_sub(300),
                    duration: 100,
                },
                WheelDirection::Down => AdbAction::Swipe {
                    x1: x,
                    y1: y,
                    x2: x,
                    y2: (y + 300).min(screen_size.1),
                    duration: 100,
                },
            };
            self.adb_shell.send_input(&action)?;

            debug!("Mouse wheel {:?} at ({}, {}) -> {:?}", dir, x, y, action);
        }

        Ok(())
    }
}
