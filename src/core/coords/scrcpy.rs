//! Scrcpy control protocol coordinate system implementation

use {
    super::{
        mapping::MappingCoordSys,
        types::{MappingPos, ScrcpyPos, VisualPos},
    },
    eframe::egui::Rect,
};

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

    pub fn update_video_size(&mut self, width: u16, height: u16) {
        self.video_width = width;
        self.video_height = height;
    }

    pub fn update_capture_orientation(&mut self, orientation: Option<u32>) {
        self.capture_orientation = orientation.map(|o| o % 4);
    }

    /// Convert scrcpy coordinate to VisualCoordSys
    ///
    /// # Parameters
    /// - `pos`: ScrcpyPos (x, y) pixel coordinates
    /// - `target`: Target VisualCoordSys
    pub fn to_visual(&self, pos: &ScrcpyPos, rect: &Rect, rotation: u32) -> VisualPos {
        // Video coords are in video frame space
        // Need to transform through user rotation to display space

        let (video_w, video_h) = if rotation & 1 == 0 {
            (rect.width(), rect.height())
        } else {
            // 90° or 270° rotation swaps dimensions
            (rect.height(), rect.width())
        };

        // Normalize to video frame coordinates
        let x_norm = pos.x as f32 / self.video_width as f32;
        let y_norm = pos.y as f32 / self.video_height as f32;

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
            _ => {
                debug_assert!(false, "rotation must be 0-3, got {}", rotation);
                (x_norm * video_w, y_norm * video_h)
            }
        };

        VisualPos::new(rect.left() + rel_x, rect.top() + rel_y)
    }

    /// Convert from visual coordinates to scrcpy coordinate
    ///
    /// # Parameters
    /// - `pos`: Screen position (VisualPos)
    /// - `rect`: Video display rectangle
    /// - `rotation`: User manual rotation (0-3, clockwise 90°)
    pub fn from_visual(&self, pos: &VisualPos, rect: &Rect, rotation: u32) -> Option<ScrcpyPos> {
        // Check if position is within rect
        if !rect.contains(*pos) {
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
            _ => {
                debug_assert!(false, "rotation must be 0-3, got {}", rotation);
                (rel_x, rel_y, display_width, display_height)
            }
        };

        // Normalize and scale to video resolution
        let x_norm = video_x / video_w;
        let y_norm = video_y / video_h;

        Some(ScrcpyPos::new(
            (x_norm * self.video_width as f32) as u32,
            (y_norm * self.video_height as f32) as u32,
        ))
    }

    /// Convert to MappingCoordSys
    ///
    /// # Parameters
    /// - `pos`: ScrcpyPos (x, y) pixel coordinates
    /// - `target`: Target MappingCoordSys
    pub fn to_mapping(&self, pos: &ScrcpyPos, target: &MappingCoordSys) -> MappingPos {
        target.from_scrcpy(pos, self)
    }

    /// Convert from MappingCoordSys
    ///
    /// # Parameters
    /// - `pos`: MappingPos (x, y) in 0.0-1.0 range
    /// - `source`: Source MappingCoordSys
    pub fn from_mapping(&self, pos: &MappingPos, source: &MappingCoordSys) -> ScrcpyPos {
        source.to_scrcpy(pos, self)
    }
}
