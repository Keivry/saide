//! Utility functions for mapping lookups
//!
//! This module provides functions to find the nearest key mapping
//! to a given position and to extract positions from mapping actions.

use crate::{
    config::mapping::{Key, KeyMapping, MappingAction},
    core::coords::MappingPos,
};

/// Find the nearest mapping to a given source position
pub fn find_nearest_mapping(
    source: &MappingPos,
    mappings: &KeyMapping,
) -> Option<(Key, MappingPos)> {
    let mut nearest: Option<(Key, MappingPos, f32)> = None;

    for (key, action) in mappings.iter() {
        if let Some(p) = extract_position(action) {
            let dx = source.x - p.x;
            let dy = source.y - p.y;
            let distance = (dx * dx + dy * dy).sqrt();

            if let Some((_, _, min_dist)) = nearest {
                if distance < min_dist {
                    nearest = Some((*key, p, distance));
                }
            } else {
                nearest = Some((*key, p, distance));
            }
        }
    }

    nearest.map(|(k, p, _)| (k, p))
}

/// Extract position from a mapping action, if applicable
pub fn extract_position(action: &MappingAction) -> Option<MappingPos> {
    match action {
        MappingAction::Tap { pos } => Some(*pos),
        MappingAction::TouchDown { pos } => Some(*pos),
        _ => None,
    }
}
