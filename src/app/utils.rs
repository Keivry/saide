/// Utility functions for mapping lookups
use crate::config::mapping::{InputAction, Key, KeyMapping};

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
