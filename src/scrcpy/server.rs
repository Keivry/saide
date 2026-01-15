//! Scrcpy Server Management
//!
//! Handles server deployment and process lifecycle.
//! Reference: scrcpy/app/src/server.c

use {
    super::codec_probe::ProfileDatabase,
    crate::{
        GpuType,
        constant::{SCRCPY_SERVER_CLASS_NAME, SCRCPY_SERVER_PATH, SCRCPY_SERVER_VERSION},
        controller::AdbShell,
        detect_gpu,
        error::{Result, SAideError},
    },
    std::process::Child,
    tracing::{debug, info},
};

/// Device name field length (as per DesktopConnection.java)
///
/// Server configuration parameters
#[derive(Debug, Clone)]
pub struct ServerParams {
    /// Session ID (8-digit hex)
    pub scid: u32,

    /// Enable video streaming
    pub video: bool,

    /// Video codec (h264, h265, av1)
    pub video_codec: String,

    /// Video bit rate in bps
    pub video_bit_rate: u32,

    /// Maximum video dimension
    pub max_size: u16,

    /// Maximum FPS
    pub max_fps: u16,

    /// Enable audio streaming
    pub audio: bool,

    /// Audio codec (opus, aac, flac, raw)
    pub audio_codec: String,

    /// Audio source (output, playback, mic, etc.)
    /// - output: REMOTE_SUBMIX (system audio routing, may miss media)
    /// - playback: AudioPlaybackCapture (Android 10+, captures all playback)
    /// - mic: Microphone input
    pub audio_source: String,

    /// Enable control channel
    pub control: bool,

    /// Use tunnel forward instead of reverse
    pub tunnel_forward: bool,

    /// Send dummy byte for connection detection
    pub send_dummy_byte: bool,

    /// Send frame metadata (12-byte header)
    pub send_frame_meta: bool,

    /// Send codec metadata (SPS/PPS)
    pub send_codec_meta: bool,

    /// Send device metadata (device name, 64 bytes)
    pub send_device_meta: bool,

    /// Log level (verbose, debug, info, warn, error)
    pub log_level: String,

    /// Video encoder name (optional, e.g., "c2.android.avc.encoder")
    pub video_encoder: Option<String>,

    /// Video codec options (e.g., "profile=1,level=1" for low latency)
    /// Format: key[:type]=value[,...]
    /// Types: int (default), long, float, string
    pub video_codec_options: Option<String>,

    /// Lock capture orientation (0-3 for natural/90/180/270, or @0-@3 for absolute)
    /// When set, prevents resolution changes on device rotation
    /// Useful for hardware decoders (NVDEC) that can't handle dynamic resolution
    pub capture_orientation: Option<String>,

    /// Keep device awake (prevent auto-sleep)
    pub stay_awake: bool,

    /// Turn off screen timeout in milliseconds (-1 = immediately, 0 = no timeout)
    pub screen_off_timeout: Option<i32>,
}

impl Default for ServerParams {
    fn default() -> Self {
        Self {
            scid: rand::random::<u32>() & 0x7FFF_FFFF,
            video: true,
            video_codec: "h264".to_string(),
            video_bit_rate: 8_000_000,
            max_size: 1920,
            max_fps: 60,
            audio: false,
            audio_codec: "opus".to_string(),
            audio_source: "output".to_string(),
            control: true,
            tunnel_forward: false,
            send_dummy_byte: true,
            send_frame_meta: true,
            send_codec_meta: true,
            send_device_meta: true,
            log_level: "info".to_string(),
            video_encoder: None,

            video_codec_options: None,

            // Auto (follows device rotation)
            capture_orientation: None,

            // Keep device awake by default
            stay_awake: true,

            // Don't turn off screen by default
            screen_off_timeout: None,
        }
    }
}

impl ServerParams {
    /// Check if should lock capture orientation for NVDEC
    ///
    /// Strategy: Always lock orientation when using NVDEC to prevent resolution changes
    /// Benefits:
    /// - Avoid decoder rebuild overhead (~200ms + brief black screen)
    /// - No need for prepend-sps-pps-to-idr-frames=1 (compatibility issues)
    /// - More stable and predictable behavior
    /// - Works on all devices
    ///
    /// Returns true if NVDEC is likely to be used (NVIDIA GPU detected)
    pub fn should_lock_orientation_for_nvdec() -> bool {
        if let GpuType::Nvidia = detect_gpu() {
            info!("NVIDIA GPU detected, will lock capture orientation for NVDEC");
            return true;
        }
        false
    }

    /// Create params with device-specific codec options
    ///
    /// Loads from cache if available, otherwise uses defaults
    pub fn for_device(serial: &str) -> Result<Self> {
        let db = ProfileDatabase::load()?;
        let mut params = Self::default();

        if let Some(profile) = db.get(serial) {
            params.video_codec_options = profile.optimal_config.clone();
            params.video_encoder = profile.video_encoder.clone();
            tracing::info!(
                "Loaded cached codec profile for {}: encoder={:?}, options={:?}",
                serial,
                params.video_encoder,
                params.video_codec_options
            );
        } else {
            tracing::warn!(
                "No cached profile for {}, using defaults. Run `probe_codec` to optimize.",
                serial
            );
        }

        Ok(params)
    }
}

/// Push server JAR to device
pub fn push_server(serial: &str, server_jar_path: &str) -> Result<()> {
    debug!("Pushing server to device: {}", serial);

    AdbShell::push_file(serial, server_jar_path, SCRCPY_SERVER_PATH)?;

    info!("Server pushed to {}", SCRCPY_SERVER_PATH);
    Ok(())
}

/// Start scrcpy server process
///
/// Returns the spawned process handle
pub fn start_server(serial: &str, params: &ServerParams) -> Result<Child> {
    let args = build_server_args(params);

    info!("Starting server with scid={:08x}", params.scid);
    info!("Server command: adb -s {} {}", serial, args.join(" "));

    let child = AdbShell::execute_jar(
        serial,
        SCRCPY_SERVER_PATH,
        "/",
        SCRCPY_SERVER_CLASS_NAME,
        SCRCPY_SERVER_VERSION,
        &args,
    )?;

    info!("Server process started (scid={:08x})", params.scid);
    Ok(child)
}

/// Build server command arguments
fn build_server_args(params: &ServerParams) -> Vec<String> {
    let mut args = Vec::new();

    // Required parameters
    args.push(format!("scid={:08x}", params.scid));
    args.push(format!("log_level={}", params.log_level));

    // Video parameters
    if !params.video {
        args.push("video=false".to_string());
    } else if params.video_codec != "h264" {
        args.push(format!("video_codec={}", params.video_codec));
    }
    if params.video && params.video_bit_rate > 0 {
        args.push(format!("video_bit_rate={}", params.video_bit_rate));
    }
    if params.video && params.max_size > 0 {
        args.push(format!("max_size={}", params.max_size));
    }
    if params.video && params.max_fps > 0 {
        args.push(format!("max_fps={}", params.max_fps));
    }
    if let Some(ref encoder) = params.video_encoder {
        args.push(format!("video_encoder={}", encoder));
        info!("Using video encoder: {}", encoder);
    }

    // 🚀 LATENCY OPTIMIZATION: Codec options
    if let Some(ref options) = params.video_codec_options {
        args.push(format!("video_codec_options={}", options));
        info!("Using video codec options: {}", options);
    }

    // 🔒 NVDEC WORKAROUND: Lock capture orientation to prevent resolution changes
    if let Some(ref orientation) = params.capture_orientation {
        args.push(format!("capture_orientation={}", orientation));
        info!(
            "Locked capture orientation: {} (prevents resolution changes)",
            orientation
        );
    }

    // Audio parameters
    if !params.audio {
        args.push("audio=false".to_string());
    } else {
        // Audio is enabled
        if params.audio_codec != "opus" {
            args.push(format!("audio_codec={}", params.audio_codec));
        }
        if params.audio_source != "output" {
            // Only set if not default
            args.push(format!("audio_source={}", params.audio_source));
        }
        args.push("audio=true".to_string());
    }

    // Control
    if !params.control {
        args.push("control=false".to_string());
    }

    // Tunnel mode
    if params.tunnel_forward {
        args.push("tunnel_forward=true".to_string());
    }

    // Metadata flags
    if params.send_dummy_byte {
        args.push("send_dummy_byte=true".to_string());
    }
    if params.send_frame_meta {
        args.push("send_frame_meta=true".to_string());
    }
    if params.send_codec_meta {
        args.push("send_codec_meta=true".to_string());
    } else {
        args.push("send_codec_meta=false".to_string());
    }
    if !params.send_device_meta {
        // send_device_meta defaults to true, only set if false
        args.push("send_device_meta=false".to_string());
    }

    // Power management options
    if params.stay_awake {
        args.push("stay_awake=true".to_string());
    }
    if let Some(timeout) = params.screen_off_timeout {
        args.push(format!("power_off_on_close={}", timeout));
    }

    args
}

/// Get socket name from scid (as per DesktopConnection.java)
pub fn get_socket_name(scid: u32) -> String { format!("scrcpy_{:08x}", scid) }

/// Read device metadata from video stream
///
/// Server sends 64-byte device name at the beginning of video stream
/// if send_device_meta=true (default)
pub fn read_device_meta<R: std::io::Read>(stream: &mut R) -> Result<String> {
    let mut buffer = [0u8; 64]; // DEVICE_NAME_FIELD_LENGTH
    stream.read_exact(&mut buffer)?;

    // Find null terminator
    let len = buffer.iter().position(|&b| b == 0).unwrap_or(64);

    String::from_utf8(buffer[..len].to_vec())
        .map_err(|_| SAideError::Other("Invalid UTF-8 in device name".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socket_name_generation() {
        assert_eq!(get_socket_name(0x12345678), "scrcpy_12345678");
        assert_eq!(get_socket_name(0xABCDEF00), "scrcpy_abcdef00");
        assert_eq!(get_socket_name(0x00000001), "scrcpy_00000001");
    }

    #[test]
    fn test_build_server_args() {
        let params = ServerParams {
            scid: 0x12345678,
            video: true,
            video_codec: "h264".to_string(),
            video_bit_rate: 8_000_000,
            max_size: 1920,
            max_fps: 60,
            audio: false,
            control: true,
            tunnel_forward: false,
            send_dummy_byte: true,
            send_frame_meta: true,
            send_codec_meta: false,
            log_level: "info".to_string(),
            ..Default::default()
        };

        let args = build_server_args(&params);

        assert!(args.contains(&"scid=12345678".to_string()));
        assert!(args.contains(&"log_level=info".to_string()));
        assert!(args.contains(&"video_bit_rate=8000000".to_string()));
        assert!(args.contains(&"max_size=1920".to_string()));
        assert!(args.contains(&"max_fps=60".to_string()));
        assert!(args.contains(&"audio=false".to_string()));
        assert!(args.contains(&"send_dummy_byte=true".to_string()));
        assert!(args.contains(&"send_frame_meta=true".to_string()));
    }

    #[test]
    fn test_minimal_params() {
        let params = ServerParams {
            scid: 1,
            video: false,
            audio: false,
            control: false,
            ..Default::default()
        };

        let args = build_server_args(&params);

        assert!(args.contains(&"video=false".to_string()));
        assert!(args.contains(&"audio=false".to_string()));
        assert!(args.contains(&"control=false".to_string()));
    }
}
