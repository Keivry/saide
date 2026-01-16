//! Device monitoring service
//!
//! Monitors device state (online/offline), rotation, and IME (keyboard) state.
//! Runs in background threads and reports changes via channels.

use {
    crate::{
        controller::adb::{AdbShell, DeviceState},
        error::{Result, SAideError},
    },
    crossbeam_channel::{Receiver, Sender, bounded},
    std::{
        thread::{self, JoinHandle},
        time::Duration,
    },
    tokio_util::sync::CancellationToken,
    tracing::{error, info, warn},
};

const DEVICE_MONITOR_POLL_INTERVAL_MS: u64 = 1000;
const DEVICE_MONITOR_CHANNEL_CAPACITY: usize = 64;

/// Device state change events
#[derive(Clone, Debug)]
pub enum DeviceMonitorEvent {
    Rotated(u32),
    ImStateChanged(bool),
    DeviceOffline,
}

/// Device monitor service
pub struct DeviceMonitor;

impl DeviceMonitor {
    /// Start device monitoring in background thread
    pub fn start(serial: &str, token: CancellationToken) -> Result<Receiver<DeviceMonitorEvent>> {
        let (event_tx, event_rx) = bounded::<DeviceMonitorEvent>(DEVICE_MONITOR_CHANNEL_CAPACITY);

        let serial = serial.to_owned();
        thread::spawn(move || {
            info!("Starting device monitor thread...");

            if token.is_cancelled() {
                info!("Device monitor initialization cancelled");
                return;
            }

            let handles = vec![
                Self::monitor_device_state(&serial, event_tx.clone(), token.clone()),
                Self::monitor_ime_state(&serial, event_tx.clone(), token.clone()),
                Self::monitor_rotation(&serial, event_tx.clone(), token.clone()),
            ];

            for handle in handles {
                if let Err(e) = handle.join().unwrap_or_else(|e| {
                    Err(SAideError::Other(format!(
                        "Device monitor thread panicked: {:?}",
                        e
                    )))
                }) {
                    error!("Device monitor thread error: {}", e);
                }
            }
        });

        Ok(event_rx)
    }

    fn monitor_device_state(
        serial: &str,
        event_tx: Sender<DeviceMonitorEvent>,
        token: CancellationToken,
    ) -> JoinHandle<Result<()>> {
        let serial = serial.to_owned();
        thread::spawn(move || -> Result<()> {
            info!("Starting device state monitor thread...");

            loop {
                if token.is_cancelled() {
                    info!("Device state monitor cancellation requested, stopping...");
                    break;
                }

                match retry_adb_command(|| AdbShell::get_device_state(&serial)) {
                    Ok(state) => {
                        if state != DeviceState::Connected {
                            info!("Device went offline: {:?}", state);

                            event_tx
                                .send(DeviceMonitorEvent::DeviceOffline)
                                .unwrap_or_else(|e| {
                                    error!("Failed to send DeviceOffline event: {}", e);
                                });

                            break;
                        }
                    }
                    Err(e) => {
                        error!("Failed to get device state: {}", e);
                        return Err(e);
                    }
                }

                thread::sleep(Duration::from_millis(DEVICE_MONITOR_POLL_INTERVAL_MS));
            }
            Ok(())
        })
    }

    fn monitor_ime_state(
        serial: &str,
        event_tx: Sender<DeviceMonitorEvent>,
        token: CancellationToken,
    ) -> JoinHandle<Result<()>> {
        let serial = serial.to_owned();
        thread::spawn(move || -> Result<()> {
            info!("Starting IME state monitor thread...");

            loop {
                if token.is_cancelled() {
                    info!("Device IME monitor cancellation requested, stopping...");
                    break;
                }

                match retry_adb_command(|| AdbShell::get_ime_state(&serial)) {
                    Ok(is_shown) => {
                        event_tx
                            .send(DeviceMonitorEvent::ImStateChanged(is_shown))
                            .unwrap_or_else(|e| {
                                error!("Failed to send IME state event: {}", e);
                            });
                    }
                    Err(e) => {
                        error!("Failed to get IME state: {}", e);
                        return Err(e);
                    }
                }

                thread::sleep(Duration::from_millis(DEVICE_MONITOR_POLL_INTERVAL_MS));
            }

            Ok(())
        })
    }

    fn monitor_rotation(
        serial: &str,
        event_tx: Sender<DeviceMonitorEvent>,
        token: CancellationToken,
    ) -> JoinHandle<Result<()>> {
        let serial = serial.to_owned();
        thread::spawn(move || -> Result<()> {
            info!("Starting device rotation monitor thread...");

            let mut last_orientation: Option<u32> = None;

            loop {
                if token.is_cancelled() {
                    info!("Device rotation monitor cancellation requested, stopping...");
                    break;
                }

                match retry_adb_command(|| AdbShell::get_screen_orientation(&serial)) {
                    Ok(orientation) => {
                        if Some(orientation) != last_orientation {
                            info!("Device rotated to orientation: {}", orientation);

                            last_orientation = Some(orientation);

                            event_tx
                                .send(DeviceMonitorEvent::Rotated(orientation))
                                .unwrap_or_else(|e| {
                                    error!("Failed to send rotation event: {}", e);
                                });
                        }
                    }
                    Err(e) => {
                        error!("Failed to get device orientation: {}", e);
                        return Err(e);
                    }
                }

                thread::sleep(Duration::from_millis(DEVICE_MONITOR_POLL_INTERVAL_MS));
            }

            Ok(())
        })
    }
}

fn retry_adb_command<F, T>(mut command_fn: F) -> Result<T>
where
    F: FnMut() -> Result<T>,
{
    const MAX_RETRIES: u32 = 3;
    const RETRY_DELAY_MS: u64 = 100;

    let mut attempts = 0;
    loop {
        match command_fn() {
            Ok(result) => return Ok(result),
            Err(e) => {
                attempts += 1;
                if attempts >= MAX_RETRIES {
                    return Err(e);
                }
                warn!(
                    "ADB command failed (attempt {}): {}. Retrying in {} ms...",
                    attempts, e, RETRY_DELAY_MS
                );
                thread::sleep(Duration::from_millis(RETRY_DELAY_MS));
            }
        }
    }
}
