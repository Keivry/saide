// SPDX-License-Identifier: MIT OR Apache-2.0

//! Coordinate system transformations for SAide
//!
//! This module implements 3 coordinate systems:
//! 1. MappingCoordSys: Normalized (0.0-1.0) coordinate system bound to display rotation, stored in
//!    config files and key mapping profiles
//! 2. VisualCoordSys: Screen/UI coordinate system relative to video display rect
//! 3. ScrcpyCoordSys: Video pixel coordinate system for scrcpy control protocol
//!
//! Transformation chain:
//! - Config loading: MappingCoordSys → cache as ScrcpyCoordSys when profile activated
//! - UI display: MappingCoordSys ↔ ScrcpyCoordSys ↔ VisualCoordSys
//! - Input events: VisualCoordSys → ScrcpyCoordSys (for control) or VisualCoordSys →
//!   MappingCoordSys (for config editing)

mod mapping;
mod scrcpy;
pub mod types;
mod visual;

#[cfg(test)]
mod tests;

pub use {
    mapping::MappingCoordSys,
    scrcpy::ScrcpyCoordSys,
    types::{MappingPos, ScrcpyPos, VisualPos},
    visual::VisualCoordSys,
};
