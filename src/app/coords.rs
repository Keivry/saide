/// Coordinate system transformations for SAide
///
/// This module implements 3 coordinate systems:
/// 1. MappingCoordSys: Normalized (0.0-1.0) coordinate system bound to device orientation,
///    stored in config files
/// 2. VisualCoordSys: Screen/UI coordinate system relative to video display rect
/// 3. ScrcpyCoordSys: Video pixel coordinate system for scrcpy control protocol
///
/// Transformation chain:
/// - Config loading: MappingCoordSys → cache as ScrcpyCoordSys when profile activated
/// - UI display: MappingCoordSys ↔ ScrcpyCoordSys ↔ VisualCoordSys
/// - Input events: VisualCoordSys → ScrcpyCoordSys (for control) or VisualCoordSys →
///   MappingCoordSys (for config editing)
use eframe::egui::{Pos2, Rect};

/// Mapping coordinate system (0.0-1.0 normalized, stored in config)
///
/// Bound to device orientation at time of mapping creation.
/// - device_orientation: 0=0°, 1=90°CCW, 2=180°, 3=270°CCW (Android Display Rotation)
///
/// Coordinates are percentage values (0.0-1.0) relative to device screen at that orientation.
///
/// Example: If device is in landscape (orientation=1), x=0.9, y=0.5 means
/// 90% from left edge, 50% from top edge of the landscape screen.
#[derive(Debug, Clone, Copy)]
pub struct MappingCoordSys {
    pub device_orientation: u32,
}

impl MappingCoordSys {
    pub fn new(device_orientation: u32) -> Self {
        Self {
            device_orientation: device_orientation % 4,
        }
    }

    /// Convert mapping coordinate (0.0-1.0) to ScrcpyCoordSys
    ///
    /// # Parameters
    /// - `pos`: (x, y) in 0.0-1.0 range
    /// - `target`: Target ScrcpyCoordSys
    ///
    /// # Notes
    /// - When capture_orientation is None, video resolution matches device orientation, so no
    ///   rotation transform needed
    /// - When capture_orientation is Some(orientation), video is locked to that orientation, must
    ///   rotate coords from device_orientation to capture_orientation
    pub fn to_scrcpy(&self, pos: (f32, f32), target: &ScrcpyCoordSys) -> (u32, u32) {
        let (x, y) = (pos.0.clamp(0.0, 1.0), pos.1.clamp(0.0, 1.0));

        let (x_px, y_px) = if let Some(capture_orient) = target.capture_orientation {
            // Capture orientation locked - need rotation transform
            // Different orientation - rotate coordinates
            // Transform from device_orientation to capture_orientation
            //
            // Device orientation is CW (Android): 0=0°, 1=90°CW, 2=180°, 3=270°CW
            // Capture orientation is also CCW: 0=0°, 1=90°CCW, 2=180°, 3=270°CCW
            // Rotation delta: (device_orientation + capture_orient) % 4

            let rotation = (self.device_orientation + capture_orient) % 4;
            match rotation {
                0 => {
                    // No rotation
                    (
                        x * target.video_width as f32,
                        y * target.video_height as f32,
                    )
                }
                1 => {
                    // 90° CW: (x, y) -> (1-y, x)
                    // Swap dimensions
                    (
                        (1.0 - y) * target.video_width as f32,
                        x * target.video_height as f32,
                    )
                }
                2 => {
                    // 180°: (x, y) -> (1-x, 1-y)
                    (
                        (1.0 - x) * target.video_width as f32,
                        (1.0 - y) * target.video_height as f32,
                    )
                }
                3 => {
                    // 270° CW = 90° CCW: (x, y) -> (y, 1-x)
                    // Swap dimensions
                    (
                        y * target.video_width as f32,
                        (1.0 - x) * target.video_height as f32,
                    )
                }
                _ => unreachable!(),
            }
        } else {
            // Capture not locked - video follows device orientation
            // Video resolution already matches device orientation, direct scale
            (
                x * target.video_width as f32,
                y * target.video_height as f32,
            )
        };

        (x_px as u32, y_px as u32)
    }

    /// Convert from ScrcpyCoordSys to mapping coordinate (0.0-1.0)
    ///
    /// # Parameters
    /// - `pos`: (x, y) pixel coordinates
    /// - `source`: Source ScrcpyCoordSys
    pub fn from_scrcpy(&self, pos: (u32, u32), source: &ScrcpyCoordSys) -> (f32, f32) {
        let percent_x = pos.0 as f32 / source.video_width as f32;
        let percent_y = pos.1 as f32 / source.video_height as f32;

        let (x, y) = if let Some(capture_orient) = source.capture_orientation {
            let rotation = (self.device_orientation + capture_orient) % 4;
            match rotation {
                0 => {
                    // No rotation
                    (percent_x, percent_y)
                }
                1 => {
                    // 90° CCW: (x, y) -> (y, 1-x)
                    (percent_y, 1.0 - percent_x)
                }
                2 => {
                    // 180°: (x, y) -> (1-x, 1-y)
                    (1.0 - percent_x, 1.0 - percent_y)
                }
                3 => {
                    // 270° CCW: (x, y) -> (1-y, x)
                    (1.0 - percent_y, percent_x)
                }
                _ => unreachable!(),
            }
        } else {
            // Capture not locked - video follows device
            (percent_x, percent_y)
        };

        (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0))
    }
}

/// Scrcpy control protocol coordinate system (pixel-based, video resolution)
///
/// These coordinates are sent to scrcpy-server in control messages.
/// - video_width/height: Video capture resolution
/// - capture_orientation: Scrcpy capture orientation lock (0-3, CCW)
///   - None: Video follows device rotation
///   - Some(0): Locked to portrait 0°
///   - Some(1): Locked to landscape 90°CCW
///   - Some(2): Locked to portrait 180°
///   - Some(3): Locked to landscape 270°CCW
#[derive(Debug, Clone, Copy)]
pub struct ScrcpyCoordSys {
    pub capture_orientation: Option<u32>,
    pub video_width: u16,
    pub video_height: u16,
}

impl ScrcpyCoordSys {
    pub fn new(video_width: u16, video_height: u16, capture_orientation: Option<u32>) -> Self {
        Self {
            capture_orientation: capture_orientation.map(|o| o % 4),
            video_width,
            video_height,
        }
    }

    /// Convert scrcpy coordinate to VisualCoordSys
    ///
    /// # Parameters
    /// - `pos`: (x, y) pixel coordinates in video frame
    /// - `target`: Target VisualCoordSys
    pub fn to_visual(&self, pos: (u32, u32), rect: Rect, rotation: u32) -> Option<Pos2> {
        // Video coords are in video frame space
        // Need to transform through user rotation to display space

        let (video_w, video_h) = if rotation & 1 == 0 {
            (rect.width(), rect.height())
        } else {
            // 90° or 270° rotation swaps dimensions
            (rect.height(), rect.width())
        };

        // Normalize to video frame coordinates
        let x_norm = pos.0 as f32 / self.video_width as f32;
        let y_norm = pos.1 as f32 / self.video_height as f32;

        // Apply user rotation (clockwise)
        let (rel_x, rel_y) = match rotation % 4 {
            0 => (x_norm * video_w, y_norm * video_h),
            1 => {
                // 90° CW: (x, y) -> (h - y, x)
                (video_h - y_norm * video_h, x_norm * video_w)
            }
            2 => {
                // 180°: (x, y) -> (w - x, h - y)
                (video_w - x_norm * video_w, video_h - y_norm * video_h)
            }
            3 => {
                // 270° CW: (x, y) -> (y, w - x)
                (y_norm * video_h, video_w - x_norm * video_w)
            }
            _ => unreachable!(),
        };

        Some(Pos2::new(rect.left() + rel_x, rect.top() + rel_y))
    }

    /// Convert from visual coordinates to scrcpy coordinate
    ///
    /// # Parameters
    /// - `pos`: Screen position (egui::Pos2)
    /// - `rect`: Video display rectangle
    /// - `rotation`: User manual rotation (0-3, clockwise 90°)
    pub fn from_visual(&self, pos: Pos2, rect: Rect, rotation: u32) -> Option<(u32, u32)> {
        // Check if position is within rect
        if !rect.contains(pos) {
            return None;
        }

        // Get relative position in display rect
        let rel_x = pos.x - rect.left();
        let rel_y = pos.y - rect.top();

        let display_width = rect.width();
        let display_height = rect.height();

        // Inverse apply user rotation to get video frame coordinates
        let (video_x, video_y, video_w, video_h) = match rotation % 4 {
            0 => {
                // No rotation
                (rel_x, rel_y, display_width, display_height)
            }
            1 => {
                // 90° CW rotation
                // Inverse: (x', y') -> (y', w - x')
                (rel_y, display_width - rel_x, display_height, display_width)
            }
            2 => {
                // 180° rotation
                // Inverse: (x', y') -> (w - x', h - y')
                (
                    display_width - rel_x,
                    display_height - rel_y,
                    display_width,
                    display_height,
                )
            }
            3 => {
                // 270° CW rotation
                // Inverse: (x', y') -> (h - y', x')
                (display_height - rel_y, rel_x, display_height, display_width)
            }
            _ => unreachable!(),
        };

        // Normalize and scale to video resolution
        let x_norm = video_x / video_w;
        let y_norm = video_y / video_h;

        Some((
            (x_norm * self.video_width as f32) as u32,
            (y_norm * self.video_height as f32) as u32,
        ))
    }

    /// Convert to MappingCoordSys
    ///
    /// # Parameters
    /// - `pos`: (x, y) pixel coordinates
    /// - `target`: Target MappingCoordSys
    pub fn to_mapping(&self, pos: (u32, u32), target: &MappingCoordSys) -> (f32, f32) {
        target.from_scrcpy(pos, self)
    }

    /// Convert from MappingCoordSys
    ///
    /// # Parameters
    /// - `pos`: (x, y) in 0.0-1.0 range
    /// - `source`: Source MappingCoordSys
    pub fn from_mapping(&self, pos: (f32, f32), source: &MappingCoordSys) -> (u32, u32) {
        source.to_scrcpy(pos, self)
    }
}

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

    /// Convert visual coordinate to ScrcpyCoordSys
    ///
    /// # Parameters
    /// - `pos`: Screen position (egui::Pos2)
    /// - `rect`: Video display rectangle (from player.video_rect())
    /// - `target`: Target ScrcpyCoordSys
    pub fn to_scrcpy(&self, pos: Pos2, rect: Rect, target: &ScrcpyCoordSys) -> Option<(u32, u32)> {
        target.from_visual(pos, rect, self.rotation)
    }

    /// Convert from ScrcpyCoordSys to visual coordinate
    ///
    /// # Parameters
    /// - `pos`: (x, y) pixel coordinates
    /// - `rect`: Video display rectangle (from player.video_rect())
    /// - `source`: Source ScrcpyCoordSys
    pub fn from_scrcpy(
        &self,
        pos: (u32, u32),
        rect: Rect,
        source: &ScrcpyCoordSys,
    ) -> Option<Pos2> {
        source.to_visual(pos, rect, self.rotation)
    }

    /// Convert to MappingCoordSys (via ScrcpyCoordSys)
    ///
    /// # Parameters
    /// - `pos`: Screen position (egui::Pos2)
    /// - `rect`: Video display rectangle (from player.video_rect())
    /// - `scrcpy_sys`: Intermediate ScrcpyCoordSys
    /// - `target`: Target MappingCoordSys
    pub fn to_mapping(
        &self,
        pos: Pos2,
        rect: Rect,
        scrcpy_sys: &ScrcpyCoordSys,
        target: &MappingCoordSys,
    ) -> Option<(f32, f32)> {
        let scrcpy_pos = self.to_scrcpy(pos, rect, scrcpy_sys)?;
        Some(target.from_scrcpy(scrcpy_pos, scrcpy_sys))
    }

    /// Convert from MappingCoordSys to visual coordinate (via ScrcpyCoordSys)
    ///
    /// # Parameters
    /// - `pos`: (x, y) in 0.0-1.0 range
    /// - `rect`: Video display rectangle (from player.video_rect())
    /// - `scrcpy_sys`: Intermediate ScrcpyCoordSys
    /// - `source`: Source MappingCoordSys
    pub fn from_mapping(
        &self,
        pos: (f32, f32),
        rect: Rect,
        scrcpy_sys: &ScrcpyCoordSys,
        source: &MappingCoordSys,
    ) -> Option<Pos2> {
        let scrcpy_pos = source.to_scrcpy(pos, scrcpy_sys);
        self.from_scrcpy(scrcpy_pos, rect, scrcpy_sys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mapping_0_to_scrcpy_no_capture_lock() {
        // Portrait device, no capture lock
        let mapping_sys = MappingCoordSys::new(0);
        let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, None);

        let scrcpy_pos = mapping_sys.to_scrcpy((0.2, 0.4), &scrcpy_sys);

        // Portrait device: direct scale
        assert_eq!(scrcpy_pos.0, 216);
        assert_eq!(scrcpy_pos.1, 960);
    }

    #[test]
    fn test_mapping_1_to_scrcpy_no_capture_lock() {
        // Landscape device, no capture lock
        let mapping_sys = MappingCoordSys::new(1);
        let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, None);

        let scrcpy_pos = mapping_sys.to_scrcpy((0.2, 0.4), &scrcpy_sys);

        // Landscape device: direct scale
        assert_eq!(scrcpy_pos.0, 480);
        assert_eq!(scrcpy_pos.1, 432);
    }

    #[test]
    fn test_mapping_2_to_scrcpy_no_capture_lock() {
        // Portrait upside-down device, no capture lock
        let mapping_sys = MappingCoordSys::new(2);
        let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, None);

        let scrcpy_pos = mapping_sys.to_scrcpy((0.2, 0.4), &scrcpy_sys);

        // Portrait upside-down device: direct scale
        assert_eq!(scrcpy_pos.0, 216);
        assert_eq!(scrcpy_pos.1, 960);
    }

    #[test]
    fn test_mapping_3_to_scrcpy_no_capture_lock() {
        // Landscape upside-down device, no capture lock
        let mapping_sys = MappingCoordSys::new(3);
        let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, None);

        let scrcpy_pos = mapping_sys.to_scrcpy((0.2, 0.4), &scrcpy_sys);

        // Landscape upside-down device: direct scale
        assert_eq!(scrcpy_pos.0, 480);
        assert_eq!(scrcpy_pos.1, 432);
    }

    #[test]
    fn test_mapping_0_to_scrcpy_capture_lock_0() {
        // Portrait device (orient=0), capture locked to portrait (orient=0)
        let mapping_sys = MappingCoordSys::new(0);
        let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(0));

        let scrcpy_pos = mapping_sys.to_scrcpy((0.2, 0.4), &scrcpy_sys);

        // No rotation: (0.2, 0.4) -> (0.2, 0.4)
        assert_eq!(scrcpy_pos.0, 216); // 0.2 * 1080
        assert_eq!(scrcpy_pos.1, 960); // 0.4 * 2400
    }

    #[test]
    fn test_mapping_1_to_scrcpy_capture_lock_0() {
        // Landscape device (orient=1), capture locked to portrait (orient=0)
        let mapping_sys = MappingCoordSys::new(1);
        let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(0));

        let scrcpy_pos = mapping_sys.to_scrcpy((0.2, 0.4), &scrcpy_sys);

        // 90° CW rotation: (0.2, 0.4) -> (1-0.4, 0.2) = (0.6, 0.2)
        assert_eq!(scrcpy_pos.0, 648); // 0.6 * 1080
        assert_eq!(scrcpy_pos.1, 480); // 0.2 * 2400
    }

    #[test]
    fn test_mapping_2_to_scrcpy_capture_lock_0() {
        // Portrait upside-down device (orient=2), capture locked to portrait (orient=0)
        let mapping_sys = MappingCoordSys::new(2);
        let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(0));

        let scrcpy_pos = mapping_sys.to_scrcpy((0.2, 0.4), &scrcpy_sys);

        // 180° rotation: (0.2, 0.4) -> (1-0.2, 1-0.4) = (0.8, 0.6)
        assert_eq!(scrcpy_pos.0, 864); // 0.8 * 1080
        assert_eq!(scrcpy_pos.1, 1440); // 0.6 * 2400
    }

    #[test]
    fn test_mapping_3_to_scrcpy_capture_lock_0() {
        // Landscape upside-down device (orient=3), capture locked to portrait (orient=0)
        let mapping_sys = MappingCoordSys::new(3);
        let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(0));

        let scrcpy_pos = mapping_sys.to_scrcpy((0.2, 0.4), &scrcpy_sys);

        // 270° CW rotation: (0.2, 0.4) -> (0.4, 1-0.2) = (0.4, 0.8)
        assert_eq!(scrcpy_pos.0, 432); // 0.4 * 1080
        assert_eq!(scrcpy_pos.1, 1920); // 0.8 * 2400
    }

    #[test]
    fn test_mapping_0_to_scrcpy_capture_lock_1() {
        // Portrait device (orient=0), capture locked to landscape (orient=1)
        let mapping_sys = MappingCoordSys::new(0);
        let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(1));

        let scrcpy_pos = mapping_sys.to_scrcpy((0.2, 0.4), &scrcpy_sys);

        // 90° CW rotation: (0.2, 0.4) -> (1-0.4, 0.2) = (0.6, 0.2)
        assert_eq!(scrcpy_pos.0, 1440); // 0.6 * 2400
        assert_eq!(scrcpy_pos.1, 216); // 0.2 * 1080
    }

    #[test]
    fn test_mapping_1_to_scrcpy_capture_lock_1() {
        // Landscape device (orient=1), capture locked to landscape (orient=1)
        let mapping_sys = MappingCoordSys::new(1);
        let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(1));

        let scrcpy_pos = mapping_sys.to_scrcpy((0.2, 0.4), &scrcpy_sys);

        // 180° rotation: (0.2, 0.4) -> (1-0.2, 1-0.4) = (0.8, 0.6)
        assert_eq!(scrcpy_pos.0, 1920); // 0.8 * 2400
        assert_eq!(scrcpy_pos.1, 648); // 0.6 * 1080
    }

    #[test]
    fn test_mapping_2_to_scrcpy_capture_lock_1() {
        // Portrait upside-down device (orient=2), capture locked to landscape (orient=1)
        let mapping_sys = MappingCoordSys::new(2);
        let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(1));

        let scrcpy_pos = mapping_sys.to_scrcpy((0.2, 0.4), &scrcpy_sys);

        // 270° CW rotation: (0.2, 0.4) -> (0.4, 1-0.2) = (0.4, 0.8)
        assert_eq!(scrcpy_pos.0, 960); // 0.4 * 2400
        assert_eq!(scrcpy_pos.1, 864); // 0.8 * 1080
    }

    #[test]
    fn test_mapping_3_to_scrcpy_capture_lock_1() {
        // Landscape upside-down device (orient=3), capture locked to landscape (orient=1)
        let mapping_sys = MappingCoordSys::new(3);
        let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(1));

        let scrcpy_pos = mapping_sys.to_scrcpy((0.2, 0.4), &scrcpy_sys);

        // 0° rotation: (0.2, 0.4) -> (0.2, 0.4)
        assert_eq!(scrcpy_pos.0, 480); // 0.2 * 2400
        assert_eq!(scrcpy_pos.1, 432); // 0.4 * 1080
    }

    #[test]
    fn test_mapping_0_to_scrcpy_capture_lock_2() {
        // Portrait device (orient=0), capture locked to 180° (orient=2)
        let mapping_sys = MappingCoordSys::new(0);
        let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(2));

        let scrcpy_pos = mapping_sys.to_scrcpy((0.2, 0.4), &scrcpy_sys);

        // 180° rotation: (0.2, 0.4) -> (1-0.2, 1-0.4) = (0.8, 0.6)
        assert_eq!(scrcpy_pos.0, 864); // 0.7 * 1080
        assert_eq!(scrcpy_pos.1, 1440); // 0.3 * 2400
    }

    #[test]
    fn test_mapping_1_to_scrcpy_capture_lock_2() {
        // Landscape device (orient=1), capture locked to 180° (orient=2)
        let mapping_sys = MappingCoordSys::new(1);
        let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(2));

        let scrcpy_pos = mapping_sys.to_scrcpy((0.2, 0.4), &scrcpy_sys);

        // 270° CW rotation: (0.2, 0.4) -> (0.4, 1-0.2) = (0.4, 0.8)
        assert_eq!(scrcpy_pos.0, 432); // 0.4 * 1080
        assert_eq!(scrcpy_pos.1, 1920); // 0.8 * 2400
    }

    #[test]
    fn test_mapping_2_to_scrcpy_capture_lock_2() {
        // Portrait upside-down device (orient=2), capture locked to 180° (orient=2)
        let mapping_sys = MappingCoordSys::new(2);
        let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(2));

        let scrcpy_pos = mapping_sys.to_scrcpy((0.2, 0.4), &scrcpy_sys);

        // 0° rotation: (0.2, 0.4) -> (0.2, 0.4)
        assert_eq!(scrcpy_pos.0, 216); // 0.2 * 1080
        assert_eq!(scrcpy_pos.1, 960); // 0.4 * 2400
    }

    #[test]
    fn test_mapping_3_to_scrcpy_capture_lock_2() {
        // Landscape upside-down device (orient=3), capture locked to 180° (orient=2)
        let mapping_sys = MappingCoordSys::new(3);
        let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(2));

        let scrcpy_pos = mapping_sys.to_scrcpy((0.2, 0.4), &scrcpy_sys);

        // 90° CCW rotation: (0.2, 0.4) -> (1-0.4, 0.2) = (0.6, 0.2)
        assert_eq!(scrcpy_pos.0, 648); // 0.6 * 1080
        assert_eq!(scrcpy_pos.1, 480); // 0.2 * 2400
    }

    #[test]
    fn test_mapping_0_to_scrcpy_capture_lock_3() {
        // Portrait device (orient=0), capture locked to landscape 270° (orient=3)
        let mapping_sys = MappingCoordSys::new(0);
        let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(3));

        let scrcpy_pos = mapping_sys.to_scrcpy((0.2, 0.4), &scrcpy_sys);

        // 270° CW rotation: (0.2, 0.4) -> (0.4, 1-0.2) = (0.4, 0.8)
        assert_eq!(scrcpy_pos.0, 960); // 0.4 * 2400
        assert_eq!(scrcpy_pos.1, 864); // 0.8 * 1080
    }

    #[test]
    fn test_mapping_1_to_scrcpy_capture_lock_3() {
        // Landscape device (orient=1), capture locked to landscape 270° (orient=3)
        let mapping_sys = MappingCoordSys::new(1);
        let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(3));

        let scrcpy_pos = mapping_sys.to_scrcpy((0.2, 0.4), &scrcpy_sys);

        // 0° rotation: (0.2, 0.4) -> (0.2, 0.4)
        assert_eq!(scrcpy_pos.0, 480); // 0.2 * 2400
        assert_eq!(scrcpy_pos.1, 432); // 0.4 * 1080
    }

    #[test]
    fn test_mapping_2_to_scrcpy_capture_lock_3() {
        // Portrait upside-down device (orient=2), capture locked to landscape 270° (orient=3)
        let mapping_sys = MappingCoordSys::new(2);
        let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(3));

        let scrcpy_pos = mapping_sys.to_scrcpy((0.2, 0.4), &scrcpy_sys);

        // 90° CCW rotation: (0.2, 0.4) -> (1-0.4, 0.2) = (0.6, 0.2)
        assert_eq!(scrcpy_pos.0, 1440); // 0.6 * 2400
        assert_eq!(scrcpy_pos.1, 216); // 0.2 * 1080
    }

    #[test]
    fn test_mapping_3_to_scrcpy_capture_lock_3() {
        // Landscape upside-down device (orient=3), capture locked to landscape 270° (orient=3)
        let mapping_sys = MappingCoordSys::new(3);
        let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(3));

        let scrcpy_pos = mapping_sys.to_scrcpy((0.2, 0.4), &scrcpy_sys);

        // 180° rotation: (0.2, 0.4) -> (1-0.2, 1-0.4) = (0.8, 0.6)
        assert_eq!(scrcpy_pos.0, 1920); // 0.8 * 2400
        assert_eq!(scrcpy_pos.1, 648); // 0.6 * 1080
    }

    #[test]
    fn test_scrcpy_no_capture_lock_to_mapping_0() {
        // Portrait device, no capture lock
        let mapping_sys = MappingCoordSys::new(0);
        let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, None);

        // (0.2, 0.4) in scrcpy coords
        let mapping_pos = mapping_sys.from_scrcpy((216, 960), &scrcpy_sys);

        // Portrait device: direct scale
        assert!((mapping_pos.0 - 0.2).abs() < 0.001);
        assert!((mapping_pos.1 - 0.4).abs() < 0.001);
    }

    #[test]
    fn test_scrcpy_no_capture_lock_to_mapping_1() {
        // Landscape device, no capture lock
        let mapping_sys = MappingCoordSys::new(1);
        let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, None);

        // (0.2, 0.4) in scrcpy coords
        let mapping_pos = mapping_sys.from_scrcpy((480, 432), &scrcpy_sys);

        // Landscape device: direct scale
        assert!((mapping_pos.0 - 0.2).abs() < 0.001);
        assert!((mapping_pos.1 - 0.4).abs() < 0.001);
    }

    #[test]
    fn test_scrcpy_no_capture_lock_to_mapping_2() {
        // Portrait upside-down device, no capture lock
        let mapping_sys = MappingCoordSys::new(2);
        let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, None);

        // (0.2, 0.4) in scrcpy coords
        let mapping_pos = mapping_sys.from_scrcpy((216, 960), &scrcpy_sys);

        // Portrait upside-down device: direct scale
        assert!((mapping_pos.0 - 0.2).abs() < 0.001);
        assert!((mapping_pos.1 - 0.4).abs() < 0.001);
    }

    #[test]
    fn test_scrcpy_no_capture_lock_to_mapping_3() {
        // Landscape upside-down device, no capture lock
        let mapping_sys = MappingCoordSys::new(3);
        let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, None);

        // (0.2, 0.4) in scrcpy coords
        let mapping_pos = mapping_sys.from_scrcpy((480, 432), &scrcpy_sys);

        // Landscape upside-down device: direct scale
        assert!((mapping_pos.0 - 0.2).abs() < 0.001);
        assert!((mapping_pos.1 - 0.4).abs() < 0.001);
    }

    #[test]
    fn test_scrcpy_capture_lock_0_to_mapping_0() {
        // Portrait device (orient=0), capture locked to portrait (orient=0)
        let mapping_sys = MappingCoordSys::new(0);
        let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(0));

        // (0.2, 0.4) in scrcpy coords
        let mapping_pos = mapping_sys.from_scrcpy((216, 960), &scrcpy_sys);

        // No rotation
        assert!((mapping_pos.0 - 0.2).abs() < 0.001);
        assert!((mapping_pos.1 - 0.4).abs() < 0.001);
    }

    #[test]
    fn test_scrcpy_capture_lock_0_to_mapping_1() {
        // Landscape device (orient=1), capture locked to portrait (orient=0)
        let mapping_sys = MappingCoordSys::new(1);
        let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(0));

        // (0.2, 0.4) in scrcpy coords
        let mapping_pos = mapping_sys.from_scrcpy((216, 960), &scrcpy_sys);

        // 90° CCW rotation: (0.2, 0.4) -> (0.4, 1-0.2) = (0.4, 0.8)
        assert!((mapping_pos.0 - 0.4).abs() < 0.001);
        assert!((mapping_pos.1 - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_scrcpy_capture_lock_0_to_mapping_2() {
        // Portrait upside-down device (orient=2), capture locked to portrait (orient=0)
        let mapping_sys = MappingCoordSys::new(2);
        let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(0));

        // (0.2, 0.4) in scrcpy coords
        let mapping_pos = mapping_sys.from_scrcpy((216, 960), &scrcpy_sys);

        // 180° rotation: (0.2, 0.4) -> (1-0.2, 1-0.4) = (0.8, 0.6)
        assert!((mapping_pos.0 - 0.8).abs() < 0.001);
        assert!((mapping_pos.1 - 0.6).abs() < 0.001);
    }

    #[test]
    fn test_scrcpy_capture_lock_0_to_mapping_3() {
        // Landscape upside-down device (orient=3), capture locked to portrait (orient=0)
        let mapping_sys = MappingCoordSys::new(3);
        let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(0));

        // (0.2, 0.4) in scrcpy coords
        let mapping_pos = mapping_sys.from_scrcpy((216, 960), &scrcpy_sys);

        // 270° CCW rotation: (0.2, 0.4) -> (1-0.4, 0.2) = (0.6, 0.2)
        assert!((mapping_pos.0 - 0.6).abs() < 0.001);
        assert!((mapping_pos.1 - 0.2).abs() < 0.001);
    }

    #[test]
    fn test_scrcpy_capture_lock_1_to_mapping_0() {
        // Portrait device (orient=0), capture locked to landscape (orient=1)
        let mapping_sys = MappingCoordSys::new(0);
        let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(1));

        // (0.2, 0.4) in scrcpy coords
        let mapping_pos = mapping_sys.from_scrcpy((480, 432), &scrcpy_sys);

        // 90° CCW rotation: (0.2, 0.4) -> (0.4, 1-0.2) = (0.4, 0.8)
        assert!((mapping_pos.0 - 0.4).abs() < 0.001);
        assert!((mapping_pos.1 - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_scrcpy_capture_lock_1_to_mapping_1() {
        // Landscape device (orient=1), capture locked to landscape (orient=1)
        let mapping_sys = MappingCoordSys::new(1);
        let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(1));

        // (0.2, 0.4) in scrcpy coords
        let mapping_pos = mapping_sys.from_scrcpy((480, 432), &scrcpy_sys);

        // 180° rotation: (0.2, 0.4) -> (1-0.2, 1-0.4) = (0.8, 0.6)
        assert!((mapping_pos.0 - 0.8).abs() < 0.001);
        assert!((mapping_pos.1 - 0.6).abs() < 0.001);
    }

    #[test]
    fn test_scrcpy_capture_lock_1_to_mapping_2() {
        // Portrait upside-down device (orient=2), capture locked to landscape (orient=1)
        let mapping_sys = MappingCoordSys::new(2);
        let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(1));

        // (0.2, 0.4) in scrcpy coords
        let mapping_pos = mapping_sys.from_scrcpy((480, 432), &scrcpy_sys);

        // 270° CCW rotation: (0.2, 0.4) -> (1-0.4, 0.2) = (0.6, 0.2)
        assert!((mapping_pos.0 - 0.6).abs() < 0.001);
        assert!((mapping_pos.1 - 0.2).abs() < 0.001);
    }

    #[test]
    fn test_scrcpy_capture_lock_1_to_mapping_3() {
        // Landscape upside-down device (orient=3), capture locked to landscape (orient=1)
        let mapping_sys = MappingCoordSys::new(3);
        let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(1));

        // (0.2, 0.4) in scrcpy coords
        let mapping_pos = mapping_sys.from_scrcpy((480, 432), &scrcpy_sys);

        // 0° rotation: (0.2, 0.4) -> (0.2, 0.4)
        assert!((mapping_pos.0 - 0.2).abs() < 0.001);
        assert!((mapping_pos.1 - 0.4).abs() < 0.001);
    }

    #[test]
    fn test_scrcpy_capture_lock_2_to_mapping_0() {
        // Portrait device (orient=0), capture locked to 180° (orient=2)
        let mapping_sys = MappingCoordSys::new(0);
        let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(2));

        // (0.2, 0.4) in scrcpy coords
        let mapping_pos = mapping_sys.from_scrcpy((216, 960), &scrcpy_sys);

        // 180° rotation: (0.2, 0.4) -> (1-0.2, 1-0.4) = (0.8, 0.6)
        assert!((mapping_pos.0 - 0.8).abs() < 0.001);
        assert!((mapping_pos.1 - 0.6).abs() < 0.001);
    }

    #[test]
    fn test_scrcpy_capture_lock_2_to_mapping_1() {
        // Landscape device (orient=1), capture locked to 180° (orient=2)
        let mapping_sys = MappingCoordSys::new(1);
        let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(2));

        // (0.2, 0.4) in scrcpy coords
        let mapping_pos = mapping_sys.from_scrcpy((216, 960), &scrcpy_sys);

        // 270° CCW rotation: (0.2, 0.4) -> (1-0.4, 0.2) = (0.6, 0.2)
        assert!((mapping_pos.0 - 0.6).abs() < 0.001);
        assert!((mapping_pos.1 - 0.2).abs() < 0.001);
    }

    #[test]
    fn test_scrcpy_capture_lock_2_to_mapping_2() {
        // Portrait upside-down device (orient=2), capture locked to 180° (orient=2)
        let mapping_sys = MappingCoordSys::new(2);
        let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(2));

        // (0.2, 0.4) in scrcpy coords
        let mapping_pos = mapping_sys.from_scrcpy((216, 960), &scrcpy_sys);

        // 0° rotation: (0.2, 0.4) -> (0.2, 0.4)
        assert!((mapping_pos.0 - 0.2).abs() < 0.001);
        assert!((mapping_pos.1 - 0.4).abs() < 0.001);
    }

    #[test]
    fn test_scrcpy_capture_lock_2_to_mapping_3() {
        // Landscape upside-down device (orient=3), capture locked to 180° (orient=2)
        let mapping_sys = MappingCoordSys::new(3);
        let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(2));

        // (0.2, 0.4) in scrcpy coords
        let mapping_pos = mapping_sys.from_scrcpy((216, 960), &scrcpy_sys);

        // 90° CCW rotation: (0.2, 0.4) -> (0.4, 1-0.2) = (0.4, 0.8)
        assert!((mapping_pos.0 - 0.4).abs() < 0.001);
        assert!((mapping_pos.1 - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_scrcpy_capture_lock_3_to_mapping_0() {
        // Portrait device (orient=0), capture locked to landscape 270° (orient=3)
        let mapping_sys = MappingCoordSys::new(0);
        let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(3));

        // (0.2, 0.4) in scrcpy coords
        let mapping_pos = mapping_sys.from_scrcpy((480, 432), &scrcpy_sys);

        // 270° CCW rotation: (0.2, 0.4) -> (1-0.4, 0.2) = (0.6, 0.2)
        assert!((mapping_pos.0 - 0.6).abs() < 0.001);
        assert!((mapping_pos.1 - 0.2).abs() < 0.001);
    }

    #[test]
    fn test_scrcpy_capture_lock_3_to_mapping_1() {
        // Landscape device (orient=1), capture locked to landscape 270° (orient=3)
        let mapping_sys = MappingCoordSys::new(1);
        let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(3));

        // (0.2, 0.4) in scrcpy coords
        let mapping_pos = mapping_sys.from_scrcpy((480, 432), &scrcpy_sys);

        // 0° rotation: (0.2, 0.4) -> (0.2, 0.4)
        assert!((mapping_pos.0 - 0.2).abs() < 0.001);
        assert!((mapping_pos.1 - 0.4).abs() < 0.001);
    }

    #[test]
    fn test_scrcpy_capture_lock_3_to_mapping_2() {
        // Portrait upside-down device (orient=2), capture locked to landscape 270° (orient=3)
        let mapping_sys = MappingCoordSys::new(2);
        let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(3));

        // (0.2, 0.4) in scrcpy coords
        let mapping_pos = mapping_sys.from_scrcpy((480, 432), &scrcpy_sys);

        // 90° CCW rotation: (0.2, 0.4) -> (0.4, 1-0.2) = (0.4, 0.8)
        assert!((mapping_pos.0 - 0.4).abs() < 0.001);
        assert!((mapping_pos.1 - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_scrcpy_capture_lock_3_to_mapping_3() {
        // Landscape upside-down device (orient=3), capture locked to landscape 270° (orient=3)
        let mapping_sys = MappingCoordSys::new(3);
        let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(3));

        // (0.2, 0.4) in scrcpy coords
        let mapping_pos = mapping_sys.from_scrcpy((480, 432), &scrcpy_sys);

        // 180° rotation: (0.2, 0.4) -> (1-0.2, 1-0.4) = (0.8, 0.6)
        assert!((mapping_pos.0 - 0.8).abs() < 0.001);
        assert!((mapping_pos.1 - 0.6).abs() < 0.001);
    }

    #[test]
    fn test_visual_rotation_0_to_scrcpy_no_capture_lock() {
        let visual_sys = VisualCoordSys::new(0);
        let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, None);
        let video_rect = Rect::from_min_size(Pos2::new(0.0, 0.0), (1080.0, 2400.0).into());

        // (0.2, 0.4) in visual coords
        // No rotation: (0.2, 0.4) -> (0.2, 0.4)
        let scrcpy_pos = visual_sys
            .to_scrcpy(Pos2::new(216.0, 960.0), video_rect, &scrcpy_sys)
            .unwrap();

        assert_eq!(scrcpy_pos.0, 216);
        assert_eq!(scrcpy_pos.1, 960);
    }

    #[test]
    fn test_visual_rotation_1_to_scrcpy_capture_lock_1() {
        let visual_sys = VisualCoordSys::new(1);
        let video_rect = Rect::from_min_size(Pos2::new(20.0, 30.0), (1080.0, 2400.0).into());
        let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, None);

        // (0.2, 0.4) in visual coords
        // 90° CCW rotation: (0.2, 0.4) -> (0.4, 1-0.2) = (0.4, 0.8)
        let scrcpy_pos = visual_sys
            .to_scrcpy(Pos2::new(236.0, 990.0), video_rect, &scrcpy_sys)
            .unwrap();
        assert_eq!(scrcpy_pos.0, 960);
        assert_eq!(scrcpy_pos.1, 864);
    }

    #[test]
    fn test_visual_rotation_2_to_scrcpy_capture_lock_2() {
        let visual_sys = VisualCoordSys::new(2);
        let video_rect = Rect::from_min_size(Pos2::new(40.0, 60.0), (1080.0, 2400.0).into());
        let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, None);

        // (0.2, 0.4) in visual coords
        // 180° rotation: (0.2, 0.4) -> (1-0.2, 1-0.4) = (0.8, 0.6)
        let scrcpy_pos = visual_sys
            .to_scrcpy(Pos2::new(256.0, 1020.0), video_rect, &scrcpy_sys)
            .unwrap();

        assert_eq!(scrcpy_pos.0, 864);
        assert_eq!(scrcpy_pos.1, 1440);
    }

    #[test]
    fn test_visual_rotation_3_to_scrcpy_capture_lock_3() {
        let visual_sys = VisualCoordSys::new(3);
        let video_rect = Rect::from_min_size(Pos2::new(0.0, 0.0), (1080.0, 2400.0).into());
        let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, None);

        // (0.2, 0.4) in visual coords
        // 270° CCW rotation: (0.2, 0.4) -> (1-0.4, 0.2) = (0.6, 0.2)
        let scrcpy_pos = visual_sys
            .to_scrcpy(Pos2::new(216.0, 960.0), video_rect, &scrcpy_sys)
            .unwrap();

        assert_eq!(scrcpy_pos.0, 1440);
        assert_eq!(scrcpy_pos.1, 216);
    }

    #[test]
    fn test_scrcpy_to_visual_rotaion_0() {
        let visual_sys = VisualCoordSys::new(0);
        let video_rect = Rect::from_min_size(Pos2::new(10.0, 20.0), (1080.0, 2400.0).into());
        let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, None);

        // (0.2, 0.4) in scrcpy coords
        let original_pos = (216, 960);

        // Scrcpy -> Visual
        // No rotation: (0.2, 0.4) -> (0.2, 0.4)
        let visual_pos = visual_sys
            .from_scrcpy((original_pos.0, original_pos.1), video_rect, &scrcpy_sys)
            .unwrap();
        assert!((visual_pos.x - 216.0 - 10.0).abs() < 1.0);
        assert!((visual_pos.y - 960.0 - 20.0).abs() < 1.0);
    }

    #[test]
    fn test_scrcpy_to_visual_rotaion_1() {
        let visual_sys = VisualCoordSys::new(1);
        let video_rect = Rect::from_min_size(Pos2::new(20.0, 30.0), (1080.0, 2400.0).into());
        let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(1));

        // (0.2, 0.4) in scrcpy coords
        let original_pos = (480, 432);

        // Scrcpy -> Visual
        // 90° CW rotation: (0.2, 0.4) -> (1-0.4, 0.2) = (0.6, 0.2)
        let visual_pos = visual_sys
            .from_scrcpy((original_pos.0, original_pos.1), video_rect, &scrcpy_sys)
            .unwrap();
        assert!((visual_pos.x - 648.0 - 20.0).abs() < 1.0);
        assert!((visual_pos.y - 480.0 - 30.0).abs() < 1.0);
    }

    #[test]
    fn test_scrcpy_to_visual_rotaion_2() {
        let visual_sys = VisualCoordSys::new(2);
        let video_rect = Rect::from_min_size(Pos2::new(40.0, 60.0), (1080.0, 2400.0).into());
        let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, Some(2));

        // (0.2, 0.4) in scrcpy coords
        let original_pos = (216, 960);

        // Scrcpy -> Visual
        // 180° rotation: (0.2, 0.4) -> (1-0.2, 1-0.4) = (0.8, 0.6)
        let visual_pos = visual_sys
            .from_scrcpy((original_pos.0, original_pos.1), video_rect, &scrcpy_sys)
            .unwrap();
        assert!((visual_pos.x - 864.0 - 40.0).abs() < 1.0);
        assert!((visual_pos.y - 1440.0 - 60.0).abs() < 1.0);
    }

    #[test]
    fn test_scrcpy_to_visual_rotaion_3() {
        let visual_sys = VisualCoordSys::new(3);
        let video_rect = Rect::from_min_size(Pos2::new(0.0, 0.0), (1080.0, 2400.0).into());
        let scrcpy_sys = ScrcpyCoordSys::new(2400, 1080, Some(3));

        // (0.2, 0.4) in scrcpy coords
        let original_pos = (480, 432);

        // Scrcpy -> Visual
        // 270° CW rotation: (0.2, 0.4) -> (0.4, 1-0.2) = (0.4, 0.8)
        let visual_pos = visual_sys
            .from_scrcpy((original_pos.0, original_pos.1), video_rect, &scrcpy_sys)
            .unwrap();
        assert!((visual_pos.x - 432.0).abs() < 1.0);
        assert!((visual_pos.y - 1920.0).abs() < 1.0);
    }

    #[test]
    fn test_visual_mapping_roundtrip() {
        let visual_sys = VisualCoordSys::new(0);
        let video_rect = Rect::from_min_size(Pos2::new(10.0, 20.0), (1080.0, 2400.0).into());
        let scrcpy_sys = ScrcpyCoordSys::new(1080, 2400, None);
        let mapping_sys = MappingCoordSys::new(0);

        let original_pos = Pos2::new(550.0, 1220.0);

        // Visual -> Mapping -> Visual
        let mapping_pos = visual_sys
            .to_mapping(original_pos, video_rect, &scrcpy_sys, &mapping_sys)
            .unwrap();
        let result_pos = visual_sys
            .from_mapping(mapping_pos, video_rect, &scrcpy_sys, &mapping_sys)
            .unwrap();

        assert!((original_pos.x - result_pos.x).abs() < 1.0);
        assert!((original_pos.y - result_pos.y).abs() < 1.0);
    }
}
