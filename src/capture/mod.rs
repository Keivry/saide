// SPDX-License-Identifier: MIT OR Apache-2.0

//! Screenshot and screen recording support.
//!
//! This module exposes two sub-modules:
//!
//! - [`screenshot`]: captures the current video frame as a PNG file.
//! - [`recorder`]: records an H.264/AAC MP4 file from the live video and optional audio stream.
//!
//! Both operations are asynchronous — they run in background threads and
//! report their outcome through a [`CaptureEvent`] sent over a channel.

pub mod recorder;
pub mod screenshot;

use {egui_event::Event, std::path::PathBuf};

/// Events emitted by capture operations once they complete.
#[derive(Debug, Event)]
pub enum CaptureEvent {
    /// A screenshot was written to the given path.
    ScreenshotSaved(PathBuf),
    /// A screen recording was written to the given path.
    RecordingSaved(PathBuf),
    /// A screenshot operation failed.
    ScreenshotError(String),
    /// A recording operation failed or was terminated with an error.
    RecordingError(String),
}
