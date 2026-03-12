// SPDX-License-Identifier: MIT OR Apache-2.0

//! Mapping coordinate system implementation

use super::{
    scrcpy::ScrcpyCoordSys,
    types::{MappingPos, ScrcpyPos},
};

/// Mapping coordinate system (0.0-1.0 normalized, stored in config)
///
/// Bound to device orientation at time of mapping creation.
/// - device_orientation: 0=0°, 1=90°CW, 2=180°, 3=270°CW (Android Display Rotation)
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

    pub fn update_device_orientation(&mut self, orientation: u32) {
        self.device_orientation = orientation % 4;
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
    pub fn to_scrcpy(&self, pos: &MappingPos, target: &ScrcpyCoordSys) -> ScrcpyPos {
        let (px, py) = if let Some(capture_orient) = target.capture_orientation {
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
                        pos.x * target.video_width as f32,
                        pos.y * target.video_height as f32,
                    )
                }
                1 => {
                    // 90° CW: (x, y) -> (1-y, x)
                    // Swap dimensions
                    (
                        (1.0 - pos.y) * target.video_width as f32,
                        pos.x * target.video_height as f32,
                    )
                }
                2 => {
                    // 180°: (x, y) -> (1-x, 1-y)
                    (
                        (1.0 - pos.x) * target.video_width as f32,
                        (1.0 - pos.y) * target.video_height as f32,
                    )
                }
                3 => {
                    // 270° CW = 90° CCW: (x, y) -> (y, 1-x)
                    // Swap dimensions
                    (
                        pos.y * target.video_width as f32,
                        (1.0 - pos.x) * target.video_height as f32,
                    )
                }
                _ => {
                    debug_assert!(false, "rotation must be 0-3, got {}", rotation);
                    (
                        pos.x * target.video_width as f32,
                        pos.y * target.video_height as f32,
                    )
                }
            }
        } else {
            // Capture not locked - video follows device orientation
            // Video resolution already matches device orientation, direct scale
            (
                pos.x * target.video_width as f32,
                pos.y * target.video_height as f32,
            )
        };

        ScrcpyPos::new(px as u32, py as u32)
    }

    /// Convert from ScrcpyCoordSys to mapping coordinate (0.0-1.0)
    ///
    /// # Parameters
    /// - `pos`: (x, y) pixel coordinates
    /// - `source`: Source ScrcpyCoordSys
    pub fn from_scrcpy(&self, pos: &ScrcpyPos, source: &ScrcpyCoordSys) -> MappingPos {
        let px = pos.x as f32 / source.video_width as f32;
        let py = pos.y as f32 / source.video_height as f32;

        let (x, y) = if let Some(capture_orient) = source.capture_orientation {
            let rotation = (self.device_orientation + capture_orient) % 4;
            match rotation {
                0 => {
                    // No rotation
                    (px, py)
                }
                1 => {
                    // 90° CCW: (x, y) -> (y, 1-x)
                    (py, 1.0 - px)
                }
                2 => {
                    // 180°: (x, y) -> (1-x, 1-y)
                    (1.0 - px, 1.0 - py)
                }
                3 => {
                    // 270° CCW: (x, y) -> (1-y, x)
                    (1.0 - py, px)
                }
                _ => {
                    debug_assert!(false, "rotation must be 0-3, got {}", rotation);
                    (px, py)
                }
            }
        } else {
            // Capture not locked - video follows device
            (px, py)
        };

        MappingPos::new(x, y)
    }
}
