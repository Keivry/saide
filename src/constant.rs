// SPDX-License-Identifier: MIT OR Apache-2.0

//! Constants used throughout the application
//!
//! This module defines various constants used in the SAide application,
//! including default paths, version strings, server configurations, and audio settings.

use {directories::ProjectDirs, std::path::PathBuf};

/// Get project configuration directory
/// Uses standard OS-specific config directory (e.g. %APPDATA% on Windows, ~/.config on Linux)
/// Falls back to a temporary directory if ProjectDirs is unavailable (e.g. in restricted
/// environments)
pub fn config_dir() -> PathBuf {
    directories::ProjectDirs::from("io", "keivry", "saide")
        .map(|dirs| dirs.config_dir().to_owned())
        .unwrap_or_else(fallback_config_dir)
}

/// Get the expected path to the configuration file (config.toml)
/// This is typically located in the OS-specific config directory (e.g. %APPDATA%\Saide\config.toml
/// on Windows, ~/.config/saide/config.toml on Linux), but falls back to a temporary directory if
/// ProjectDirs is unavailable
pub fn config_file() -> PathBuf { config_dir().join("config.toml") }

/// Get project data directory
/// Uses standard OS-specific data directory (e.g. %APPDATA% on Windows, ~/.local/share on Linux)
/// Falls back to a temporary directory if ProjectDirs is unavailable (e.g. in restricted
/// environments)
pub fn data_dir() -> PathBuf {
    directories::ProjectDirs::from("io", "keivry", "saide")
        .map(|dirs| dirs.data_dir().to_owned())
        .unwrap_or_else(fallback_data_dir)
}

/// Fallback configuration path (used when ProjectDirs unavailable)
/// Uses /tmp for Linux/Unix, %TEMP% for Windows
fn fallback_config_dir() -> PathBuf { std::env::temp_dir().join("saide") }

/// Fallback data directory path
fn fallback_data_dir() -> PathBuf { std::env::temp_dir().join("saide") }

/// Get the expected filename of the scrcpy server JAR file based on the version string
pub fn scrcpy_server_filename() -> String {
    format!("scrcpy-server-{}", SCRCPY_SERVER_VERSION_STRING)
}

/// Resolve the path to the scrcpy server JAR file
///
/// Checks multiple locations in order:
/// 1. OS-specific data directory (e.g. %APPDATA% on Windows, ~/.local/share on Linux)
/// 2. Current working directory
///
/// If the file is not found in either location, returns the expected path in the current directory
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

/// Jitter factor for custom keymapping position adjustments
/// This small value is added to the position of each keymapping to prevent
/// multiple keymappings from having the exact same position.
pub const CUSTOM_KEYMAPPING_POS_JITTER: f32 = 0.005;
