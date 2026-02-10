//! Mapping actions that can be triggered by input events
//!
//! This module defines enumerations for mapping actions that can be triggered by key presses or
//! mouse events. These actions include touch events (tap, swipe, drag), scroll events, key events,
//! and special actions like back, home, menu, and power. The module also includes logic to convert
//! mapping actions into scrcpy-specific actions by translating coordinate systems.

use {
    super::{Modifiers, mouse::WheelDirection},
    crate::core::coords::{MappingCoordSys, MappingPos, ScrcpyCoordSys, ScrcpyPos},
    serde::{Deserialize, Serialize},
};

/// Mapping action loaded from config
///
/// Coordinates are stored as:
/// - Percentage (0.0-1.0 f32) for x and y positions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action")]
pub enum MappingAction {
    Tap {
        #[serde(flatten)]
        pos: MappingPos,
    },
    Swipe {
        #[serde(flatten)]
        path: [MappingPos; 2],
        duration: u32,
    },
    /// Touch down event (start of drag)
    TouchDown {
        #[serde(flatten)]
        pos: MappingPos,
    },
    /// Touch move event (during drag)
    TouchMove {
        #[serde(flatten)]
        pos: MappingPos,
    },
    /// Touch up event (end of drag)
    TouchUp {
        #[serde(flatten)]
        pos: MappingPos,
    },
    Scroll {
        #[serde(flatten)]
        pos: MappingPos,
        direction: WheelDirection,
    },
    Key {
        keycode: u8,
    },
    KeyCombo {
        modifiers: Modifiers,
        keycode: u8,
    },
    Text {
        text: String,
    },
    Back,
    Home,
    Menu,
    Power,

    Ignore,
}

/// Scrcpy-specific action derived from MappingAction
///
/// Coordinates are converted to scrcpy coordinate system
/// - Absolute pixel positions based on scrcpy video size
#[derive(Debug, Clone)]
pub enum ScrcpyAction {
    Tap {
        pos: ScrcpyPos,
    },
    Swipe {
        path: [ScrcpyPos; 2],
        duration: u32,
    },
    /// Touch down event (start of drag)
    TouchDown {
        pos: ScrcpyPos,
    },
    /// Touch move event (during drag)
    TouchMove {
        pos: ScrcpyPos,
    },
    /// Touch up event (end of drag)
    TouchUp {
        pos: ScrcpyPos,
    },
    Scroll {
        pos: ScrcpyPos,
        direction: WheelDirection,
    },
    Key {
        keycode: u8,
    },
    KeyCombo {
        modifiers: Modifiers,
        keycode: u8,
    },
    Text {
        text: String,
    },
    Back,
    Home,
    Menu,
    Power,

    Ignore,
}

impl ScrcpyAction {
    /// Convert a MappingAction to a ScrcpyAction using the provided coordinate systems
    pub fn from_mapping_action(
        action: &MappingAction,
        scrcpy_coords: &ScrcpyCoordSys,
        mapping_coords: &MappingCoordSys,
    ) -> Self {
        match action {
            MappingAction::Tap { pos } => ScrcpyAction::Tap {
                pos: mapping_coords.to_scrcpy(pos, scrcpy_coords),
            },
            MappingAction::Swipe { path, duration } => ScrcpyAction::Swipe {
                path: [
                    mapping_coords.to_scrcpy(&path[0], scrcpy_coords),
                    mapping_coords.to_scrcpy(&path[1], scrcpy_coords),
                ],
                duration: *duration,
            },
            MappingAction::TouchDown { pos } => ScrcpyAction::TouchDown {
                pos: mapping_coords.to_scrcpy(pos, scrcpy_coords),
            },
            MappingAction::TouchMove { pos } => ScrcpyAction::TouchMove {
                pos: mapping_coords.to_scrcpy(pos, scrcpy_coords),
            },
            MappingAction::TouchUp { pos } => ScrcpyAction::TouchUp {
                pos: mapping_coords.to_scrcpy(pos, scrcpy_coords),
            },
            MappingAction::Scroll { pos, direction } => ScrcpyAction::Scroll {
                pos: mapping_coords.to_scrcpy(pos, scrcpy_coords),
                direction: direction.clone(),
            },
            MappingAction::Key { keycode } => ScrcpyAction::Key { keycode: *keycode },
            MappingAction::KeyCombo { modifiers, keycode } => ScrcpyAction::KeyCombo {
                modifiers: *modifiers,
                keycode: *keycode,
            },
            MappingAction::Text { text } => ScrcpyAction::Text { text: text.clone() },
            MappingAction::Back => ScrcpyAction::Back,
            MappingAction::Home => ScrcpyAction::Home,
            MappingAction::Menu => ScrcpyAction::Menu,
            MappingAction::Power => ScrcpyAction::Power,
            MappingAction::Ignore => ScrcpyAction::Ignore,
        }
    }
}
