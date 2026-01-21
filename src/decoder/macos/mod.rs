//! macOS platform video decoders
//!
//! This module provides macOS-specific hardware-accelerated video decoding
//! implementations using VideoToolbox framework.

#[cfg(target_os = "macos")]
pub mod vt;

#[cfg(target_os = "macos")]
pub use vt::VtDecoder;
