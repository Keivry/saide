//! Coordinate type definitions for SAide

use {
    eframe::egui::Pos2,
    serde::{Deserialize, Serialize},
};

/// Mapping position in normalized (0.0-1.0) coordinate system
///
/// Used for storing mapping points in config files.
/// Coordinates are clamped to 0.0-1.0 range.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MappingPos {
    pub x: f32,
    pub y: f32,
}

impl MappingPos {
    pub fn new(x: f32, y: f32) -> Self {
        Self {
            x: x.clamp(0.0, 1.0),
            y: y.clamp(0.0, 1.0),
        }
    }
}

impl From<(f32, f32)> for MappingPos {
    fn from(pos: (f32, f32)) -> Self { Self::new(pos.0, pos.1) }
}

impl From<MappingPos> for (f32, f32) {
    fn from(pos: MappingPos) -> Self { (pos.x, pos.y) }
}

/// Scrcpy position in pixel coordinate system
///
/// Used for sending control messages to scrcpy-server.
/// Coordinates are in pixel units.
#[derive(Debug, Clone, Copy)]
pub struct ScrcpyPos {
    pub x: u32,
    pub y: u32,
}

impl ScrcpyPos {
    pub fn new(x: u32, y: u32) -> Self { Self { x, y } }
}

impl From<(u32, u32)> for ScrcpyPos {
    fn from(pos: (u32, u32)) -> Self { Self::new(pos.0, pos.1) }
}

impl From<ScrcpyPos> for (u32, u32) {
    fn from(pos: ScrcpyPos) -> Self { (pos.x, pos.y) }
}

/// Visual position in screen/UI coordinate system
///
/// Used for displaying positions in the egui window.
/// Coordinates are in egui::Pos2 units.
pub type VisualPos = Pos2;
