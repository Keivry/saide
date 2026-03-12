// SPDX-License-Identifier: MIT OR Apache-2.0

//! Constants used throughout the application
//!
//! This module defines various constants used in the SAide application,
//! including default paths, version strings, server configurations, and audio settings.

use {directories::ProjectDirs, std::path::PathBuf};

/// Get project configuration directory path
/// Returns None if unable to determine (e.g., Docker/sandbox environment)
pub fn config_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("io", "keivry", "saide")
        .map(|dirs| dirs.config_dir().join("config.toml"))
}

/// Get project data directory path
/// Returns None if unable to determine (e.g., Docker/sandbox environment)
pub fn data_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("io", "keivry", "saide").map(|dirs| dirs.data_dir().to_owned())
}

/// Fallback configuration path (used when ProjectDirs unavailable)
/// Uses /tmp for Linux/Unix, %TEMP% for Windows
pub fn fallback_config_path() -> PathBuf { std::env::temp_dir().join("saide").join("config.toml") }

/// Fallback data directory path
pub fn fallback_data_path() -> PathBuf { std::env::temp_dir().join("saide") }

pub fn scrcpy_server_filename() -> String {
    format!("scrcpy-server-{}", SCRCPY_SERVER_VERSION_STRING)
}

pub fn resolve_scrcpy_server_path() -> PathBuf {
    let filename = scrcpy_server_filename();

    if let Some(dir) = ProjectDirs::from("io", "keivry", "saide") {
        let path = dir.data_dir().join(filename.as_str());
        if path.is_file() {
            return path;
        }
    }

    let current_dir_path = PathBuf::from(filename.as_str());
    if current_dir_path.is_file() {
        return current_dir_path;
    }

    let legacy_repo_path = PathBuf::from("3rd-party").join(filename.as_str());
    if legacy_repo_path.is_file() {
        return legacy_repo_path;
    }

    current_dir_path
}

/// Version of the scrcpy server
pub const SCRCPY_SERVER_VERSION: &str = "3.3.3";

/// Version string of the scrcpy server
pub const SCRCPY_SERVER_VERSION_STRING: &str = "v3.3.3";

/// Java class name of the scrcpy server
pub const SCRCPY_SERVER_CLASS_NAME: &str = "com.genymobile.scrcpy.Server";

/// Path on the device where the scrcpy server will be pushed
pub const SCRCPY_SERVER_PATH: &str = "/data/local/tmp/scrcpy-server.jar";

/// Time to wait for the server to shut down gracefully (in milliseconds)
pub const GRACEFUL_WAIT_MS: u64 = 250;

/// Default port range for ADB reverse/forward
pub const DEFAULT_PORT_RANGE: (u16, u16) = (27183, 27199);

/// Audio playback buffer size (frames per CPAL callback)
/// Phase 3 optimization: Reduced to 64 frames (1.33ms @ 48kHz) for minimal latency
/// Previous: 128 frames (2.67ms)
/// Can be overridden via config.toml: [scrcpy.audio] buffer_frames = 64
pub const AUDIO_BUFFER_FRAMES: usize = 64;

/// Ring buffer capacity (total samples, interleaved)
/// Opus frame: ~1920 samples (20ms @ 48kHz stereo)
/// Capacity should be at least 2x the frame size to account for jitter
pub const AUDIO_RING_CAPACITY: usize = 5760;

/// Maximum allowed packet size for video/audio packets (10 MB)
/// Protects against DoS attacks via maliciously large packet_size values
/// Typical values: H.264 keyframe ~500KB, audio packet ~2KB
/// This limit allows 4K video keyframes while preventing memory exhaustion
pub const MAX_PACKET_SIZE: usize = 10 * 1024 * 1024;

/// Preset video resolution tiers (long edge in pixels)
/// Used for intelligent window resizing when video exceeds screen bounds
/// Format: sorted descending for efficient downsampling search
pub const VIDEO_RESOLUTION_TIERS: &[u32] = &[
    3840, // 4K UHD
    2560, // QHD / 1440p
    1920, // FHD / 1080p
    1600, // HD+
    1280, // HD / 720p
    960,  // qHD
    800,  // SVGA
    640,  // VGA
    480,  // HVGA
];
