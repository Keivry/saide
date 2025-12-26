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
/// 128 frames = 2.67ms @ 48kHz (ultra-low latency)
pub const AUDIO_BUFFER_FRAMES: usize = 128;

/// Ring buffer capacity (total samples, interleaved)
/// Opus frame: ~1920 samples (20ms @ 48kHz stereo)
/// Capacity should be at least 2x the frame size to account for jitter
pub const AUDIO_RING_CAPACITY: usize = 5760;
