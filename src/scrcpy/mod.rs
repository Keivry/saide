// SPDX-License-Identifier: MIT OR Apache-2.0

//! SAide-side scrcpy runtime modules.
//!
//! This module groups the higher-level, SAide-specific scrcpy runtime
//! components that depend on `adbshell` and `crossbeam-channel` directly.
//! Pure protocol types remain in the `scrcpy` crate.

pub mod codec_probe;
pub mod connection;
pub mod hwcodec;
pub mod server;
