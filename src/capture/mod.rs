// SPDX-License-Identifier: MIT OR Apache-2.0

pub mod recorder;
pub mod screenshot;

use std::path::PathBuf;

#[derive(Debug)]
pub enum CaptureEvent {
    ScreenshotSaved(PathBuf),
    RecordingSaved(PathBuf),
    /// A screenshot operation failed.
    ScreenshotError(String),
    /// A recording operation failed or was terminated with an error.
    RecordingError(String),
}
