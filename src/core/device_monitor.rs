// SPDX-License-Identifier: MIT OR Apache-2.0

//! Device monitoring service
//!
//! Monitors device state (online/offline), rotation, and IME (keyboard) state.
//! Runs as async tasks and reports changes via channels.

use {
    crate::{
        controller::adb::{AdbShell, DeviceState},
        error::{Result, SAideError},
        runtime::TOKIO_RT,
    },
    crossbeam_channel::{Receiver, Sender, bounded},
    std::time::Duration,
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
    /// Start device monitoring in background task
    pub fn start(serial: &str, token: CancellationToken) -> Result<Receiver<DeviceMonitorEvent>> {
        let (event_tx, event_rx) = bounded::<DeviceMonitorEvent>(DEVICE_MONITOR_CHANNEL_CAPACITY);

        let serial = serial.to_owned();
        TOKIO_RT.spawn(async move {
            info!("Starting device monitor tasks...");

            if token.is_cancelled() {
                info!("Device monitor initialization cancelled");
                return;
            }

            tokio::join!(
                Self::monitor_device_state(serial.clone(), event_tx.clone(), token.clone()),
                Self::monitor_ime_state(serial.clone(), event_tx.clone(), token.clone()),
                Self::monitor_rotation(serial, event_tx, token),
            );
        });

        Ok(event_rx)
    }

    async fn monitor_device_state(
        serial: String,
        event_tx: Sender<DeviceMonitorEvent>,
        token: CancellationToken,
    ) {
        info!("Starting device state monitor...");
        let mut interval =
            tokio::time::interval(Duration::from_millis(DEVICE_MONITOR_POLL_INTERVAL_MS));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = token.cancelled() => {
                    info!("Device state monitor cancellation requested, stopping...");
                    return;
                }
                _ = interval.tick() => {}
            }

            match run_adb_with_retry(token.clone(), AdbShell::get_device_state, serial.clone())
                .await
            {
                Ok(state) => {
                    if state != DeviceState::Connected {
                        info!("Device went offline: {:?}", state);
                        event_tx
                            .send(DeviceMonitorEvent::DeviceOffline)
                            .unwrap_or_else(|e| {
                                error!("Failed to send DeviceOffline event: {}", e);
                            });
                        return;
                    }
                }
                Err(e) => {
                    error!("Failed to get device state: {}", e);
                    return;
                }
            }
        }
    }

    async fn monitor_ime_state(
        serial: String,
        event_tx: Sender<DeviceMonitorEvent>,
        token: CancellationToken,
    ) {
        info!("Starting IME state monitor...");
        let mut interval =
            tokio::time::interval(Duration::from_millis(DEVICE_MONITOR_POLL_INTERVAL_MS));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = token.cancelled() => {
                    info!("Device IME monitor cancellation requested, stopping...");
                    return;
                }
                _ = interval.tick() => {}
            }

            match run_adb_with_retry(token.clone(), AdbShell::get_ime_state, serial.clone()).await {
                Ok(is_shown) => {
                    event_tx
                        .send(DeviceMonitorEvent::ImStateChanged(is_shown))
                        .unwrap_or_else(|e| {
                            error!("Failed to send IME state event: {}", e);
                        });
                }
                Err(e) => {
                    error!("Failed to get IME state: {}", e);
                    return;
                }
            }
        }
    }

    async fn monitor_rotation(
        serial: String,
        event_tx: Sender<DeviceMonitorEvent>,
        token: CancellationToken,
    ) {
        info!("Starting device rotation monitor...");
        let mut interval =
            tokio::time::interval(Duration::from_millis(DEVICE_MONITOR_POLL_INTERVAL_MS));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let mut last_orientation: Option<u32> = None;

        loop {
            tokio::select! {
                _ = token.cancelled() => {
                    info!("Device rotation monitor cancellation requested, stopping...");
                    return;
                }
                _ = interval.tick() => {}
            }

            match run_adb_with_retry(
                token.clone(),
                AdbShell::get_screen_orientation,
                serial.clone(),
            )
            .await
            {
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
                    return;
                }
            }
        }
    }
}

async fn run_adb_with_retry<T>(
    token: CancellationToken,
    f: fn(&str) -> Result<T>,
    serial: String,
) -> Result<T>
where
    T: Send + 'static,
{
    if token.is_cancelled() {
        return Err(SAideError::Cancelled);
    }

    const MAX_RETRIES: u32 = 3;
    const RETRY_DELAY_MS: u64 = 100;

    let mut attempts = 0u32;
    loop {
        let s = serial.clone();
        let result = tokio::task::spawn_blocking(move || f(&s))
            .await
            .map_err(|e| SAideError::Other(format!("ADB task panic: {e:?}")))?;

        match result {
            Ok(v) => return Ok(v),
            Err(e) => {
                attempts += 1;
                if attempts >= MAX_RETRIES {
                    return Err(e);
                }
                warn!(
                    "ADB command failed (attempt {}): {}. Retrying in {}ms...",
                    attempts, e, RETRY_DELAY_MS
                );
                tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
            }
        }
    }
}
