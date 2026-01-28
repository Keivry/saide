//! Mouse input mapper for Android touch events
//!
//! This module maps mouse input events (move, button, wheel) to Android touch events
//! using the scrcpy control protocol. It handles click, drag, long-press, and
//! wheel events, translating them into appropriate touch sequences.
//!
//! **Performance Note**: Uses lock-free atomics for 240 Hz update loop.
//! State is packed into `AtomicU64` bitfields to avoid torn reads and mutex contention.

use {
    crate::{
        config::{
            InputConfig,
            mapping::{MouseButton, WheelDirection},
        },
        controller::control_sender::ControlSender,
        error::Result,
    },
    std::{
        sync::atomic::{AtomicU64, Ordering},
        time::Instant,
    },
    tracing::{debug, trace},
};

// ============================================================================
// Atomic State Encoding (Lock-Free Mouse State)
// ============================================================================

/// Mouse state kind (2 bits: 0-3)
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MouseStateKind {
    Idle = 0,
    Pressed = 1,
    Dragging = 2,
    LongPressing = 3,
}

/// Pack (state, x, y) into AtomicU64
/// - bits 0-1:   state (0=Idle, 1=Pressed, 2=Dragging, 3=LongPressing)
/// - bits 2-22:  x coordinate (21 bits, max 2,097,151 - supports displays beyond 8K)
/// - bits 23-43: y coordinate (21 bits, max 2,097,151)
/// - bits 44-63: reserved (20 bits)
const fn pack_snapshot(state: MouseStateKind, x: u32, y: u32) -> u64 {
    let state_bits = (state as u64) & 0x3;
    let x_bits = ((x as u64) & 0x1F_FFFF) << 2; // 21 bits at position 2
    let y_bits = ((y as u64) & 0x1F_FFFF) << 23; // 21 bits at position 23
    state_bits | x_bits | y_bits
}

/// Unpack AtomicU64 snapshot into (state, x, y)
fn unpack_snapshot(packed: u64) -> (MouseStateKind, u32, u32) {
    let state_raw = (packed & 0x3) as u8;
    let state = match state_raw {
        0 => MouseStateKind::Idle,
        1 => MouseStateKind::Pressed,
        2 => MouseStateKind::Dragging,
        3 => MouseStateKind::LongPressing,
        _ => unreachable!("State masked to 2 bits"),
    };
    let x = ((packed >> 2) & 0x1F_FFFF) as u32;
    let y = ((packed >> 23) & 0x1F_FFFF) as u32;
    (state, x, y)
}

/// Pack (start_x, start_y) into AtomicU64
/// - bits 0-20:  start_x (21 bits)
/// - bits 21-41: start_y (21 bits)
/// - bits 42-63: reserved (22 bits)
const fn pack_start_coords(start_x: u32, start_y: u32) -> u64 {
    let x_bits = (start_x as u64) & 0x1F_FFFF;
    let y_bits = ((start_y as u64) & 0x1F_FFFF) << 21;
    x_bits | y_bits
}

/// Unpack start_coords AtomicU64 into (start_x, start_y)
const fn unpack_start_coords(packed: u64) -> (u32, u32) {
    let start_x = (packed & 0x1F_FFFF) as u32;
    let start_y = ((packed >> 21) & 0x1F_FFFF) as u32;
    (start_x, start_y)
}

/// Get process start time (shared between instant_to_ms and ms_to_instant)
fn get_process_start() -> &'static Instant {
    static PROCESS_START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    PROCESS_START.get_or_init(Instant::now)
}

/// Convert Instant to milliseconds since process start (monotonic)
fn instant_to_ms(instant: Instant) -> u64 {
    // SAFETY: Instant is opaque, we store elapsed time since process start
    // This avoids platform-specific epoch conversions (Oracle warning)
    instant.duration_since(*get_process_start()).as_millis() as u64
}

/// Convert milliseconds back to Instant
fn ms_to_instant(ms: u64) -> Instant { *get_process_start() + std::time::Duration::from_millis(ms) }

// ============================================================================
// Snapshot Types (for returning consistent state)
// ============================================================================

/// Consistent snapshot of mouse state (no torn reads)
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
    /// Packed snapshot: (state, x, y) for atomic reads
    snapshot: AtomicU64,
    /// Dragging start coordinates (start_x, start_y)
    start_coords: AtomicU64,
    /// Timestamp in milliseconds (for Pressed/Dragging states)
    timestamp_ms: AtomicU64,
    config: InputConfig,
}

impl MouseMapper {
    pub fn new(sender: ControlSender, config: InputConfig) -> Self {
        Self {
            sender,
            snapshot: AtomicU64::new(pack_snapshot(MouseStateKind::Idle, 0, 0)),
            start_coords: AtomicU64::new(0),
            timestamp_ms: AtomicU64::new(0),
            config,
        }
    }

    pub fn update(&self) -> Result<()> {
        let (state, x, y) = unpack_snapshot(self.snapshot.load(Ordering::Acquire));
        let timestamp = self.timestamp_ms.load(Ordering::Acquire);

        match state {
            MouseStateKind::Pressed => {
                let elapsed_ms = instant_to_ms(Instant::now()).saturating_sub(timestamp);
                if elapsed_ms >= self.config.long_press_ms {
                    debug!("Long press triggered at ({}, {}) [from update]", x, y);
                    self.snapshot.store(
                        pack_snapshot(MouseStateKind::LongPressing, x, y),
                        Ordering::Release,
                    );
                }
            }
            MouseStateKind::Dragging => {
                let elapsed_ms = instant_to_ms(Instant::now()).saturating_sub(timestamp);
                if elapsed_ms >= self.config.drag_interval_ms {
                    self.sender.send_touch_move(x, y)?;
                    debug!(
                        "UPDATE: Drag move to ({}, {}) [elapsed={}ms]",
                        x, y, elapsed_ms
                    );
                    self.timestamp_ms
                        .store(instant_to_ms(Instant::now()), Ordering::Release);
                }
            }
            _ => {}
        }

        Ok(())
    }

    pub fn get_button_state(&self) -> MouseState {
        let (state, x, y) = unpack_snapshot(self.snapshot.load(Ordering::Acquire));
        let timestamp = self.timestamp_ms.load(Ordering::Acquire);
        let (start_x, start_y) = unpack_start_coords(self.start_coords.load(Ordering::Relaxed));

        match state {
            MouseStateKind::Idle => MouseState::Idle,
            MouseStateKind::Pressed => MouseState::Pressed {
                x,
                y,
                time: ms_to_instant(timestamp),
            },
            MouseStateKind::Dragging => MouseState::Dragging {
                start_x,
                start_y,
                current_x: x,
                current_y: y,
                last_update: ms_to_instant(timestamp),
            },
            MouseStateKind::LongPressing => MouseState::LongPressing { x, y },
        }
    }

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

    fn handle_left_button_press(&self, x: u32, y: u32) -> Result<()> {
        let now = instant_to_ms(Instant::now());
        self.timestamp_ms.store(now, Ordering::Relaxed);
        self.snapshot.store(
            pack_snapshot(MouseStateKind::Pressed, x, y),
            Ordering::Release,
        );

        self.sender.send_touch_down(x, y)?;
        trace!("Left button pressed at ({}, {}), sent TouchDown", x, y);
        Ok(())
    }

    fn handle_left_button_release(&self, x: u32, y: u32) -> Result<()> {
        let prev_state = self.get_button_state();
        debug!(
            "Button release at ({}, {}), prev_state={:?}",
            x, y, prev_state
        );

        self.snapshot
            .store(pack_snapshot(MouseStateKind::Idle, 0, 0), Ordering::Release);

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

                self.sender.send_touch_up(x, y)?;

                if distance >= self.config.drag_threshold_px {
                    debug!(
                        "ACTION: Fast drag/flick from ({}, {}) to ({}, {}), distance={:.1}px",
                        start_x, start_y, x, y, distance
                    );
                } else if elapsed >= self.config.long_press_ms as u128 {
                    debug!(
                        "ACTION: Long press completed at ({}, {}) after {}ms",
                        start_x, start_y, elapsed
                    );
                } else {
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
                self.sender.send_touch_up(x, y)?;
                debug!(
                    "ACTION: Drag completed, ended at ({}, {})",
                    current_x, current_y
                );
            }
            MouseState::LongPressing { x: lp_x, y: lp_y } => {
                self.sender.send_touch_up(x, y)?;
                debug!("ACTION: Long press released at ({}, {})", lp_x, lp_y);
            }
            MouseState::Idle => {
                self.sender.send_touch_up(x, y)?;
                debug!("Spurious left button release at ({}, {})", x, y);
            }
        }

        Ok(())
    }

    pub fn handle_move_event(&self, x: u32, y: u32) -> Result<()> {
        let (state, current_x, current_y) = unpack_snapshot(self.snapshot.load(Ordering::Acquire));

        match state {
            MouseStateKind::Pressed => {
                let distance = ((x as f32 - current_x as f32).powi(2)
                    + (y as f32 - current_y as f32).powi(2))
                .sqrt();

                trace!(
                    "Move during Pressed: from ({}, {}) to ({}, {}), distance={:.1}px",
                    current_x, current_y, x, y, distance
                );

                if distance >= self.config.drag_threshold_px {
                    self.start_coords
                        .store(pack_start_coords(current_x, current_y), Ordering::Release);
                    self.timestamp_ms
                        .store(instant_to_ms(Instant::now()), Ordering::Release);
                    self.snapshot.store(
                        pack_snapshot(MouseStateKind::Dragging, x, y),
                        Ordering::Release,
                    );
                    debug!(
                        "STATE TRANSITION: Pressed -> Dragging (distance={:.1}px >= {}px)",
                        distance, self.config.drag_threshold_px
                    );
                }
            }
            MouseStateKind::Dragging => {
                let last_update_ms = self.timestamp_ms.load(Ordering::Acquire);
                let elapsed_ms = instant_to_ms(Instant::now()).saturating_sub(last_update_ms);

                if elapsed_ms >= self.config.drag_interval_ms {
                    self.sender.send_touch_move(x, y)?;
                    trace!("Drag move to ({}, {}) [elapsed={}ms]", x, y, elapsed_ms);

                    self.timestamp_ms
                        .store(instant_to_ms(Instant::now()), Ordering::Release);
                    self.snapshot.store(
                        pack_snapshot(MouseStateKind::Dragging, x, y),
                        Ordering::Release,
                    );
                } else {
                    self.snapshot.store(
                        pack_snapshot(MouseStateKind::Dragging, x, y),
                        Ordering::Release,
                    );
                }
            }
            MouseStateKind::LongPressing => {
                debug!(
                    "STATE TRANSITION: LongPressing -> Dragging (moving from ({}, {}) to ({}, {}))",
                    current_x, current_y, x, y
                );

                self.start_coords
                    .store(pack_start_coords(current_x, current_y), Ordering::Release);
                self.timestamp_ms
                    .store(instant_to_ms(Instant::now()), Ordering::Release);
                self.snapshot.store(
                    pack_snapshot(MouseStateKind::Dragging, x, y),
                    Ordering::Release,
                );
                debug!(
                    "Drag started from long press: ({}, {}) -> ({}, {})",
                    current_x, current_y, x, y
                );
            }
            _ => {}
        }

        Ok(())
    }

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
