/// Utility functions for coordinate transformations and mapping lookups
use {
    crate::config::mapping::{AdbAction, Key, KeyMapping},
    eframe::egui::Pos2,
    tracing::debug,
};

pub struct CoordinatesTransformParams {
    pub video_rect: egui::Rect,
    pub video_rotation: u32,
    pub device_physical_size: (u32, u32),
    pub device_orientation: u32,
    pub capture_orientation: u32,
}

/// Transform egui position to device logical coordinates for ADB input
///
/// Coordinate transformation chain:
/// 1. egui screen coords -> video display coords (considering user rotation)
/// 2. Inverse apply user rotation -> video original coords (scrcpy fixed output)
/// 3. Transform from video orientation to device current orientation -> ADB logical coords
///
/// Note: ADB automatically handles the mapping from logical coords to physical touch coords,
/// so we only need to provide coords relative to the device's current display orientation.
pub fn screen_to_device_coords(
    pos: &egui::Pos2,
    trans_params: &CoordinatesTransformParams,
) -> Option<(u32, u32)> {
    let (video_rect, video_rotation, device_physical_size, device_orientation, capture_orientation) = (
        &trans_params.video_rect,
        trans_params.video_rotation,
        trans_params.device_physical_size,
        trans_params.device_orientation,
        trans_params.capture_orientation,
    );

    // Step 1: Get relative coordinates in video display rect
    let rel_x = pos.x - video_rect.left();
    let rel_y = pos.y - video_rect.top();

    let video_width = video_rect.width();
    let video_height = video_rect.height();

    debug!("== Coordinate Transform ==");
    debug!("Screen pos: ({:.1}, {:.1})", pos.x, pos.y);
    debug!("Video rect: {:?}", video_rect);
    debug!("Relative pos in video: ({:.1}, {:.1})", rel_x, rel_y);

    // Step 2: Inverse apply user rotation to get video original coordinates
    // This transforms from rotated display back to scrcpy's fixed output orientation
    //
    // Note: video_width/height here are display rect dimensions (after rotation)
    // Original video dimensions need to be reconstructed based on rotation
    let (video_x, video_y, video_w, video_h) = match video_rotation % 4 {
        // 0 degrees - no rotation
        // Display: W×H, Original: W×H
        0 => (rel_x, rel_y, video_width, video_height),

        // 90 degrees clockwise rotation
        // Display: H×W, Original: W×H
        // Inverse transform: (x', y') => (y', H - x')
        1 => (rel_y, video_width - rel_x, video_height, video_width),

        // 180 degrees rotation
        // Display: W×H, Original: W×H
        // Inverse transform: (x', y') => (W - x', H - y')
        2 => (
            video_width - rel_x,
            video_height - rel_y,
            video_width,
            video_height,
        ),

        // 270 degrees clockwise rotation
        // Display: H×W, Original: W×H
        // Inverse transform: (x', y') => (W - y', x')
        3 => (video_height - rel_y, rel_x, video_height, video_width),

        _ => return None,
    };

    debug!(
        "Video original coords: ({:.1}, {:.1}) in {}x{}",
        video_x, video_y, video_w, video_h
    );
    debug!("Video rotation: {}", video_rotation);
    debug!("Device orientation: {}", device_orientation);
    debug!("Device physical size: {:?}", device_physical_size);

    // Step 3: Transform from video orientation to device current orientation
    //
    // Video orientation: natural orientation + counter-clockwise capture_orientation
    // Device current orientation: natural orientation + clockwise orientation
    // Total rotation needed: clockwise (capture_orientation + orientation)
    //
    // This accounts for:
    // - Video is captured with fixed orientation (capture_orientation counter-clockwise)
    // - Device may be rotated to different orientation (orientation clockwise)
    // - ADB expects coords relative to device's current display orientation
    let total_rotation = (capture_orientation + device_orientation) % 4;
    debug!("Total rotation: {}", total_rotation);

    // Calculate device logical size at current orientation
    let (device_w, device_h) = if device_orientation & 1 == 0 {
        (device_physical_size.0 as f32, device_physical_size.1 as f32)
    } else {
        (device_physical_size.1 as f32, device_physical_size.0 as f32)
    };

    // Apply rotation and scaling
    let (device_x, device_y) = match total_rotation {
        // 0 degrees - direct scale
        0 => {
            let scale_x = device_w / video_w;
            let scale_y = device_h / video_h;
            (video_x * scale_x, video_y * scale_y)
        }
        // 90 degrees clockwise - transpose and flip X
        1 => {
            let scale_x = device_w / video_h;
            let scale_y = device_h / video_w;
            (video_y * scale_x, device_h - video_x * scale_y)
        }
        // 180 degrees - flip both axes
        2 => {
            let scale_x = device_w / video_w;
            let scale_y = device_h / video_h;
            (device_w - video_x * scale_x, device_h - video_y * scale_y)
        }
        // 270 degrees clockwise - transpose and flip Y
        3 => {
            let scale_x = device_w / video_h;
            let scale_y = device_h / video_w;
            (device_w - video_y * scale_x, video_x * scale_y)
        }
        _ => return None,
    };

    debug!(
        "Device logical size: {}x{}",
        device_w as u32, device_h as u32
    );
    debug!(
        "Device coords: ({:.1}, {:.1}) -> ({}, {})",
        device_x,
        device_y,
        device_x as u32,
        device_y as u32
    );

    Some((device_x as u32, device_y as u32))
}

/// Convert device coordinates to screen coordinates in video rect
pub fn device_to_screen_coords(
    device_pos: (u32, u32),
    transform_params: &CoordinatesTransformParams,
) -> Option<Pos2> {
    let (video_rect, video_rotation, device_physical_size, device_orientation, capture_orientation) = (
        &transform_params.video_rect,
        transform_params.video_rotation,
        transform_params.device_physical_size,
        transform_params.device_orientation,
        transform_params.capture_orientation,
    );
    // This is the inverse of coordinate_transform
    //
    // coordinate_transform does:
    // 1. Screen -> Video coords (inverse user rotation)
    // 2. Video -> Device coords (apply total_rotation)
    //
    // We need to do:
    // 1. Device -> Video coords (inverse total_rotation)
    // 2. Video -> Screen coords (apply user rotation)

    let total_rotation = (capture_orientation + device_orientation) % 4;

    // Calculate device logical size at current orientation
    let (device_w, device_h) = if device_orientation & 1 == 0 {
        (device_physical_size.0 as f32, device_physical_size.1 as f32)
    } else {
        (device_physical_size.1 as f32, device_physical_size.0 as f32)
    };

    let video_width = video_rect.width();
    let video_height = video_rect.height();

    // Determine original video dimensions before user rotation
    // If rotation is odd (90° or 270°), dimensions are swapped
    let (video_w, video_h) = if video_rotation & 1 == 0 {
        (video_width, video_height)
    } else {
        (video_height, video_width)
    };

    let device_x = device_pos.0 as f32;
    let device_y = device_pos.1 as f32;

    // Step 1: Inverse total rotation to get video original coordinates
    let (video_x, video_y) = match total_rotation {
        // 0 degrees
        0 => {
            let scale_x = video_w / device_w;
            let scale_y = video_h / device_h;
            (device_x * scale_x, device_y * scale_y)
        }
        // 90 degrees clockwise - inverse transform
        1 => {
            let scale_x = video_h / device_w;
            let scale_y = video_w / device_h;
            ((device_h - device_y) * scale_y, device_x * scale_x)
        }
        // 180 degrees
        2 => {
            let scale_x = video_w / device_w;
            let scale_y = video_h / device_h;
            (
                (device_w - device_x) * scale_x,
                (device_h - device_y) * scale_y,
            )
        }
        // 270 degrees clockwise
        3 => {
            let scale_x = video_h / device_w;
            let scale_y = video_w / device_h;
            (device_y * scale_y, (device_w - device_x) * scale_x)
        }
        _ => return None,
    };

    // Step 2: Apply user rotation to transform from video original coords to display coords
    let (rel_x, rel_y) = match video_rotation % 4 {
        // 0 degrees - no rotation
        0 => (video_x, video_y),

        // 90 degrees clockwise rotation
        // Transform: (x, y) => (H - y, x)
        1 => (video_h - video_y, video_x),

        // 180 degrees rotation
        // Transform: (x, y) => (W - x, H - y)
        2 => (video_w - video_x, video_h - video_y),

        // 270 degrees clockwise rotation
        // Transform: (x, y) => (y, W - x)
        3 => (video_y, video_w - video_x),

        _ => return None,
    };

    // Convert to screen coordinates
    let screen_x = video_rect.left() + rel_x;
    let screen_y = video_rect.top() + rel_y;

    Some(Pos2::new(screen_x, screen_y))
}

/// Find the nearest mapping to a given position
pub fn find_nearest_mapping(
    device_pos: (u32, u32),
    mappings: &KeyMapping,
) -> Option<(Key, (u32, u32))> {
    let mut nearest: Option<(Key, (u32, u32), f32)> = None;

    for (key, action) in mappings.read().iter() {
        if let Some((x, y)) = extract_position(action) {
            let dx = device_pos.0 as f32 - x as f32;
            let dy = device_pos.1 as f32 - y as f32;
            let distance = (dx * dx + dy * dy).sqrt();

            if let Some((_, _, min_dist)) = nearest {
                if distance < min_dist {
                    nearest = Some((*key, (x, y), distance));
                }
            } else {
                nearest = Some((*key, (x, y), distance));
            }
        }
    }

    nearest.map(|(key, pos, _)| (key, pos))
}

/// Extract position from AdbAction
pub fn extract_position(action: &AdbAction) -> Option<(u32, u32)> {
    match action {
        AdbAction::Tap { x, y } => Some((*x, *y)),
        AdbAction::TouchDown { x, y } => Some((*x, *y)),
        _ => None,
    }
}
