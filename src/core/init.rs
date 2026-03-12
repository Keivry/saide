// SPDX-License-Identifier: MIT OR Apache-2.0

//! Background initialization coordinator
//!
//! Coordinates scrcpy connection, device monitoring, and input mapper initialization.
//! Delegates work to ConnectionService, DeviceMonitor, and InputManager.

use {
    crate::{
        config::SAideConfig,
        controller::{control_sender::ControlSender, keyboard::KeyboardMapper, mouse::MouseMapper},
        core::{
            connection::{ConnectionResult, ConnectionService, InputManager},
            device_monitor::DeviceMonitor,
        },
        error::SAideError,
        scrcpy::connection::{AudioDisabledReason, ScrcpyConnection},
    },
    crossbeam_channel::{Receiver, Sender},
    std::{
        net::TcpStream,
        process::Command,
        sync::Arc,
        thread,
        time::{Duration, Instant},
    },
    tokio_util::sync::CancellationToken,
};

pub use super::device_monitor::DeviceMonitorEvent;

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
        audio_disabled_reason: Option<AudioDisabledReason>,
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
    const ADB_SERVER_STARTUP_WAIT_MS: u64 = 500;
    const ADB_SERVER_CHECK_INTERVAL_MS: u64 = 50;

    // Delay to allow ADB server to be ready
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(ADB_SERVER_STARTUP_WAIT_MS) {
        if Command::new("adb")
            .args(["shell", "echo", "ok"])
            .output()
            .is_ok()
        {
            break;
        }
        thread::sleep(Duration::from_millis(ADB_SERVER_CHECK_INTERVAL_MS));
    }

    let serial = serial.to_owned();

    // Start device monitor
    match DeviceMonitor::start(&serial, cancellation_token.clone()) {
        Ok(device_monitor_rx) => {
            if tx
                .send(InitEvent::DeviceMonitor(device_monitor_rx))
                .is_err()
            {
                return;
            }
        }
        Err(e) => {
            let _ = tx.send(InitEvent::Error(e));
            return;
        }
    }

    // Start scrcpy connection (async)
    let tx_clone = tx.clone();
    ConnectionService::start(
        &serial,
        config.clone(),
        move |result| match result {
            Ok(conn_result) => {
                send_connection_events(&tx_clone, conn_result, &config);
            }
            Err(e) => {
                let _ = tx_clone.send(InitEvent::Error(e));
            }
        },
        cancellation_token,
    );
}

fn send_connection_events(
    tx: &Sender<InitEvent>,
    conn_result: ConnectionResult,
    config: &Arc<SAideConfig>,
) {
    let ConnectionResult {
        connection,
        control_sender,
        video_stream,
        audio_stream,
        video_resolution,
        device_name,
        audio_disabled_reason,
        capture_orientation,
    } = conn_result;

    if tx
        .send(InitEvent::ConnectionReady {
            connection,
            control_sender: control_sender.clone(),
            video_stream,
            audio_stream,
            video_resolution,
            device_name,
            audio_disabled_reason,
            capture_orientation,
        })
        .is_err()
    {
        return;
    }

    let tx_keyboard = tx.clone();
    InputManager::create_keyboard_mapper(control_sender.clone(), move |mapper| {
        let _ = tx_keyboard.send(InitEvent::KeyboardMapper(mapper));
    });

    let tx_mouse = tx.clone();
    InputManager::create_mouse_mapper(config.input.clone(), control_sender, move |mapper| {
        let _ = tx_mouse.send(InitEvent::MouseMapper(mapper));
    });
}
