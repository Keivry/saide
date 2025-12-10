//! Scrcpy Server Management
//!
//! Handles server deployment and process lifecycle.
//! Reference: scrcpy/app/src/server.c

use {
    anyhow::{Context, Result},
    std::process::{Command, Stdio},
    tracing::{debug, info},
};

/// Server JAR file path on device
const DEVICE_SERVER_PATH: &str = "/data/local/tmp/scrcpy-server.jar";

/// Device name field length (as per DesktopConnection.java)

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
}

impl Default for ServerParams {
    fn default() -> Self {
        Self {
            // Use only 31 bits to avoid signed int issues on Java side
            scid: rand::random::<u32>() & 0x7FFF_FFFF,
            video: true,
            video_codec: "h264".to_string(),
            video_bit_rate: 8_000_000,
            max_size: 1600,
            max_fps: 60,
            audio: false,
            audio_codec: "opus".to_string(),
            control: true,
            tunnel_forward: false,
            send_dummy_byte: true,
            send_frame_meta: true,
            send_codec_meta: false,
            send_device_meta: true, // Default is true in scrcpy
            log_level: "info".to_string(),
        }
    }
}

/// Push server JAR to device
pub fn push_server(serial: &str, server_jar_path: &str) -> Result<()> {
    debug!("Pushing server to device: {}", serial);

    let status = Command::new("adb")
        .args(["-s", serial, "push", server_jar_path, DEVICE_SERVER_PATH])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .status()
        .context("Failed to execute adb push")?;

    if !status.success() {
        anyhow::bail!("Failed to push server to device");
    }

    info!("Server pushed to {}", DEVICE_SERVER_PATH);
    Ok(())
}

/// Build server command arguments
fn build_server_args(params: &ServerParams) -> Vec<String> {
    let mut args = vec![
        "shell".to_string(),
        format!("CLASSPATH={}", DEVICE_SERVER_PATH),
        "app_process".to_string(),
        "/".to_string(),
        "com.genymobile.scrcpy.Server".to_string(),
        "3.3.3".to_string(), // Version
    ];

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

    // Audio parameters
    if !params.audio {
        args.push("audio=false".to_string());
    } else if params.audio_codec != "opus" {
        args.push(format!("audio_codec={}", params.audio_codec));
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

    args
}

/// Start scrcpy server process
///
/// Returns the spawned process handle
pub fn start_server(serial: &str, params: &ServerParams) -> Result<std::process::Child> {
    let args = build_server_args(params);

    debug!("Starting server with scid={:08x}", params.scid);
    debug!("Server command: adb -s {} {}", serial, args.join(" "));

    let mut cmd = Command::new("adb");
    cmd.arg("-s").arg(serial);
    cmd.args(&args);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let child = cmd.spawn().context("Failed to spawn server process")?;

    info!("Server process started (scid={:08x})", params.scid);
    Ok(child)
}

/// Get socket name from scid (as per DesktopConnection.java)
pub fn get_socket_name(scid: u32) -> String {
    format!("scrcpy_{:08x}", scid)
}

/// Read device metadata from video stream
///
/// Server sends 64-byte device name at the beginning of video stream
/// if send_device_meta=true (default)
pub fn read_device_meta<R: std::io::Read>(stream: &mut R) -> Result<String> {
    let mut buffer = [0u8; 64]; // DEVICE_NAME_FIELD_LENGTH
    stream
        .read_exact(&mut buffer)
        .context("Failed to read device metadata")?;

    // Find null terminator
    let len = buffer.iter().position(|&b| b == 0).unwrap_or(64);

    String::from_utf8(buffer[..len].to_vec()).context("Invalid UTF-8 in device name")
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
            max_size: 1600,
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
        assert!(args.contains(&"max_size=1600".to_string()));
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
