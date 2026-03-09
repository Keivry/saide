//! Mouse input mapping configuration
//!
//! This module defines the structures and enums related to mouse input mapping, including mouse
//! buttons and scroll wheel directions.
use {
    egui::PointerButton,
    serde::{Deserialize, Serialize},
};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Extra1,
    Extra2,
}

/// Scroll wheel direction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WheelDirection {
    Up,
    Down,
}

// Convert egui PointerButton to MouseButton
impl From<PointerButton> for MouseButton {
    fn from(button: PointerButton) -> Self {
        match button {
            PointerButton::Primary => MouseButton::Left,
            PointerButton::Secondary => MouseButton::Right,
            PointerButton::Middle => MouseButton::Middle,
            PointerButton::Extra1 => MouseButton::Extra1,
            PointerButton::Extra2 => MouseButton::Extra2,
        }
    }
}
