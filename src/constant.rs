//! Constants used throughout the application
//!
//! This module defines various constants used in the SAide application,
//! including default paths, version strings, server configurations, and audio settings.

use {directories::ProjectDirs, lazy_static::lazy_static, std::path::PathBuf};

lazy_static! {
    /// Project directories for SAide application
    static ref PROJECT_DIR: ProjectDirs =
        ProjectDirs::from("io", "keivry", "saide").expect("Failed to determine project directories");

    /// Default configuration file path
    /// E.g., on Linux, this would typically be "~/.config/saide/config.toml"
    /// on Windows, it would be "C:\Users\<User>\AppData\Roaming\saide\config.toml"
    /// on macOS, it would be "~/Library/Application Support/saide/config.toml"
    pub static ref CONFIG_PATH: PathBuf = PROJECT_DIR.config_dir().join("config.toml");

    /// Data directory path
    /// E.g., on Linux, this would typically be "~/.local/share/saide/"
    /// on Windows, it would be "C:\Users\<User>\AppData\Roaming\saide\Data"
    /// on macOS, it would be "~/Library/Application Support/saide/"
    pub static ref DATA_PATH: PathBuf = PROJECT_DIR.data_dir().to_owned();
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
