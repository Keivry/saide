// SPDX-License-Identifier: MIT OR Apache-2.0

//! Device monitoring service
//!
//! Monitors device state (online/offline), rotation, and IME (keyboard) state.
//! When a TCP control stream is available, listens for scrcpy server
//! `DeviceMessage` events (low-latency). Falls back to ADB polling when the
//! scrcpy server does not send these messages (e.g. pre-built JAR).

use {
    crate::{
        controller::DeviceState,
        error::{Result, SAideError},
        runtime::TOKIO_RT,
        scrcpy::device_message::DeviceMessage,
    },
    adbshell::AdbShell,
    crossbeam_channel::{Receiver, Sender, bounded},
    egui_event::Event,
    std::{
        net::TcpStream,
        sync::{Arc, Mutex},
        time::Duration,
    },
    tokio_util::sync::CancellationToken,
    tracing::{debug, error, info, warn},
};

const DEVICE_MONITOR_CHANNEL_CAPACITY: usize = 64;

/// Device state change events
#[derive(Clone, Debug, Event)]
pub enum DeviceMonitorEvent {
    Rotated(u32),
    ImStateChanged(bool),
    DeviceOffline,
}

/// Device monitor service
pub struct DeviceMonitor {
    shell: Arc<AdbShell>,
    monitor_stream: Option<TcpStream>,
    event_tx: Sender<DeviceMonitorEvent>,
}

impl DeviceMonitor {
    /// Start device monitoring in background tasks.
    ///
    /// When `monitor_stream` is `Some`, listens for `DeviceMessage` events on
    /// the TCP control channel and performs lightweight TCP health checks.
    /// Falls back to ADB polling when no TCP stream is available.
    pub fn start(
        shell: Arc<AdbShell>,
        monitor_stream: Option<TcpStream>,
        token: CancellationToken,
    ) -> Result<Receiver<DeviceMonitorEvent>> {
        let (event_tx, event_rx) = bounded::<DeviceMonitorEvent>(DEVICE_MONITOR_CHANNEL_CAPACITY);

        let monitor = Self {
            shell,
            monitor_stream,
            event_tx,
        };

        TOKIO_RT.spawn(async move {
            info!("Starting device monitor tasks...");

            if token.is_cancelled() {
                info!("Device monitor initialization cancelled");
                return;
            }

            monitor.run(token).await;
        });

        Ok(event_rx)
    }

    async fn run(self, token: CancellationToken) {
        if let Some(stream) = self.monitor_stream {
            let event_tx = self.event_tx.clone();
            let shell = self.shell.clone();
            let health_stream = stream.try_clone().expect("monitor_stream try_clone failed");

            let shell_for_ime = shell.clone();
            let event_tx_for_ime = event_tx.clone();
            let token_for_ime = token.clone();
            let shell_for_rot = shell.clone();
            let event_tx_for_rot = event_tx.clone();
            let token_for_rot = token.clone();

            tokio::join!(
                Self::listen_device_messages(stream, event_tx.clone(), token.clone()),
                Self::monitor_device_state_tcp(health_stream, event_tx, shell, token),
                Self::monitor_ime_state_adb_fallback(
                    shell_for_ime,
                    event_tx_for_ime,
                    token_for_ime
                ),
                Self::monitor_rotation_adb_fallback(shell_for_rot, event_tx_for_rot, token_for_rot),
            );
        } else {
            let event_tx = self.event_tx;
            let shell = self.shell;
            let token_clone = token.clone();

            tokio::join!(
                Self::monitor_device_state_adb(shell.clone(), event_tx.clone(), token),
                Self::monitor_rotation_adb_fallback(
                    shell.clone(),
                    event_tx.clone(),
                    token_clone.clone()
                ),
                Self::monitor_ime_state_adb_fallback(shell, event_tx, token_clone),
            );
        }
    }

    /// Listen for `DeviceMessage` events on the TCP control stream.
    ///
    /// Parses `RotationChanged` and `ImeStateChanged` messages and forwards
    /// them as `DeviceMonitorEvent`s. Other message types are silently
    /// ignored.
    async fn listen_device_messages(
        stream: TcpStream,
        event_tx: Sender<DeviceMonitorEvent>,
        token: CancellationToken,
    ) {
        info!("Starting TCP device message listener...");

        // Use blocking mode with a short timeout so that read_exact() on
        // multi-byte payloads doesn't immediately fail with WouldBlock.
        // When the type byte and payload arrive in separate TCP segments
        // (possible for large clipboard/UHID messages), the short timeout
        // gives the kernel time to coalesce the remaining bytes before
        // returning a recoverable WouldBlock.
        if let Err(e) = stream.set_read_timeout(Some(Duration::from_millis(100))) {
            error!("Failed to set monitor stream read timeout: {}", e);
            return;
        }

        let stream = Arc::new(Mutex::new(stream));

        loop {
            tokio::select! {
                _ = token.cancelled() => {
                    info!("Device message listener cancelled");
                    return;
                }
                _ = tokio::time::sleep(Duration::from_millis(100)) => {}
            }

            let stream = Arc::clone(&stream);
            match tokio::task::spawn_blocking(move || {
                DeviceMessage::read_from(&mut *stream.lock().expect("mutex poisoned"))
            })
            .await
            {
                Ok(Ok(Some(DeviceMessage::RotationChanged(rot)))) => {
                    let orientation = rot / 90;
                    debug!(
                        "TCP: received rotation event: {}° -> orientation {}",
                        rot, orientation
                    );
                    let _ = event_tx.send(DeviceMonitorEvent::Rotated(orientation));
                }
                Ok(Ok(Some(DeviceMessage::ImeStateChanged(visible)))) => {
                    debug!("TCP: received IME state event: visible={}", visible);
                    let _ = event_tx.send(DeviceMonitorEvent::ImStateChanged(visible));
                }
                Ok(Ok(Some(other))) => {
                    debug!("TCP: received other device message: {:?}", other);
                }
                Ok(Ok(None)) => {
                    // Timeout / WouldBlock / EOF — no complete message yet
                }
                Ok(Err(e)) => {
                    // IO error on stream → device likely offline
                    error!("Device message listener IO error: {}", e);
                    let _ = event_tx.send(DeviceMonitorEvent::DeviceOffline);
                    return;
                }
                Err(e) => {
                    error!(
                        "spawn_blocking join error in device message listener: {}",
                        e
                    );
                    return;
                }
            }
        }
    }

    /// TCP-based device health check using `peer_addr()`.
    ///
    /// Every 2 seconds, tests whether the TCP connection is still alive.
    /// After 3 consecutive failures, confirms with ADB once before declaring
    /// the device offline.
    async fn monitor_device_state_tcp(
        stream: TcpStream,
        event_tx: Sender<DeviceMonitorEvent>,
        shell: Arc<AdbShell>,
        token: CancellationToken,
    ) {
        info!("Starting TCP device state monitor...");
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        const MAX_CONSECUTIVE_FAILURES: u32 = 3;

        let mut failures: u32 = 0;

        loop {
            tokio::select! {
                _ = token.cancelled() => {
                    info!("TCP device state monitor cancelled");
                    return;
                }
                _ = interval.tick() => {}
            }

            match stream.peer_addr() {
                Ok(_) => {
                    failures = 0;
                }
                Err(_) => {
                    failures += 1;
                    if failures >= MAX_CONSECUTIVE_FAILURES {
                        let serial = shell.serial().to_string();
                        let confirmed_offline = tokio::task::spawn_blocking(move || {
                            matches!(
                                AdbShell::get_device_state(&serial),
                                Err(_) | Ok(DeviceState::Disconnected)
                            )
                        })
                        .await
                        .unwrap_or(true);

                        if confirmed_offline {
                            info!("Device offline ({} TCP failures, ADB confirmed)", failures);
                            let _ = event_tx.send(DeviceMonitorEvent::DeviceOffline);
                            return;
                        }
                        failures = 0;
                    }
                }
            }
        }
    }

    // ── ADB fallback methods (pre-built JAR) ──────────────────────────

    /// DEPRECATED: pre-built jar fallback — ADB-based device state polling.
    async fn monitor_device_state_adb(
        shell: Arc<AdbShell>,
        event_tx: Sender<DeviceMonitorEvent>,
        token: CancellationToken,
    ) {
        info!("Starting ADB device state monitor (fallback)...");
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = token.cancelled() => {
                    info!("ADB device state monitor cancelled");
                    return;
                }
                _ = interval.tick() => {}
            }

            let serial = shell.serial().to_string();
            match run_adb_with_retry(token.clone(), shell.clone(), move |_s| {
                AdbShell::get_device_state(&serial)
            })
            .await
            {
                Ok(state) => {
                    if state != DeviceState::Connected {
                        info!("Device went offline (ADB): {:?}", state);
                        let _ = event_tx.send(DeviceMonitorEvent::DeviceOffline);
                        return;
                    }
                }
                Err(e) => {
                    error!("Failed to get device state (ADB): {}", e);
                    return;
                }
            }
        }
    }

    /// ADB-based IME state polling (low-priority fallback).
    ///
    /// The patched scrcpy-server now handles IME detection internally
    /// (via in-process dumpsys) and sends events over TCP. This fallback
    /// exists for pre-built JARs and as a safety net if the TCP path
    /// fails to detect IME changes.
    async fn monitor_ime_state_adb_fallback(
        shell: Arc<AdbShell>,
        event_tx: Sender<DeviceMonitorEvent>,
        token: CancellationToken,
    ) {
        info!("Starting ADB IME state monitor (fallback)...");
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = token.cancelled() => {
                    info!("ADB IME monitor cancelled");
                    return;
                }
                _ = interval.tick() => {}
            }

            match run_adb_with_retry(token.clone(), shell.clone(), |s| s.get_ime_state()).await {
                Ok(is_shown) => {
                    let _ = event_tx.send(DeviceMonitorEvent::ImStateChanged(is_shown));
                }
                Err(e) => {
                    error!("Failed to get IME state (ADB): {}", e);
                    return;
                }
            }
        }
    }

    /// DEPRECATED: pre-built jar fallback — ADB-based rotation polling.
    async fn monitor_rotation_adb_fallback(
        shell: Arc<AdbShell>,
        event_tx: Sender<DeviceMonitorEvent>,
        token: CancellationToken,
    ) {
        info!("Starting ADB device rotation monitor (fallback)...");
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let mut last_orientation: Option<u32> = None;

        loop {
            tokio::select! {
                _ = token.cancelled() => {
                    info!("ADB rotation monitor cancelled");
                    return;
                }
                _ = interval.tick() => {}
            }

            match run_adb_with_retry(token.clone(), shell.clone(), |s| s.get_screen_orientation())
                .await
            {
                Ok(orientation) => {
                    if Some(orientation) != last_orientation {
                        info!("Device rotated to orientation: {}", orientation);
                        last_orientation = Some(orientation);
                        let _ = event_tx.send(DeviceMonitorEvent::Rotated(orientation));
                    }
                }
                Err(e) => {
                    error!("Failed to get device orientation (ADB): {}", e);
                    return;
                }
            }
        }
    }
}

async fn run_adb_with_retry<T, F>(token: CancellationToken, shell: Arc<AdbShell>, f: F) -> Result<T>
where
    T: Send + 'static,
    F: Fn(&AdbShell) -> adbshell::AdbResult<T> + Send + Sync + 'static,
{
    if token.is_cancelled() {
        return Err(SAideError::Cancelled);
    }

    const MAX_RETRIES: u32 = 3;
    const RETRY_DELAY_MS: u64 = 100;

    let f = Arc::new(f);
    let mut attempts = 0u32;
    loop {
        let shell_ref = shell.clone();
        let f_ref = f.clone();
        let result = tokio::task::spawn_blocking(move || f_ref(&shell_ref))
            .await
            .map_err(|e| SAideError::Other(format!("ADB task panic: {e:?}")))?;

        match result {
            Ok(v) => return Ok(v),
            Err(e) => {
                attempts += 1;
                if attempts >= MAX_RETRIES {
                    return Err(SAideError::from(e));
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

#[cfg(test)]
mod tests {
    use {super::*, std::io::Cursor};

    #[test]
    fn test_rotation_message_parsed() {
        // DeviceMessage type byte 3 = RotationChanged, followed by 4-byte BE u32
        let mut buf = Vec::new();
        buf.push(3u8);
        buf.extend_from_slice(&90u32.to_be_bytes());

        let msg = DeviceMessage::read_from(&mut Cursor::new(buf))
            .expect("read_from failed")
            .expect("message should be Some");
        assert_eq!(msg, DeviceMessage::RotationChanged(90));
    }

    #[test]
    fn test_ime_message_parsed() {
        // DeviceMessage type byte 4 = ImeStateChanged, followed by 1-byte bool
        let buf = vec![4u8, 0x01];

        let msg = DeviceMessage::read_from(&mut Cursor::new(buf))
            .expect("read_from failed")
            .expect("message should be Some");
        assert_eq!(msg, DeviceMessage::ImeStateChanged(true));
    }

    #[test]
    fn test_tcp_probe_detects_disconnect() {
        // peer_addr() may return Ok after remote disconnect — the address is
        // cached by the kernel; real disconnection is detected via read errors.
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind");
        let addr = listener.local_addr().unwrap();
        let client = TcpStream::connect(addr).expect("failed to connect");
        let (server, _addr) = listener.accept().expect("failed to accept");
        drop(server);

        let mut failures: u32 = 0;
        const MAX_CONSECUTIVE_FAILURES: u32 = 3;

        for _ in 0..(MAX_CONSECUTIVE_FAILURES + 2) {
            match client.peer_addr() {
                Ok(_) => failures = 0,
                Err(_) => {
                    failures += 1;
                    if failures >= MAX_CONSECUTIVE_FAILURES {
                        break;
                    }
                }
            }
        }
    }
}
