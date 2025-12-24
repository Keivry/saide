//! Module for interacting with Android devices via adb commands.
//!
//! This module provides functions to retrieve device information such as
//! screen size, orientation, input method state, and device serial using adb commands.

use {
    crate::error::{Result, SAideError},
    std::{
        ffi::OsStr,
        process::{Child, Command, Stdio},
        sync::mpsc,
        thread,
        time::Duration,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceState {
    Connected,
    Disconnected,
    Unauthorized,
}

pub struct AdbShell;

impl AdbShell {
    /// Get Android device screen size using separate adb command
    /// blocking until command completes.
    pub fn get_physical_screen_size(serial: &str) -> Result<(u32, u32)> {
        check_serial(serial)?;

        // Use separate adb command to get screen size, not through shell session
        let output = Command::new("adb")
            .args(["-s", serial])
            .args(["shell", "wm size"])
            .output()
            .map_err(|e| {
                SAideError::AdbError(format!(
                    "Failed to query screen size for device {serial}: {e}",
                ))
            })?;

        // Parse output like "Physical size: 1080x2340"
        let output_str = String::from_utf8_lossy(&output.stdout);
        output_str
            .lines()
            .find(|line| line.contains("Physical size:"))
            .and_then(|line| line.split(':').nth(1))
            .and_then(|size_part| {
                let size_str = size_part.trim();
                let mut parts = size_str.split('x');
                let width = parts.next().and_then(|w| w.trim().parse::<u32>().ok());
                let height = parts.next().and_then(|h| h.trim().parse::<u32>().ok());

                match (width, height) {
                    (Some(w), Some(h)) => Some((w, h)),
                    _ => None,
                }
            })
            .ok_or_else(|| {
                SAideError::AdbError(format!(
                    "Failed to parse screen size from output: {}",
                    output_str.trim()
                ))
            })
    }

    /// Get Android device screen orientation using separate adb command (with 3s timeout)
    pub fn get_screen_orientation(serial: &str) -> Result<u32> {
        check_serial(serial)?;

        let child = Command::new("adb")
            .args(["-s", serial])
            .args(["shell", "dumpsys window displays | grep mCurrentRotation"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                SAideError::AdbError(format!(
                    "Failed to query orientation for device {serial}: {e}"
                ))
            })?;

        // Wait with 3 second timeout
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = tx.send(child.wait_with_output());
        });

        let output = match rx.recv_timeout(Duration::from_secs(3)) {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => {
                return Err(SAideError::AdbError(format!(
                    "Failed to query Android device orientation: {}",
                    e
                )));
            }
            Err(_) => {
                return Err(SAideError::AdbError(
                    "Timeout to query Android device orientation".to_string(),
                ));
            }
        };

        if !output.status.success() {
            return Err(SAideError::AdbError(format!(
                "Failed to query Android device orientation, adb exited with status: {}",
                output.status
            )));
        }

        // Parse output like "mCurrentRotation=ROTATION_0"
        // TODO: Support different Android versions if output format changes
        let output_str = String::from_utf8_lossy(&output.stdout);
        output_str
            .lines()
            .find(|line| line.contains("mCurrentRotation"))
            .and_then(|line| line.split('=').nth(1))
            .and_then(|rotation_part| {
                let rotation_str = rotation_part.trim();
                // Match rotation strings like "ROTATION_0", "ROTATION_90", etc.
                match rotation_str {
                    "ROTATION_0" => Some(0),
                    "ROTATION_90" => Some(1),
                    "ROTATION_180" => Some(2),
                    "ROTATION_270" => Some(3),
                    _ => None,
                }
            })
            .ok_or_else(|| {
                SAideError::AdbError(format!(
                    "Failed to parse rotation from output: {}",
                    output_str.trim()
                ))
            })
    }

    /// Get android device input method state (with 3s timeout)
    pub fn get_ime_state(serial: &str) -> Result<bool> {
        check_serial(serial)?;

        let child = Command::new("adb")
            .args(["-s", serial])
            .args([
                "shell",
                "dumpsys window InputMethod | grep 'isVisible=true'",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                SAideError::AdbError(format!(
                    "Failed to query ime state for device {serial}: {e}"
                ))
            })?;

        // Wait with 3 second timeout
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = tx.send(child.wait_with_output());
        });

        let output = match rx.recv_timeout(Duration::from_secs(3)) {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => {
                return Err(SAideError::AdbError(format!(
                    "Failed to query ime state in device: {e}",
                )));
            }
            Err(_) => {
                return Err(SAideError::AdbError(
                    "Timeout to query ime state in device".to_string(),
                ));
            }
        };

        Ok(output.status.success())
    }

    /// Get Android device serial using adb command, blocking until command completes.
    pub fn get_device_serial() -> Result<String> {
        let output = Command::new("adb")
            .args(["get-serialno"])
            .output()
            .map_err(|e| {
                SAideError::AdbError(format!("Failed to query Android device Id: {}", e))
            })?;

        let output_str = String::from_utf8_lossy(&output.stdout);
        let device_serial = output_str.trim().to_string();
        Ok(device_serial)
    }

    /// Get ADB device state (device/offline/unauthorized/no device)
    pub fn get_device_state(serial: &str) -> Result<DeviceState> {
        check_serial(serial)?;

        let output = Command::new("adb")
            .args(["-s", serial])
            .args(["get-state"])
            .output()
            .map_err(|e| {
                SAideError::AdbError(format!("Failed to query state for device {serial}: {e}"))
            })?;

        let state = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if state.contains("unauthorized") {
            Ok(DeviceState::Unauthorized)
        } else if state.contains("not found") {
            Ok(DeviceState::Disconnected)
        } else if state.contains("device") {
            Ok(DeviceState::Connected)
        } else {
            Err(SAideError::AdbError(format!(
                "Unknown Android device state: {state}",
            )))
        }
    }

    /// Get Android API level, e.g., 30 for Android 11. Blocking until command completes.
    pub fn get_android_version(serial: &str) -> Result<u32> {
        let output = AdbShell::get_prop(serial, "ro.build.version.sdk")?;
        output
            .trim()
            .parse()
            .map_err(|e| SAideError::AdbError(format!("Failed to parse Android version: {}", e)))
    }

    /// Get device platform
    pub fn get_platform(serial: &str) -> Result<String> {
        let platform = AdbShell::get_prop(serial, "ro.board.platform")?;
        if !platform.is_empty() && platform != "unknown" {
            return Ok(platform);
        }

        AdbShell::get_prop(serial, "ro.hardware")
    }

    /// Get Android system property
    pub fn get_prop(serial: &str, prop_name: &str) -> Result<String> {
        check_serial(serial)?;

        let output = Command::new("adb")
            .args(["-s", serial, "shell", "getprop", prop_name])
            .output()?;

        if !output.status.success() {
            return Ok("unknown".to_string());
        }

        let value = String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_lowercase();

        Ok(value)
    }

    /// Push file to Android device using adb push command, blocking until command completes.
    pub fn push_file(serial: &str, file: &str, path: &str) -> Result<()> {
        check_serial(serial)?;

        let status = Command::new("adb")
            .args(["-s", serial, "push", file, path])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .status()
            .map_err(|e| {
                SAideError::AdbError(format!(
                    "Failed to push file [{}] to device [{}]: {}",
                    file, path, e
                ))
            })?;

        if !status.success() {
            return Err(SAideError::AdbError(format!(
                "Failed to push file [{}] to device [{}], adb exited with status: {}",
                file, path, status
            )));
        }

        Ok(())
    }

    /// Execute jar file on Android device using adb command, returning Child process.
    pub fn execute_jar<I, S>(
        serial: &str,
        jar: &str,
        running_dir: &str,
        class_name: &str,
        version: &str,
        args: I,
    ) -> Result<Child>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        check_serial(serial)?;

        let child = Command::new("adb")
            .args(["-s", serial])
            .args([
                "shell".to_string(),
                format!("CLASSPATH={}", jar),
                "app_process".to_string(),
                running_dir.to_string(),
                class_name.to_string(),
                version.to_string(),
            ])
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                SAideError::AdbError(format!(
                    "Failed to execute JAR file [{}] on device [{}]: {}",
                    jar, serial, e
                ))
            })?;

        Ok(child)
    }

    /// Setup ADB reverse tunnel
    ///
    /// Command: `adb reverse localabstract:<socket_name> tcp:<local_port>`
    pub fn setup_reverse_tunnel(serial: &str, socket_name: &str, local_port: u16) -> Result<()> {
        check_serial(serial)?;

        let status = Command::new("adb")
            .args([
                "-s",
                serial,
                "reverse",
                &format!("localabstract:{}", socket_name),
                &format!("tcp:{}", local_port),
            ])
            .status()
            .map_err(|e| {
                SAideError::AdbError(format!(
                    "Failed to setup adb reverse tunnel for device [{}]: {}",
                    serial, e
                ))
            })?;

        if !status.success() {
            return Err(SAideError::AdbError(format!(
                "Failed to setup adb reverse tunnel for device [{}], adb exited with status: {}",
                serial, status
            )));
        }

        Ok(())
    }

    /// Remove ADB reverse tunnel
    pub fn remove_reverse_tunnel(serial: &str, socket_name: &str) -> Result<()> {
        check_serial(serial)?;

        let status = Command::new("adb")
            .args([
                "-s",
                serial,
                "reverse",
                "--remove",
                &format!("localabstract:{}", socket_name),
            ])
            .output(); // Use output() instead of status() to capture stderr

        match status {
            Ok(output) if !output.status.success() => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                // Ignore "not found" errors (tunnel already removed)
                if !stderr.contains("not found") && !stderr.is_empty() {
                    return Err(SAideError::AdbError(format!(
                        "Failed to remove adb reverse tunnel for device [{}], adb exited with status: {}, stderr: {}",
                        serial, output.status, stderr
                    )));
                }
            }
            Ok(_) => {}
            Err(e) => {
                return Err(SAideError::AdbError(format!(
                    "Failed to remove adb reverse tunnel for device [{}]: {}",
                    serial, e
                )));
            }
        }

        Ok(())
    }
}

fn check_serial(serial: &str) -> Result<()> {
    if serial.is_empty() {
        return Err(SAideError::AdbError(
            "Device serial cannot be empty".to_string(),
        ));
    }
    Ok(())
}
