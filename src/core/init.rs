// SPDX-License-Identifier: MIT OR Apache-2.0

//! Background initialization coordinator
//!
//! Coordinates scrcpy connection, device monitoring, and input mapper initialization.
//! Delegates work to ConnectionService, DeviceMonitor, and InputManager.

use {
    crate::{
        behavior::BehaviorEngine,
        config::SAideConfig,
        controller::{control_sender::ControlSender, keyboard::KeyboardMapper, mouse::MouseMapper},
        core::{
            connection::{ConnectionResult, ConnectionService, InputManager},
            device_monitor::DeviceMonitor,
        },
        error::SAideError,
        scrcpy::connection::{AudioDisabledReason, ScrcpyConnection},
    },
    adbshell::{AdbShell, DeviceState},
    crossbeam_channel::{Receiver, Sender},
    std::{
        net::TcpStream,
        sync::{Arc, Mutex},
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
        connection: Arc<ScrcpyConnection>,
        control_sender: ControlSender,
        video_stream: TcpStream,
        audio_stream: Option<TcpStream>,
        video_resolution: (u32, u32),
        device_name: Option<String>,
        audio_disabled_reason: Option<AudioDisabledReason>,
        capture_orientation: Option<u32>,
        behavior_engine: Arc<Mutex<BehaviorEngine>>,
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
    const ADB_DEVICE_WAIT_MS: u64 = 500;
    const ADB_DEVICE_CHECK_INTERVAL_MS: u64 = 50;

    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(ADB_DEVICE_WAIT_MS) {
        if matches!(
            AdbShell::get_device_state(serial),
            Ok(DeviceState::Connected)
        ) {
            break;
        }
        thread::sleep(Duration::from_millis(ADB_DEVICE_CHECK_INTERVAL_MS));
    }

    let shell = match AdbShell::new(serial) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            let _ = tx.send(InitEvent::Error(e.into()));
            return;
        }
    };

    let shell_for_monitor = shell.clone();

    let tx_clone = tx.clone();
    let monitor_shell = shell_for_monitor;
    let monitor_token = cancellation_token.clone();
    ConnectionService::start(
        shell,
        config.clone(),
        move |result| match result {
            Ok(conn_result) => {
                send_connection_events(
                    &tx_clone,
                    conn_result,
                    &config,
                    monitor_shell,
                    monitor_token.clone(),
                );
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
    monitor_shell: Arc<AdbShell>,
    token: CancellationToken,
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
        shell: _connection_shell,
        monitor_stream,
    } = conn_result;

    // 创建共享的 BehaviorEngine 实例
    let behavior_config = config.behavior_config();
    let engine = BehaviorEngine::new(
        behavior_config,
        video_resolution.0 as u16,
        video_resolution.1 as u16,
    );
    let engine = Arc::new(Mutex::new(engine));
    let engine_for_player = Arc::clone(&engine);

    let connection = Arc::new(connection);

    if tx
        .send(InitEvent::ConnectionReady {
            connection: Arc::clone(&connection),
            control_sender: control_sender.clone(),
            video_stream,
            audio_stream,
            video_resolution,
            device_name,
            audio_disabled_reason,
            capture_orientation,
            behavior_engine: engine_for_player,
        })
        .is_err()
    {
        return;
    }

    let tx_keyboard = tx.clone();
    InputManager::create_keyboard_mapper(control_sender.clone(), engine.clone(), move |mapper| {
        let _ = tx_keyboard.send(InitEvent::KeyboardMapper(mapper));
    });

    let tx_mouse = tx.clone();
    InputManager::create_mouse_mapper(
        config.input.clone(),
        control_sender,
        engine,
        move |mapper| {
            let _ = tx_mouse.send(InitEvent::MouseMapper(mapper));
        },
    );

    // Start DeviceMonitor with TCP stream (after connection is established)
    match DeviceMonitor::start(monitor_shell, monitor_stream, token) {
        Ok(device_monitor_rx) => {
            if tx
                .send(InitEvent::DeviceMonitor(device_monitor_rx))
                .is_err()
            {}
        }
        Err(e) => {
            let _ = tx.send(InitEvent::Error(e));
        }
    }
}
