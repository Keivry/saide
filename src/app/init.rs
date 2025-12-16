use {
    crate::{
        config::ConfigManager,
        controller::{
            adb::AdbShell,
            control_sender::ControlSender,
            keyboard::KeyboardMapper,
            mouse::MouseMapper,
        },
        scrcpy::connection::ScrcpyConnection,
    },
    anyhow::Context,
    crossbeam_channel::{Receiver, Sender, bounded},
    std::{
        net::TcpStream,
        process::Command,
        thread,
        time::{Duration, Instant},
    },
    tracing::{debug, error, info},
};

pub const DEVICE_MONITOR_POLL_INTERVAL_MS: u64 = 1000;
pub const DEVICE_MONITOR_CHANNEL_CAPACITY: usize = 1;

pub enum DeviceMonitorEvent {
    /// Device rotated event with new orientation (0-3), clockwise
    Rotated(u32),
    /// Device input method (IME) state changed, true = shown, false = hidden
    ImStateChanged(bool),
}

pub const INIT_RESULT_CHANNEL_CAPACITY: usize = 5;

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
        capture_orientation_locked: bool,
    },
    KeyboardMapper(KeyboardMapper),
    MouseMapper(MouseMapper),
    DeviceMonitor(Receiver<DeviceMonitorEvent>),
    DeviceId(String),
    PhysicalSize((u32, u32)),
    Error(anyhow::Error),
}

/// Background initialization function
pub fn start_initialization(config_manager: &ConfigManager, tx: Sender<InitEvent>) {
    // Note: External scrcpy initialization removed - using internal StreamPlayer

    // Delay to allow ADB server to be ready
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(500) {
        // check if adb is responsive
        if Command::new("adb")
            .args(["shell", "echo", "ok"])
            .output()
            .is_ok()
        {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }

    // Device monitor initialization
    let dm_tx = tx.clone();
    thread::spawn(move || -> Result<(), anyhow::Error> {
        // Get device ID
        let device_id = AdbShell::get_device_id()?;
        debug!("Using device ID: {}", device_id);
        dm_tx.send(InitEvent::DeviceId(device_id))?;

        // Get device physical screen size
        let physical_size = AdbShell::get_physical_screen_size()?;
        debug!(
            "Device physical screen size: {}x{}",
            physical_size.0, physical_size.1
        );
        dm_tx.send(InitEvent::PhysicalSize(physical_size))?;

        // Create channel for rotation events
        let (event_tx, event_rx) = bounded::<DeviceMonitorEvent>(DEVICE_MONITOR_CHANNEL_CAPACITY);
        dm_tx.send(InitEvent::DeviceMonitor(event_rx))?;

        // Start rotation and im state monitoring
        let mut last_rotation = None;
        loop {
            match AdbShell::get_screen_orientation() {
                Ok(current_rotation) => {
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
                    error!("Failed to get screen orientation: {}", e);
                }
            }

            // Poll input method state
            if let Ok(im_state) = AdbShell::get_ime_state()
                && event_tx
                    .send(DeviceMonitorEvent::ImStateChanged(im_state))
                    .is_err()
            {
                debug!("IME event channel disconnected, stopping monitor");
                break;
            }

            thread::sleep(Duration::from_millis(DEVICE_MONITOR_POLL_INTERVAL_MS));
        }

        Ok(())
    });

    // ScrcpyConnection initialization (async - moved to separate thread)
    // This will create mappers AFTER connection is established
    let conn_config = config_manager.config();
    let conn_tx = tx.clone();
    thread::spawn(move || -> Result<(), anyhow::Error> {
        info!("Establishing scrcpy connection...");

        // Wait for device to be ready
        thread::sleep(Duration::from_millis(500));

        // Get device serial
        let serial = AdbShell::get_device_id()?;
        debug!("Connecting to device: {}", serial);

        // 检测是否需要锁定 capture_orientation（需要在 async 闭包之前）
        use crate::scrcpy::server::ServerParams;
        let capture_orientation_locked = ServerParams::should_lock_orientation_for_nvdec();

        // Establish ScrcpyConnection (blocking)
        let runtime = tokio::runtime::Runtime::new()?;
        let mut connection = runtime.block_on(async {
            let server_jar_path = "3rd-party/scrcpy-server-v3.3.3";

            // Create server params from config
            let mut params = ServerParams::for_device(&serial)?;

            // Apply config settings
            let bit_rate = {
                let rate_str = &conn_config.scrcpy.video.bit_rate;
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
            params.video_codec = conn_config.scrcpy.video.codec.clone();
            params.video_bit_rate = bit_rate;
            params.max_size = conn_config.scrcpy.video.max_size as u16;
            params.max_fps = conn_config.scrcpy.video.max_fps as u16;
            params.audio = conn_config.scrcpy.audio.enabled;
            params.audio_codec = conn_config.scrcpy.audio.codec.clone();
            params.audio_source = conn_config.scrcpy.audio.source.clone();
            params.control = true;
            params.send_device_meta = true;
            params.send_codec_meta = true;
            params.send_frame_meta = true;

            // Apply screen control options
            params.stay_awake = conn_config.scrcpy.options.stay_awake;
            params.screen_off_timeout = if conn_config.scrcpy.options.turn_screen_off {
                Some(-1) // Turn off immediately
            } else {
                None
            };

            // 🔒 NVDEC Optimization: Lock capture orientation to prevent resolution changes
            // Benefits:
            // - Avoid decoder rebuild overhead (~200ms + black screen)
            // - No need for prepend-sps-pps-to-idr-frames=1 (compatibility)
            // - More stable, works on all devices
            if capture_orientation_locked {
                // Lock to current device orientation (absolute)
                // @0 = lock to 0° (portrait), follows device's natural orientation
                params.capture_orientation = Some("@0".to_string());
                info!("🔒 Locked capture orientation for NVDEC (prevents resolution changes)");
                info!("Video orientation fixed, decoder never needs rebuilding");
            }

            ScrcpyConnection::connect(&serial, server_jar_path, params)
                .await
                .map_err(|e| {
                    error!("Failed to establish scrcpy connection: {}", e);
                    e
                })
        })?;

        info!("ScrcpyConnection established successfully");

        // Extract streams (but keep connection alive)
        let video_stream = connection
            .video_stream
            .take()
            .ok_or_else(|| anyhow::anyhow!("Video stream not available"))?;
        let audio_stream = connection.audio_stream.take();
        let control_stream = connection
            .control_stream
            .take()
            .ok_or_else(|| anyhow::anyhow!("Control stream not available"))?;
        let video_resolution = connection
            .video_resolution
            .ok_or_else(|| anyhow::anyhow!("Video resolution not available"))?;
        let device_name = connection.device_name.clone();
        let audio_disabled_reason = connection.audio_disabled_reason.clone();

        // Create ControlSender with cloned stream
        // We need to clone the TcpStream to keep both in ControlSender and return original
        let control_stream_clone = control_stream
            .try_clone()
            .context("Failed to clone control stream")?;

        let control_sender = ControlSender::new(
            control_stream_clone,
            video_resolution.0 as u16,
            video_resolution.1 as u16,
        );

        info!(
            "ControlSender created with resolution {}x{}, capture_orientation_locked={}",
            video_resolution.0, video_resolution.1, capture_orientation_locked
        );

        // Put the original control_stream back into connection to keep it alive
        connection.control_stream = Some(control_stream);

        // Send connection ready event (with connection to keep alive)
        conn_tx.send(InitEvent::ConnectionReady {
            connection,
            control_sender: control_sender.clone(),
            video_stream,
            audio_stream,
            video_resolution,
            device_name,
            audio_disabled_reason,
            capture_orientation_locked,
        })?;

        // Now create keyboard mapper (if enabled)
        if conn_config.general.keyboard_enabled {
            let keyboard_mapper =
                KeyboardMapper::new(conn_config.mappings.clone(), control_sender.clone())?;
            debug!("Keyboard mapper initialized with ControlSender");
            conn_tx.send(InitEvent::KeyboardMapper(keyboard_mapper))?;
        }

        // Create mouse mapper (if enabled)
        if conn_config.general.mouse_enabled {
            let mouse_mapper = MouseMapper::new(control_sender.clone())?;
            debug!("Mouse mapper initialized with ControlSender");
            conn_tx.send(InitEvent::MouseMapper(mouse_mapper))?;
        }

        Ok(())
    });
}
