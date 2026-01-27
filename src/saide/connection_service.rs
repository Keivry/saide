//! Scrcpy connection service
//!
//! Handles establishing scrcpy connections, creating control senders,
//! and initializing input mappers (keyboard/mouse).

use {
    crate::{
        config::SAideConfig,
        controller::{control_sender::ControlSender, keyboard::KeyboardMapper, mouse::MouseMapper},
        decoder::AutoDecoder,
        error::{Result, SAideError},
        scrcpy::{connection::ScrcpyConnection, server::ServerParams},
    },
    std::{net::TcpStream, sync::Arc, thread},
    tokio_util::sync::CancellationToken,
    tracing::{debug, error, info},
};

/// Connection initialization result
pub struct ConnectionResult {
    pub connection: ScrcpyConnection,
    pub control_sender: ControlSender,
    pub video_stream: TcpStream,
    pub audio_stream: Option<TcpStream>,
    pub video_resolution: (u32, u32),
    pub device_name: Option<String>,
    pub audio_disabled_reason: Option<String>,
    pub capture_orientation: Option<u32>,
}

/// Connection service - manages scrcpy connection lifecycle
pub struct ConnectionService;

impl ConnectionService {
    /// Start scrcpy connection in background thread
    pub fn start<F>(serial: &str, config: Arc<SAideConfig>, on_ready: F, token: CancellationToken)
    where
        F: FnOnce(Result<ConnectionResult>) + Send + 'static,
    {
        let serial = serial.to_owned();
        thread::spawn(move || {
            info!("Establishing scrcpy connection...");
            let result = Self::establish_connection(&serial, config, token);
            on_ready(result);
        });
    }

    /// Establish scrcpy connection (blocking)
    fn establish_connection(
        serial: &str,
        config: Arc<SAideConfig>,
        token: CancellationToken,
    ) -> Result<ConnectionResult> {
        if token.is_cancelled() {
            info!("Scrcpy connection initialization cancelled");
            return Err(SAideError::Cancelled);
        }

        debug!("Connecting to device: {}", serial);

        let capture_orientation = config.scrcpy.video.capture_orientation.or_else(|| {
            if AutoDecoder::needs_orientation_lock(config.gpu.hwdecode) {
                Some(0)
            } else {
                None
            }
        });

        // Establish ScrcpyConnection (blocking)
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| {
                error!("Failed to create Tokio runtime: {}", e);
                SAideError::Other(format!("Failed to create Tokio runtime: {}", e))
            })?;

        let mut connection = runtime.block_on(async {
            tokio::select! {
                _ = token.cancelled() => {
                    Err(SAideError::Cancelled)
                },
                conn = Self::scrcpy_connection(serial, config.clone(), capture_orientation) => {
                    conn
                }
            }
        })?;

        info!("ScrcpyConnection established successfully");

        let video_stream = connection
            .take_video_stream()
            .ok_or_else(|| SAideError::Other("Video stream not available".to_string()))?;
        let audio_stream = connection.take_audio_stream();
        let control_stream = connection
            .take_control_stream()
            .ok_or_else(|| SAideError::Other("Control stream not available".to_string()))?;
        let video_resolution = connection
            .video_resolution
            .ok_or_else(|| SAideError::Other("Video resolution not available".to_string()))?;
        let device_name = connection.device_name.clone();
        let audio_disabled_reason = connection.audio_disabled_reason.clone();

        let control_stream_clone = control_stream
            .try_clone()
            .map_err(|e| SAideError::Other(format!("Failed to clone control stream: {}", e)))?;

        let control_sender = ControlSender::new(
            control_stream_clone,
            video_resolution.0 as u16,
            video_resolution.1 as u16,
        );

        info!(
            "ControlSender created with resolution {}x{}, capture_orientation={:?}",
            video_resolution.0, video_resolution.1, capture_orientation
        );

        connection.set_control_stream(control_stream);

        Ok(ConnectionResult {
            connection,
            control_sender,
            video_stream,
            audio_stream,
            video_resolution,
            device_name,
            audio_disabled_reason,
            capture_orientation,
        })
    }

    /// Establish scrcpy connection with given config
    async fn scrcpy_connection(
        serial: &str,
        config: Arc<SAideConfig>,
        capture_orientation: Option<u32>,
    ) -> Result<ScrcpyConnection> {
        let server_path = &config.general.scrcpy_server;

        // Create server params from config
        let mut params = ServerParams::for_device(serial)?;

        // Apply config settings
        let bit_rate = {
            let rate_str = &config.scrcpy.video.bit_rate;
            let multiplier = if rate_str.ends_with('M') || rate_str.ends_with('m') {
                1_000_000
            } else if rate_str.ends_with('K') || rate_str.ends_with('k') {
                1_000
            } else {
                1
            };
            let num_str = rate_str.trim_end_matches(|c: char| !c.is_ascii_digit());
            num_str.parse::<u32>().unwrap_or(8) * multiplier
        };

        params.video = true;
        params.video_codec = config.scrcpy.video.codec.clone();
        params.video_bit_rate = bit_rate;
        params.max_size = config.scrcpy.video.max_size as u16;
        params.max_fps = config.scrcpy.video.max_fps as u16;
        params.audio = config.scrcpy.audio.enabled;
        params.audio_codec = config.scrcpy.audio.codec.clone();
        params.audio_source = config.scrcpy.audio.source.clone();
        params.control = true;
        params.send_device_meta = true;
        params.send_codec_meta = true;
        params.send_frame_meta = true;

        // Apply screen control options
        params.stay_awake = config.scrcpy.options.stay_awake;
        params.screen_off_timeout = if config.scrcpy.options.turn_screen_off {
            Some(-1) // Turn off immediately
        } else {
            None
        };

        // 🔒 NVDEC Optimization: Lock capture orientation to prevent resolution changes
        // Benefits:
        // - Avoid decoder rebuild overhead (~200ms + black screen)
        // - No need for prepend-sps-pps-to-idr-frames=1 (compatibility)
        // - More stable, works on all devices
        params.capture_orientation = capture_orientation;

        ScrcpyConnection::connect(serial, server_path, &config.general.bind_address, params)
            .map_err(|e| {
                error!("Failed to establish scrcpy connection: {}", e);
                e
            })
    }
}

/// Input manager - creates keyboard and mouse mappers
pub struct InputManager;

impl InputManager {
    /// Create keyboard mapper
    pub fn create_keyboard_mapper<F>(
        config: Arc<SAideConfig>,
        control_sender: ControlSender,
        capture_orientation: Option<u32>,
        on_ready: F,
    ) where
        F: FnOnce(KeyboardMapper) + Send + 'static,
    {
        thread::spawn(move || {
            let mapper =
                KeyboardMapper::new(config.mappings.clone(), control_sender, capture_orientation);
            debug!("Keyboard mapper initialized with ControlSender");
            on_ready(mapper);
        });
    }

    /// Create mouse mapper
    pub fn create_mouse_mapper<F>(
        config: Arc<SAideConfig>,
        control_sender: ControlSender,
        on_ready: F,
    ) where
        F: FnOnce(MouseMapper) + Send + 'static,
    {
        thread::spawn(move || {
            let mapper = MouseMapper::new(control_sender, config.input.clone());
            debug!("Mouse mapper initialized with ControlSender");
            on_ready(mapper);
        });
    }
}
