use {
    crate::{
        config::mapping::{MouseButton, WheelDirection},
        controller::control_sender::ControlSender,
    },
    anyhow::Result,
    parking_lot::Mutex,
    std::time::Instant,
    tracing::{debug, trace},
};

/// Movement threshold to distinguish drag from click (in pixels)
const DRAG_THRESHOLD: f32 = 5.0;
/// Long press duration threshold (in milliseconds)
const LONG_PRESS_DURATION_MS: u128 = 300;
/// Drag update interval (in milliseconds) - send every move event for smooth dragging
const DRAG_UPDATE_INTERVAL_MS: u128 = 8; // ~120fps, matches typical mouse polling rate

/// Mouse button state for tracking press/drag/long-press
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MouseState {
    /// No button pressed
    Idle,
    /// Button pressed at position (x, y) at time
    Pressed { x: u32, y: u32, time: Instant },
    /// Dragging from (x1, y1), currently at (x, y)
    Dragging {
        start_x: u32,
        start_y: u32,
        current_x: u32,
        current_y: u32,
        last_update: Instant,
    },
    /// Long pressing at (x, y) - event already sent
    LongPressing { x: u32, y: u32 },
}

pub struct MouseMapper {
    sender: ControlSender,
    /// Current mouse state for left button
    left_button_state: Mutex<MouseState>,
}

impl MouseMapper {
    /// Create a new mouse mapper
    pub fn new(sender: ControlSender) -> Result<Self> {
        Ok(Self {
            sender,
            left_button_state: Mutex::new(MouseState::Idle),
        })
    }

    /// Update mouse state - call this every frame
    /// Checks for long press timeout and sends drag updates
    pub fn update(&self) -> Result<()> {
        let state = self.left_button_state.lock().clone();

        match state {
            MouseState::Pressed {
                x,
                y,
                time: start_time,
            } => {
                // Check if long press duration exceeded
                let elapsed = start_time.elapsed().as_millis();
                if elapsed >= LONG_PRESS_DURATION_MS {
                    // Long press triggered - don't send any ADB event
                    // Android will detect the sustained touch and trigger long press automatically
                    // The TouchDown is already being held, so Android knows it's a long press
                    debug!("Long press triggered at ({}, {}) [from update]", x, y);

                    // Transition to LongPressing state
                    self.update_button_state(MouseState::LongPressing { x, y });
                }
            }
            MouseState::Dragging {
                current_x,
                current_y,
                last_update,
                ..
            } => {
                // Only send update if enough time passed
                let elapsed = last_update.elapsed().as_millis();
                if elapsed >= DRAG_UPDATE_INTERVAL_MS {
                    // Send TouchMove event for drag updates
                    self.sender.send_touch_move(current_x, current_y)?;
                    debug!(
                        "UPDATE: Drag move to ({}, {}) [elapsed={}ms]",
                        current_x, current_y, elapsed
                    );

                    // Update last_update timestamp
                    self.update_button_state(MouseState::Dragging {
                        start_x: current_x,
                        start_y: current_y,
                        current_x,
                        current_y,
                        last_update: Instant::now(),
                    });
                }
            }
            _ => {}
        }

        Ok(())
    }

    pub fn get_button_state(&self) -> MouseState { self.left_button_state.lock().clone() }

    pub fn update_button_state(&self, new_state: MouseState) {
        *self.left_button_state.lock() = new_state;
    }

    /// Handle mouse button event
    pub fn handle_button_event(
        &self,
        button: MouseButton,
        pressed: bool,
        x: u32,
        y: u32,
    ) -> Result<()> {
        match button {
            MouseButton::Left => {
                if pressed {
                    self.handle_left_button_press(x, y)?;
                } else {
                    self.handle_left_button_release(x, y)?;
                }
            }
            MouseButton::Right if pressed => {
                self.sender.send_key_press(4, 0)?; // KEYCODE_BACK
                debug!("Mouse right button -> Back");
            }
            MouseButton::Middle if pressed => {
                self.sender.send_key_press(3, 0)?; // KEYCODE_HOME
                debug!("Mouse middle button -> Home");
            }
            _ => {}
        }

        Ok(())
    }

    /// Handle left button press
    fn handle_left_button_press(&self, x: u32, y: u32) -> Result<()> {
        // Update state to Pressed
        self.update_button_state(MouseState::Pressed {
            x,
            y,
            time: Instant::now(),
        });

        // Send TouchDown event immediately
        self.sender.send_touch_down(x, y)?;
        trace!("Left button pressed at ({}, {}), sent TouchDown", x, y);
        Ok(())
    }

    /// Handle left button release
    fn handle_left_button_release(&self, x: u32, y: u32) -> Result<()> {
        // Get previous state
        let prev_state = self.get_button_state();
        debug!(
            "Button release at ({}, {}), prev_state={:?}",
            x, y, prev_state
        );

        // Reset state to Idle
        self.update_button_state(MouseState::Idle);

        match prev_state {
            MouseState::Pressed {
                x: start_x,
                y: start_y,
                time: start_time,
            } => {
                let elapsed = start_time.elapsed().as_millis();
                let distance = ((x as f32 - start_x as f32).powi(2)
                    + (y as f32 - start_y as f32).powi(2))
                .sqrt();

                debug!(
                    "Release in Pressed state: distance={:.1}px, elapsed={}ms",
                    distance, elapsed
                );

                // Send TouchUp to complete the touch sequence
                self.sender.send_touch_up(x, y)?;

                if distance >= DRAG_THRESHOLD {
                    // Fast drag that didn't trigger Dragging state transition
                    debug!(
                        "ACTION: Fast drag/flick from ({}, {}) to ({}, {}), distance={:.1}px",
                        start_x, start_y, x, y, distance
                    );
                } else if elapsed >= LONG_PRESS_DURATION_MS {
                    // Long press was already handled in update()
                    debug!(
                        "ACTION: Long press completed at ({}, {}) after {}ms",
                        start_x, start_y, elapsed
                    );
                } else {
                    // Normal tap
                    debug!(
                        "ACTION: Tap at ({}, {}) after {}ms",
                        start_x, start_y, elapsed
                    );
                }
            }
            MouseState::Dragging {
                current_x,
                current_y,
                ..
            } => {
                // Dragging ended - send TouchUp to complete the touch sequence
                self.sender.send_touch_up(x, y)?;
                debug!(
                    "ACTION: Drag completed, ended at ({}, {})",
                    current_x, current_y
                );
            }
            MouseState::LongPressing { x: lp_x, y: lp_y } => {
                // Long press event was already sent, send TouchUp to complete
                self.sender.send_touch_up(x, y)?;
                debug!("ACTION: Long press released at ({}, {})", lp_x, lp_y);
            }
            MouseState::Idle => {
                // Spurious release, but still send TouchUp to be safe
                self.sender.send_touch_up(x, y)?;
                debug!("Spurious left button release at ({}, {})", x, y);
            }
        }

        Ok(())
    }

    /// Handle mouse move event (for drag detection)
    pub fn handle_move_event(&self, x: u32, y: u32) -> Result<()> {
        let state = self.get_button_state();

        match state {
            MouseState::Pressed {
                x: start_x,
                y: start_y,
                time: _start_time,
            } => {
                let distance = ((x as f32 - start_x as f32).powi(2)
                    + (y as f32 - start_y as f32).powi(2))
                .sqrt();

                trace!(
                    "Move during Pressed: from ({}, {}) to ({}, {}), distance={:.1}px",
                    start_x, start_y, x, y, distance
                );

                if distance >= DRAG_THRESHOLD {
                    // Transition to dragging state
                    // Don't send initial swipe here - let update() handle it
                    self.update_button_state(MouseState::Dragging {
                        start_x,
                        start_y,
                        current_x: x,
                        current_y: y,
                        last_update: Instant::now(),
                    });
                    debug!(
                        "STATE TRANSITION: Pressed -> Dragging (distance={:.1}px >= {}px)",
                        distance, DRAG_THRESHOLD
                    );
                }
                // Note: Long press is now checked in update() method
            }
            MouseState::Dragging {
                start_x,
                start_y,
                last_update,
                ..
            } => {
                // Send MOVE event immediately if enough time passed (throttling)
                let elapsed = last_update.elapsed().as_millis();
                if elapsed >= DRAG_UPDATE_INTERVAL_MS {
                    self.sender.send_touch_move(x, y)?;
                    trace!("Drag move to ({}, {}) [elapsed={}ms]", x, y, elapsed);

                    // Update state with new timestamp
                    self.update_button_state(MouseState::Dragging {
                        start_x,
                        start_y,
                        current_x: x,
                        current_y: y,
                        last_update: Instant::now(),
                    });
                } else {
                    // Just update position, don't send event yet (throttling)
                    self.update_button_state(MouseState::Dragging {
                        start_x,
                        start_y,
                        current_x: x,
                        current_y: y,
                        last_update,
                    });
                }
            }
            MouseState::LongPressing { x: lp_x, y: lp_y } => {
                // Long press detected, start dragging
                debug!(
                    "STATE TRANSITION: LongPressing -> Dragging (moving from ({}, {}) to ({}, {}))",
                    lp_x, lp_y, x, y
                );

                // Transition to dragging state without sending TouchDown
                // (Long press Swipe event is already being processed by Android)
                self.update_button_state(MouseState::Dragging {
                    start_x: lp_x,
                    start_y: lp_y,
                    current_x: x,
                    current_y: y,
                    last_update: Instant::now(),
                });
                debug!(
                    "Drag started from long press: ({}, {}) -> ({}, {})",
                    lp_x, lp_y, x, y
                );
            }
            _ => {
                // Ignore movement in other states (Pressed is handled separately, Idle is
                // impossible here)
            }
        }

        Ok(())
    }

    /// Handle mouse wheel event
    pub fn handle_wheel_event(&self, x: u32, y: u32, dir: &WheelDirection) -> Result<()> {
        let (hscroll, vscroll) = match dir {
            WheelDirection::Up => (0.0, -5.0),
            WheelDirection::Down => (0.0, 5.0),
        };
        self.sender.send_scroll(x, y, hscroll, vscroll)?;

        debug!("Mouse wheel {:?}", dir);

        Ok(())
    }
}
