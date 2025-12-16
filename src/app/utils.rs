/// Utility functions for coordinate transformations and mapping lookups
use {
    crate::config::mapping::{InputAction, Key, KeyMapping},
    eframe::egui::{Pos2, Rect},
    tracing::trace,
};

pub struct CoordinatesTransformParams {
    pub video_rect: egui::Rect,
    pub video_rotation: u32,
    #[allow(dead_code)]
    pub device_physical_size: (u32, u32),
    #[allow(dead_code)]
    pub device_orientation: u32,
    #[allow(dead_code)]
    pub capture_orientation: u32,
}

/// Transform egui position to device logical coordinates for ADB input
///
/// Coordinate transformation chain:
/// 1. egui screen coords -> video display coords (considering user manual rotation)
/// 2. Inverse apply user rotation -> video original coords
/// 3. Scale to device size -> ADB logical coords
///
/// Note:
/// - scrcpy video orientation follows device rotation (not fixed!)
/// - When device rotates, video resolution changes to match
/// - So video coords = device current coords (just need scaling)
/// - ADB expects coords relative to device's current display orientation
///
/// DEPRECATED: No longer used, mapping config now uses video coordinates directly
#[allow(dead_code)]
pub fn screen_to_device_coords(
    pos: &egui::Pos2,
    trans_params: &CoordinatesTransformParams,
) -> Option<(u32, u32)> {
    let (
        video_rect,
        video_rotation,
        device_physical_size,
        device_orientation,
        _capture_orientation,
    ) = (
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

    trace!("== Coordinate Transform ==");
    trace!("Screen pos: ({:.1}, {:.1})", pos.x, pos.y);
    trace!("Video rect: {:?}", video_rect);
    trace!("Relative pos in video: ({:.1}, {:.1})", rel_x, rel_y);

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

    trace!(
        "Video original coords: ({:.1}, {:.1}) in {}x{}",
        video_x, video_y, video_w, video_h
    );
    trace!("Video rotation: {} (user manual)", video_rotation);
    trace!("Device orientation: {}", device_orientation);
    trace!("Device physical size: {:?}", device_physical_size);

    // Step 3: Scale to device size
    //
    // Important: scrcpy video resolution follows device orientation!
    // - Device orientation 0 (portrait): video is 540x1200
    // - Device orientation 1 (landscape): video is 1200x540
    // So video coords are already in device's current orientation
    // We only need to scale from video size to device size

    // Calculate device logical size at current orientation
    let (device_w, device_h) = if device_orientation & 1 == 0 {
        (device_physical_size.0 as f32, device_physical_size.1 as f32)
    } else {
        (device_physical_size.1 as f32, device_physical_size.0 as f32)
    };

    // Simple scaling (video orientation = device orientation)
    let scale_x = device_w / video_w;
    let scale_y = device_h / video_h;
    let (device_x, device_y) = (video_x * scale_x, video_y * scale_y);

    trace!(
        "Device logical size: {}x{}",
        device_w as u32, device_h as u32
    );
    trace!(
        "Device coords: ({:.1}, {:.1}) -> ({}, {})",
        device_x, device_y, device_x as u32, device_y as u32
    );

    Some((device_x as u32, device_y as u32))
}

/// Convert video percentage coordinates (0.0-1.0) to screen coordinates in video rect
///
/// This function accepts percentage coordinates from config/profile (0.0-1.0 range)
/// and converts them to screen coordinates for display purposes.
///
/// Percentage is relative to VIDEO coordinates, not device physical size.
pub fn device_to_screen_coords(
    video_percent: (f32, f32),
    transform_params: &CoordinatesTransformParams,
) -> Option<Pos2> {
    let video_rect = &transform_params.video_rect;
    let video_rotation = transform_params.video_rotation;

    let video_width = video_rect.width();
    let video_height = video_rect.height();

    // Determine original video dimensions before user rotation
    // If rotation is odd (90° or 270°), dimensions are swapped
    let (video_w, video_h) = if video_rotation & 1 == 0 {
        (video_width, video_height)
    } else {
        (video_height, video_width)
    };

    // Convert percentage to video original coordinates
    let video_x = video_percent.0 * video_w;
    let video_y = video_percent.1 * video_h;

    // Apply user rotation to transform from video original coords to display coords
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
///
/// Note: Coordinates in mappings could be either percentage (0.0-1.0) or pixels (after
/// convert_to_pixels) device_pos should be in the same coordinate space
pub fn find_nearest_mapping(
    device_pos: (f32, f32),
    mappings: &KeyMapping,
) -> Option<(Key, (f32, f32))> {
    let mut nearest: Option<(Key, (f32, f32), f32)> = None;

    for (key, action) in mappings.read().iter() {
        if let Some((x, y)) = extract_position(action) {
            let dx = device_pos.0 - x;
            let dy = device_pos.1 - y;
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

/// Extract position from InputAction (as f32, could be percentage or pixels)
pub fn extract_position(action: &InputAction) -> Option<(f32, f32)> {
    match action {
        InputAction::Tap { x, y } => Some((*x, *y)),
        InputAction::TouchDown { x, y } => Some((*x, *y)),
        _ => None,
    }
}

/// Transform egui position to video coordinates for scrcpy control channel
///
/// Scrcpy 控制通道期望：
/// - 坐标相对于视频分辨率（不是设备分辨率）
/// - 不需要考虑设备旋转（服务端会自动处理）
/// - screenSize = video resolution（当前视频分辨率）
///
/// 坐标转换链：
/// 1. egui 屏幕坐标 → 视频显示区域相对坐标
/// 2. 反向应用用户旋转 → 视频原始坐标
///
/// 返回：(x, y, screenWidth, screenHeight)
/// - x, y: 视频坐标（相对于视频分辨率）
/// - screenWidth, screenHeight: 视频原始尺寸（发送给服务端用于验证）
pub fn screen_to_video_coords(
    pos: &Pos2,
    video_rect: &Rect,
    video_rotation: u32,
) -> Option<(u32, u32, u16, u16)> {
    // Step 1: Get relative coordinates in video display rect
    let rel_x = pos.x - video_rect.left();
    let rel_y = pos.y - video_rect.top();

    // Check if position is within video rect
    if rel_x < 0.0 || rel_y < 0.0 {
        return None;
    }

    let display_width = video_rect.width();
    let display_height = video_rect.height();

    if rel_x > display_width || rel_y > display_height {
        return None;
    }

    trace!("== Video Coordinate Transform ==");
    trace!("Screen pos: ({:.1}, {:.1})", pos.x, pos.y);
    trace!("Video rect: {:?}", video_rect);
    trace!("Relative pos in display: ({:.1}, {:.1})", rel_x, rel_y);
    trace!("Video rotation (user manual): {}", video_rotation);

    // Step 2: Inverse apply user rotation to get video original coordinates
    //
    // User rotation transforms video original coords to display coords
    // We need to do the inverse transform
    //
    // Note: display_width/height are the dimensions AFTER rotation
    // Original video dimensions depend on rotation parity
    let (video_x, video_y, video_w, video_h) = match video_rotation % 4 {
        // 0 degrees - no rotation
        // Display: W×H, Original: W×H
        0 => (rel_x, rel_y, display_width, display_height),

        // 90 degrees clockwise rotation
        // Display: H×W, Original: W×H
        // Forward: (x, y) => (H - y, x)
        // Inverse: (x', y') => (y', W - x')
        1 => (rel_y, display_width - rel_x, display_height, display_width),

        // 180 degrees rotation
        // Display: W×H, Original: W×H
        // Forward: (x, y) => (W - x, H - y)
        // Inverse: (x', y') => (W - x', H - y')
        2 => (
            display_width - rel_x,
            display_height - rel_y,
            display_width,
            display_height,
        ),

        // 270 degrees clockwise rotation (= 90 degrees counter-clockwise)
        // Display: H×W, Original: W×H
        // Forward: (x, y) => (y, W - x)
        // Inverse: (x', y') => (H - y', x')
        3 => (display_height - rel_y, rel_x, display_height, display_width),

        _ => return None,
    };

    trace!(
        "Video original coords: ({:.1}, {:.1}) in {}x{}",
        video_x, video_y, video_w as u32, video_h as u32
    );

    // Return: (x, y, screenWidth, screenHeight)
    // These are the exact values that go into ControlMessage::Position
    Some((
        video_x as u32,
        video_y as u32,
        video_w as u16,
        video_h as u16,
    ))
}

/// Convert device coordinates (from config.toml) to video coordinates
///
/// 自定义映射中的坐标是设备坐标，需要转换为视频坐标
///
/// 转换链：
/// 1. 设备坐标（考虑设备旋转） → 设备 portrait 坐标
/// 2. 缩放（设备分辨率 → 视频分辨率）
/// 3. 应用 capture_orientation（如果视频本身被旋转捕获）
#[allow(dead_code)]
pub fn device_to_video_coords(
    device_x: u32,
    device_y: u32,
    device_physical_size: (u32, u32),
    device_orientation: u32,
    video_size: (u16, u16),
) -> (u32, u32, u16, u16) {
    // Step 1: Convert device coords to portrait orientation
    let (device_w, device_h) = if device_orientation & 1 == 0 {
        device_physical_size
    } else {
        (device_physical_size.1, device_physical_size.0)
    };

    let (portrait_x, portrait_y) = match device_orientation % 4 {
        0 => (device_x, device_y),
        1 => (device_y, device_w - device_x),
        2 => (device_w - device_x, device_h - device_y),
        3 => (device_h - device_y, device_x),
        _ => (device_x, device_y),
    };

    // Step 2: Scale to video resolution
    let scale_x = video_size.0 as f32 / device_physical_size.0 as f32;
    let scale_y = video_size.1 as f32 / device_physical_size.1 as f32;

    let video_x = (portrait_x as f32 * scale_x) as u32;
    let video_y = (portrait_y as f32 * scale_y) as u32;

    trace!(
        "Device→Video: ({}, {}) in {}x{} (orient={}) → ({}, {}) in {}x{}",
        device_x,
        device_y,
        device_w,
        device_h,
        device_orientation,
        video_x,
        video_y,
        video_size.0,
        video_size.1
    );

    (video_x, video_y, video_size.0, video_size.1)
}
