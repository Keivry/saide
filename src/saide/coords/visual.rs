//! Visual (UI/screen) coordinate system implementation

use {
    super::{
        mapping::MappingCoordSys,
        scrcpy::ScrcpyCoordSys,
        types::{MappingPos, ScrcpyPos, VisualPos},
    },
    eframe::egui::Rect,
};

/// Visual coordinate system (UI/screen space)
///
/// Coordinates relative to egui window/screen.
/// - rotation: User manual rotation applied to video display (0-3, clockwise 90°)
///
/// Note: video_rect is NOT cached here because it changes every frame (window resize, first frame,
/// etc.) Instead, video_rect is passed as a parameter to conversion methods.
#[derive(Debug, Clone, Copy)]
pub struct VisualCoordSys {
    pub rotation: u32,
}

impl VisualCoordSys {
    pub fn new(rotation: u32) -> Self {
        Self {
            rotation: rotation % 4,
        }
    }

    pub fn update_rotation(&mut self, rotation: u32) { self.rotation = rotation % 4; }

    /// Convert visual coordinate to ScrcpyCoordSys
    ///
    /// # Parameters
    /// - `pos`: Screen position (VisualPos)
    /// - `rect`: Video display rectangle (from player.video_rect())
    /// - `target`: Target ScrcpyCoordSys
    pub fn to_scrcpy(
        &self,
        pos: &VisualPos,
        rect: &Rect,
        target: &ScrcpyCoordSys,
    ) -> Option<ScrcpyPos> {
        target.from_visual(pos, rect, self.rotation)
    }

    /// Convert from ScrcpyCoordSys to visual coordinate
    ///
    /// # Parameters
    /// - `pos`: ScrcpyPos (x, y) pixel coordinates
    /// - `rect`: Video display rectangle (from player.video_rect())
    /// - `source`: Source ScrcpyCoordSys
    pub fn from_scrcpy(&self, pos: &ScrcpyPos, rect: &Rect, source: &ScrcpyCoordSys) -> VisualPos {
        source.to_visual(pos, rect, self.rotation)
    }

    /// Convert to MappingCoordSys (via ScrcpyCoordSys)
    ///
    /// # Parameters
    /// - `pos`: Screen position (VisualPos)
    /// - `rect`: Video display rectangle (from player.video_rect())
    /// - `scrcpy_sys`: Intermediate ScrcpyCoordSys
    /// - `target`: Target MappingCoordSys
    pub fn to_mapping(
        &self,
        pos: &VisualPos,
        rect: &Rect,
        scrcpy_sys: &ScrcpyCoordSys,
        target: &MappingCoordSys,
    ) -> Option<MappingPos> {
        let scrcpy_pos = self.to_scrcpy(pos, rect, scrcpy_sys)?;
        Some(target.from_scrcpy(&scrcpy_pos, scrcpy_sys))
    }

    /// Convert from MappingCoordSys to visual coordinate (via ScrcpyCoordSys)
    ///
    /// # Parameters
    /// - `pos`: MappingPos (x, y) in 0.0-1.0 range
    /// - `rect`: Video display rectangle (from player.video_rect())
    /// - `scrcpy_sys`: Intermediate ScrcpyCoordSys
    /// - `source`: Source MappingCoordSys
    pub fn from_mapping(
        &self,
        pos: &MappingPos,
        rect: &Rect,
        scrcpy_sys: &ScrcpyCoordSys,
        source: &MappingCoordSys,
    ) -> VisualPos {
        let scrcpy_pos = source.to_scrcpy(pos, scrcpy_sys);
        self.from_scrcpy(&scrcpy_pos, rect, scrcpy_sys)
    }
}
