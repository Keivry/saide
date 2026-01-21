//! Windows platform video decoders
//!
//! This module provides Windows-specific hardware-accelerated video decoding
//! implementations using Media Foundation and D3D11VA.

#[cfg(target_os = "windows")]
pub mod mf;

#[cfg(target_os = "windows")]
pub use mf::MfDecoder;
