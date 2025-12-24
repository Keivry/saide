//! Background initialization for scrcpy connection, device monitor, and input mappers
//!
//! This module handles the asynchronous initialization of the scrcpy connection,
//! device monitoring (rotation and input method state), and input mappers (keyboard and mouse).
//! It uses separate threads to perform these tasks and communicates results back
//! to the main application thread via channels.

use {
    crate::{
        config::SAideConfig,
        controller::{
            adb::{AdbShell, DeviceState},
            control_sender::ControlSender,
            keyboard::KeyboardMapper,
            mouse::MouseMapper,
        },
        error::{Result, SAideError},
        scrcpy::{connection::ScrcpyConnection, server::ServerParams},
    },
    crossbeam_channel::{Receiver, Sender, bounded},
    std::{
        net::TcpStream,
        process::Command,
        sync::Arc,
        thread,
        time::{Duration, Instant},
    },
    tokio_util::sync::CancellationToken,
    tracing::{debug, error, info, warn},
};

// Device monitor polling interval (ms)
pub const DEVICE_MONITOR_POLL_INTERVAL_MS: u64 = 1000;

// Allow buffering 3 rotation events to avoid blocking monitor thread
pub const DEVICE_MONITOR_CHANNEL_CAPACITY: usize = 3;
pub enum DeviceMonitorEvent {
    /// Device rotated event with new orientation (0-3), clockwise
    Rotated(u32),
    /// Device input method (IME) state changed, true = shown, false = hidden
    ImStateChanged(bool),
    /// Device went offline (USB disconnected or connection lost)
    DeviceOffline,
}

// Capacity for initialization result channel
pub const INIT_RESULT_CHANNEL_CAPACITY: usize = 4;
/// Initialization event
pub enum InitEvent {
    /// ScrcpyConnection established with streams
    ConnectionReady {
        connection: ScrcpyConnection,
        control_sender: ControlSender,
        video_stream: TcpStream,
        audio_stream: Option<TcpStream>,
        video_resolution: (u32, u32),
        device_name: Option<String>,
        audio_disabled_reason: Option<String>,
        capture_orientation: Option<u32>,
    },
    KeyboardMapper(KeyboardMapper),
    MouseMapper(MouseMapper),
    DeviceMonitor(Receiver<DeviceMonitorEvent>),
    Error(SAideError),
}

/// Background initialization function
pub fn start_initialization(
    serial: &str,
    config: Arc<SAideConfig>,
    tx: Sender<InitEvent>,
    cancellation_token: CancellationToken,
) {
    // Note: External scrcpy initialization removed - using internal StreamPlayer

    const ADB_SERVER_STARTUP_WAIT_MS: u64 = 500;
    const ADB_SERVER_CHECK_INTERVAL_MS: u64 = 50;

    // Delay to allow ADB server to be ready
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(ADB_SERVER_STARTUP_WAIT_MS) {
        // check if adb is responsive
        if Command::new("adb")
            .args(["shell", "echo", "ok"])
            .output()
            .is_ok()
        {
            break;
        }
        thread::sleep(Duration::from_millis(ADB_SERVER_CHECK_INTERVAL_MS));
    }

    // Device monitor initialization
    start_device_monitor(serial, tx.clone(), cancellation_token.clone());

    // Scrcpy connection initialization
    start_scrcpy_connection(serial, config, tx.clone(), cancellation_token.clone());
}

fn start_scrcpy_connection(
    serial: &str,
    config: Arc<SAideConfig>,
    tx: Sender<InitEvent>,
    token: CancellationToken,
) {
    // ScrcpyConnection initialization (async - moved to separate thread)
    // This will create mappers AFTER connection is established
    let serial = serial.to_owned();
    thread::spawn(move || -> Result<()> {
        info!("Establishing scrcpy connection...");

        // Get device serial
        if token.is_cancelled() {
            info!("Scrcpy connection initialization cancelled");
            return Ok(());
        }

        debug!("Connecting to device: {}", serial);

        // Check if we should lock orientation for NVDEC
        // TODO: User can override this in config later
        let capture_orientation = if ServerParams::should_lock_orientation_for_nvdec() {
            Some(0) // Lock to portrait (0°)
        } else {
            None
        };

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
                conn = scrcpy_connection(&serial, config.clone(), capture_orientation) => {
                    conn
                }
            }
        })?;

        info!("ScrcpyConnection established successfully");

        // Extract streams (but keep connection alive)
        let video_stream = connection
            .video_stream
            .take()
            .ok_or_else(|| SAideError::Other("Video stream not available".to_string()))?;
        let audio_stream = connection.audio_stream.take();
        let control_stream = connection
            .control_stream
            .take()
            .ok_or_else(|| SAideError::Other("Control stream not available".to_string()))?;
        let video_resolution = connection
            .video_resolution
            .ok_or_else(|| SAideError::Other("Video resolution not available".to_string()))?;
        let device_name = connection.device_name.clone();
        let audio_disabled_reason = connection.audio_disabled_reason.clone();

        // Create ControlSender with cloned stream
        // We need to clone the TcpStream to keep both in ControlSender and return original
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

        // Put the original control_stream back into connection to keep it alive
        connection.control_stream = Some(control_stream);

        // Send connection ready event (with connection to keep alive)
        tx.send(InitEvent::ConnectionReady {
            connection,
            control_sender: control_sender.clone(),
            video_stream,
            audio_stream,
            video_resolution,
            device_name,
            audio_disabled_reason,
            capture_orientation,
        })?;

        // Now create keyboard mapper (if enabled)
        if config.general.keyboard_enabled {
            let keyboard_mapper = KeyboardMapper::new(
                config.mappings.clone(),
                control_sender.clone(),
                capture_orientation,
            );
            debug!("Keyboard mapper initialized with ControlSender");
            tx.send(InitEvent::KeyboardMapper(keyboard_mapper))?;
        }

        // Create mouse mapper (if enabled)
        if config.general.mouse_enabled {
            let mouse_mapper = MouseMapper::new(control_sender.clone());
            debug!("Mouse mapper initialized with ControlSender");
            tx.send(InitEvent::MouseMapper(mouse_mapper))?;
        }

        Ok(())
    });
}

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
    if let Some(capture_orientation) = capture_orientation {
        // Lock to current device orientation (absolute)
        // @0 = lock to 0° (portrait), follows device's natural orientation
        params.capture_orientation = Some(format!("@{}", capture_orientation));
        info!("🔒 Locked capture orientation for NVDEC (prevents resolution changes)");
    }

    ScrcpyConnection::connect(serial, server_path, params)
        .await
        .map_err(|e| {
            error!("Failed to establish scrcpy connection: {}", e);
            e
        })
}

fn start_device_monitor(serial: &str, tx: Sender<InitEvent>, token: CancellationToken) {
    let serial = serial.to_owned();
    thread::spawn(move || -> Result<()> {
        info!("Starting device monitor thread...");

        if token.is_cancelled() {
            info!("Device monitor initialization cancelled");
            return Ok(());
        }

        // Create channel for rotation events
        let (event_tx, event_rx) = bounded::<DeviceMonitorEvent>(DEVICE_MONITOR_CHANNEL_CAPACITY);
        tx.send(InitEvent::DeviceMonitor(event_rx))?;

        start_device_state_monitor(&serial, event_tx.clone(), token.clone());
        start_ime_state_monitor(&serial, event_tx.clone(), token.clone());
        start_rotation_monitor(&serial, event_tx.clone(), token.clone());

        Ok(())
    });
}

fn start_device_state_monitor(
    serial: &str,
    tx: Sender<DeviceMonitorEvent>,
    token: CancellationToken,
) {
    thread::spawn(move || -> Result<()> {
        info!("Starting device monitor thread...");

        if token.is_cancelled() {
            info!("Device monitor initialization cancelled");
            return Ok(());
        }

        // Create channel for rotation events
        let (event_tx, event_rx) = bounded::<DeviceMonitorEvent>(DEVICE_MONITOR_CHANNEL_CAPACITY);
        tx.send(InitEvent::DeviceMonitor(event_rx))?;

        // Start rotation and im state monitoring
        const MAX_CONSECUTIVE_ERRORS: u32 = 3; // Exit after 3 consecutive failures

        let mut last_rotation = None;
        let mut consecutive_errors = 0;
        loop {
            // Check for cancellation
            if token.is_cancelled() {
                info!("Device monitor cancellation requested, stopping...");
                break;
            }

            if let Ok(state) = AdbShell::get_device_state(&serial)
                && state != DeviceState::Connected
            {
                info!("ADB state: {:?} - sending DeviceOffline event", state);
                let _ = event_tx.send(DeviceMonitorEvent::DeviceOffline);

                // Device is offline, exit monitoring
                break;
            }

            if token.is_cancelled() {
                info!("Device monitor cancellation requested, stopping...");
                break;
            }

            match AdbShell::get_screen_orientation(&serial) {
                Ok(current_rotation) => {
                    consecutive_errors = 0; // Reset error counter on success

                    if Some(current_rotation) != last_rotation {
                        debug!(
                            "Rotation changed: {:?} -> {}",
                            last_rotation, current_rotation
                        );
                        last_rotation = Some(current_rotation);

                        // Send rotation event
                        if let Err(e) = event_tx.send(DeviceMonitorEvent::Rotated(current_rotation))
                        {
                            debug!("Rotation event channel disconnected: {}", e);
                            break;
                        }
                    }
                }
                Err(e) => {
                    consecutive_errors += 1;
                    if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                        warn!(
                            "Device disconnected (adb failed {} times): {}",
                            consecutive_errors, e
                        );

                        break; // Exit monitoring thread
                    }
                    error!(
                        "Failed to get screen orientation ({}/{}): {}",
                        consecutive_errors, MAX_CONSECUTIVE_ERRORS, e
                    );
                }
            }

            if token.is_cancelled() {
                info!("Device monitor cancellation requested, stopping...");
                break;
            }
            thread::sleep(Duration::from_millis(DEVICE_MONITOR_POLL_INTERVAL_MS));
        }

        info!("Device monitor thread stopped");

        Ok(())
    });
}
fn start_ime_state_monitor(serial: &str, tx: Sender<DeviceMonitorEvent>, token: CancellationToken) {
    thread::spawn(move || {
        info!("Starting IME state monitor thread...");

        let mut consecutive_errors = 0;
        loop {
            if token.is_cancelled() {
                info!("Device monitor cancellation requested, stopping...");
                break;
            }

            // Poll input method state (skip if device is disconnected)
            if consecutive_errors == 0
                && let Ok(im_state) = AdbShell::get_ime_state(&serial)
                && event_tx
                    .send(DeviceMonitorEvent::ImStateChanged(im_state))
                    .is_err()
            {
                debug!("IME event channel disconnected, stopping monitor");
                break;
            }
        }
    });
}
fn start_rotation_monitor(serial: &str, tx: Sender<DeviceMonitorEvent>, token: CancellationToken) {}
