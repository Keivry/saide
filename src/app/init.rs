use {
    crate::{
        config::ConfigManager,
        controller::{adb::AdbShell, keyboard::KeyboardMapper, mouse::MouseMapper, scrcpy::Scrcpy},
    },
    anyhow::anyhow,
    crossbeam_channel::{Receiver, Sender, bounded},
    std::{
        ffi::OsStr,
        process::Command,
        sync::Arc,
        thread,
        time::{Duration, Instant},
    },
    sysinfo::{ProcessesToUpdate, System},
    tracing::{debug, error},
};

pub const DEVICE_MONITOR_POLL_INTERVAL_MS: u64 = 1000;
pub const DEVICE_MONITOR_CHANNEL_CAPACITY: usize = 1;

pub enum DeviceMonitorEvent {
    /// Device rotated event with new orientation (0-3), clockwise
    Rotated(u32),
    /// Device input method (IME) state changed, true = shown, false = hidden
    ImStateChanged(bool),
}

pub const INIT_RESULT_CHANNEL_CAPACITY: usize = 6;

/// Initialization event
pub enum InitEvent {
    Scrcpy(Scrcpy),
    KeyboardMapper(Option<KeyboardMapper>),
    MouseMapper(Option<MouseMapper>),
    DeviceMonitor(Receiver<DeviceMonitorEvent>),
    DeviceId(String),
    PhysicalSize((u32, u32)),
    Error(anyhow::Error),
}

/// Background initialization function
pub fn start_initialization(config_manager: &ConfigManager, tx: Sender<InitEvent>) {
    // Scrcpy initialization
    let config = config_manager.config();
    let scrcpy_tx = tx.clone();
    thread::spawn(move || -> Result<(), anyhow::Error> {
        // Ensure no existing scrcpy process is running
        let mut sys = System::new_all();
        sys.refresh_processes(ProcessesToUpdate::All, true);

        if sys.processes().values().any(|process| {
            process.exe().and_then(|path| path.file_name()) == Some(OsStr::new("scrcpy"))
        }) {
            scrcpy_tx.send(InitEvent::Error(anyhow!(
                "Existing scrcpy process detected , please terminate it first",
            )))?;

            // Early return on error
            return Ok(());
        }

        // Initialize scrcpy manager
        let mut scrcpy = Scrcpy::new(Arc::clone(&config.scrcpy));

        if let Err(e) = scrcpy
            .spawn()?
            .wait_for_ready(Duration::from_secs(config.general.init_timeout as u64))
        {
            scrcpy.terminate().ok();
            scrcpy_tx.send(InitEvent::Error(anyhow!("Failed to start scrcpy: {}", e)))?;
        }
        debug!("Scrcpy process started and ready");

        scrcpy_tx.send(InitEvent::Scrcpy(scrcpy))?;
        Ok(())
    });

    // Delay to allow scrcpy to start ADB server
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
                            error!("Failed to send rotation event: {}", e);
                            break;
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to get screen orientation: {}", e);
                }
            }

            // Poll input method state
            if let Ok(im_state) = AdbShell::get_ime_state() {
                event_tx
                    .send(DeviceMonitorEvent::ImStateChanged(im_state))
                    .unwrap_or_else(|e| {
                        error!("Failed to send IME state event: {}", e);
                    });
            }

            thread::sleep(Duration::from_millis(DEVICE_MONITOR_POLL_INTERVAL_MS));
        }

        Ok(())
    });

    let kbd_config = config_manager.config();
    let kbd_tx = tx.clone();
    thread::spawn(move || -> Result<(), anyhow::Error> {
        // Initialize keyboard mapper
        let keyboard_mapper = kbd_config
            .general
            .keyboard_enabled
            .then_some(KeyboardMapper::new(kbd_config.mappings.clone()))
            .transpose()?;
        debug!("Keyboard mapper initialized");

        kbd_tx.send(InitEvent::KeyboardMapper(keyboard_mapper))?;
        Ok(())
    });

    let mouse_config = config_manager.config();
    let mouse_tx = tx.clone();
    thread::spawn(move || -> Result<(), anyhow::Error> {
        // Initialize mouse mapper
        let mouse_mapper = mouse_config
            .general
            .mouse_enabled
            .then_some(MouseMapper::new())
            .transpose()?;
        debug!("Mouse mapper initialized");

        mouse_tx.send(InitEvent::MouseMapper(mouse_mapper))?;
        Ok(())
    });
}
