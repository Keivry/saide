// SPDX-License-Identifier: MIT OR Apache-2.0

use {
    crate::{error::Result, scrcpy::codec_probe::EncoderProfileDatabase},
    adbshell::AdbShell,
    scrcpy_protocol::{
        ScrcpyError,
        SCRCPY_SERVER_CLASS_NAME,
        SCRCPY_SERVER_PATH,
        SCRCPY_SERVER_VERSION,
    },
    std::{path::Path, process::Child},
    tracing::{debug, info},
};

/// Parameters passed to the scrcpy server at startup.
///
/// Build a value with [`Default::default`] for sensible defaults, then
/// override individual fields, or use [`ServerParams::for_device`] to
/// automatically populate encoder settings from a cached profile database.
#[derive(Debug, Clone)]
pub struct ServerParams {
    /// Session identifier sent in the `scid=` server argument (31-bit, Java
    /// signed-int safe).  Generated automatically by [`Default::default`].
    pub scid: u32,
    pub video: bool,
    pub video_codec: String,
    pub video_bit_rate: u32,
    pub max_size: u16,
    pub max_fps: u16,
    pub audio: bool,
    pub audio_codec: String,
    pub audio_source: String,
    pub control: bool,
    pub tunnel_forward: bool,
    pub send_dummy_byte: bool,
    pub send_frame_meta: bool,
    pub send_codec_meta: bool,
    pub send_device_meta: bool,
    pub log_level: String,
    pub video_encoder: Option<String>,
    pub video_codec_options: Option<String>,
    pub capture_orientation: Option<u32>,
    pub stay_awake: bool,
    pub screen_off_timeout: Option<i32>,
}

fn generate_scid() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let pid = std::process::id();
    (nanos ^ pid.rotate_left(16)) & 0x7FFF_FFFF
}

impl Default for ServerParams {
    fn default() -> Self {
        Self {
            scid: generate_scid(),
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
            capture_orientation: None,
            stay_awake: true,
            screen_off_timeout: None,
        }
    }
}

impl ServerParams {
    pub fn for_device(serial: &str, config_dir: &Path) -> Result<Self> {
        let db = EncoderProfileDatabase::load(config_dir)?;
        let mut params = Self::default();

        if let Some(profile) = db.get(serial) {
            params.video_codec_options = profile.optimal_config.clone();
            params.video_encoder = profile.video_encoder.clone();
        }

        Ok(params)
    }
}

pub fn push_server(serial: &str, server_jar_path: &str) -> Result<()> {
    debug!("Pushing server to device: {}", serial);
    AdbShell::push_file(serial, server_jar_path, SCRCPY_SERVER_PATH)?;
    info!("Server pushed to {}", SCRCPY_SERVER_PATH);
    Ok(())
}

pub fn start_server(serial: &str, params: &ServerParams) -> Result<Child> {
    let args = build_server_args(params);
    info!("Starting server with args: {}", args.join(" "));
    AdbShell::execute_jar(
        serial,
        SCRCPY_SERVER_PATH,
        "/",
        SCRCPY_SERVER_CLASS_NAME,
        SCRCPY_SERVER_VERSION,
        &args,
    )
    .map_err(Into::into)
}

fn build_server_args(params: &ServerParams) -> Vec<String> {
    let mut args = Vec::new();

    args.push(format!("scid={:08x}", params.scid));
    args.push(format!("log_level={}", params.log_level));

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
    }
    if let Some(ref options) = params.video_codec_options {
        args.push(format!("video_codec_options={}", options));
    }
    if let Some(orientation) = params.capture_orientation {
        args.push(format!("capture_orientation=@{}", orientation * 90));
    }

    if !params.audio {
        args.push("audio=false".to_string());
    } else {
        if params.audio_codec != "opus" {
            args.push(format!("audio_codec={}", params.audio_codec));
        }
        if params.audio_source != "output" {
            args.push(format!("audio_source={}", params.audio_source));
        }
        args.push("audio=true".to_string());
    }

    if !params.control {
        args.push("control=false".to_string());
    }
    if params.tunnel_forward {
        args.push("tunnel_forward=true".to_string());
    }
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
        args.push("send_device_meta=false".to_string());
    }
    if params.stay_awake {
        args.push("stay_awake=true".to_string());
    }
    if let Some(timeout) = params.screen_off_timeout {
        args.push(format!("power_off_on_close={}", timeout));
    }

    args
}

pub fn get_socket_name(scid: u32) -> String { format!("scrcpy_{:08x}", scid) }

pub fn read_device_meta<R: std::io::Read>(stream: &mut R) -> Result<String> {
    let mut buffer = [0u8; 64];
    stream.read_exact(&mut buffer)?;
    let len = buffer.iter().position(|&b| b == 0).unwrap_or(64);
    String::from_utf8(buffer[..len].to_vec())
        .map_err(|_| ScrcpyError::Other("Invalid UTF-8 in device name".to_string()).into())
}
