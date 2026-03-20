// SPDX-License-Identifier: MIT OR Apache-2.0

//! Module for interacting with Android devices via adb commands.
//!
//! This module provides functions to retrieve device information such as
//! screen size, orientation, input method state, and device serial using adb commands.

use {
    crate::error::{Result, SAideError},
    std::{
        ffi::OsStr,
        fmt,
        io::Read,
        process::{Child, Command, ExitStatus, Stdio},
        time::Duration,
    },
    tracing::trace,
    wait_timeout::ChildExt,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceState {
    Connected,
    Disconnected,
    Unknown,
}

pub struct AdbShell;

impl AdbShell {
    /// Verify that ADB is available in PATH
    ///
    /// Should be called once at application startup to fail fast
    /// if ADB is not installed or not in PATH.
    pub fn verify_adb_available() -> Result<()> {
        Command::new("adb")
            .arg("version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| SAideError::AdbNotFound(e.to_string()))?
            .success()
            .then_some(())
            .ok_or_else(|| SAideError::AdbCommandFailed("non-zero exit status".to_string()))
    }

    /// Get Android device screen size using separate adb command
    pub fn get_physical_screen_size(serial: &str) -> Result<(u32, u32)> {
        check_serial(serial)?;

        run_adb_command(
            ["-s", serial, "shell", "wm", "size"],
            None,
            |status, output| {
                // Parse output like "Physical size: 1080x2340"
                status
                    .success()
                    .then(|| {
                        output
                            .lines()
                            .find(|line| line.contains("Physical size:"))
                            .and_then(|line| line.split(':').nth(1))
                            .and_then(|size_part| {
                                let size_str = size_part.trim();
                                let mut parts = size_str.split('x');
                                let width = parts.next().and_then(|w| w.trim().parse::<u32>().ok());
                                let height =
                                    parts.next().and_then(|h| h.trim().parse::<u32>().ok());

                                match (width, height) {
                                    (Some(w), Some(h)) => Some((w, h)),
                                    _ => None,
                                }
                            })
                    })
                    .flatten()
                    .ok_or_else(|| {
                        SAideError::AdbError(format!(
                            "Failed to parse screen size from output: {}",
                            output.trim()
                        ))
                    })
            },
        )
    }

    /// Get Android device screen orientation using separate adb command (with 3s timeout)
    pub fn get_screen_orientation(serial: &str) -> Result<u32> {
        check_serial(serial)?;

        run_adb_command(
            ["-s", serial, "shell", "dumpsys", "window", "displays"],
            Some(Duration::from_secs(3)),
            |status, output| {
                status
                    .success()
                    .then(|| {
                        output
                            .lines()
                            .find(|line| line.contains("mCurrentRotation"))
                            .and_then(|line| line.split('=').nth(1))
                            .and_then(|rotation_part| {
                                let rotation_str = rotation_part.trim();

                                match rotation_str {
                                    "ROTATION_0" | "0" => Some(0),
                                    "ROTATION_90" | "1" => Some(1),
                                    "ROTATION_180" | "2" => Some(2),
                                    "ROTATION_270" | "3" => Some(3),
                                    _ => None,
                                }
                            })
                    })
                    .flatten()
                    .ok_or_else(|| {
                        SAideError::AdbError(
                            "Failed to parse screen orientation in dumpsys output".to_string(),
                        )
                    })
            },
        )
    }

    /// Get android device input method state (with 3s timeout)
    pub fn get_ime_state(serial: &str) -> Result<bool> {
        check_serial(serial)?;

        run_adb_command(
            ["-s", serial, "shell", "dumpsys", "window", "InputMethod"],
            Some(Duration::from_secs(3)),
            |status, output| {
                // Parse output to find "isVisible=true" line
                Ok(status.success() && output.lines().any(|line| line.contains("isVisible=true")))
            },
        )
    }

    /// Get Android device serial using adb command, blocking until command completes.
    pub fn get_device_serial() -> Result<String> {
        run_adb_command(["get-serialno"], None, |status, output| {
            status
                .success()
                .then(|| output.trim().to_string())
                .ok_or_else(|| {
                    SAideError::AdbDeviceNotFound(format!("adb exited with status: {}", status))
                })
        })
    }

    /// Get ADB device state (device/offline/unauthorized/no device)
    pub fn get_device_state(serial: &str) -> Result<DeviceState> {
        check_serial(serial)?;

        run_adb_command(["-s", serial, "get-state"], None, |status, output| {
            if !status.success() {
                return Ok(DeviceState::Disconnected);
            }
            match output.trim() {
                "device" => Ok(DeviceState::Connected),
                _ => Ok(DeviceState::Unknown),
            }
        })
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

        run_adb_command(
            ["-s", serial, "shell", "getprop", prop_name],
            None,
            |status, output| {
                status
                    .success()
                    .then(|| output.trim().to_string())
                    .ok_or_else(|| {
                        SAideError::AdbError(format!(
                            "Failed to get property [{}], adb exited with status: {}",
                            prop_name, status
                        ))
                    })
            },
        )
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
            .stdout(Stdio::null())
            .stderr(Stdio::null())
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
                if stderr.contains("not found") || stderr.contains("No such reverse") {
                    return Ok(());
                }
                if !stderr.is_empty() {
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

/// Run adb command with given arguments and optional timeout, parsing output with provided closure.
fn run_adb_command<I, S, F, R>(args: I, timeout: Option<Duration>, parse: F) -> Result<R>
where
    I: IntoIterator<Item = S> + fmt::Debug,
    S: AsRef<OsStr>,
    F: FnOnce(ExitStatus, &str) -> Result<R>,
{
    trace!("Running adb command with args: {:?}", args);
    let mut child = Command::new("adb")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| SAideError::AdbError(format!("Failed to spawn adb: {e}")))?;

    let status = if let Some(to) = timeout {
        match child.wait_timeout(to).map_err(|e| {
            SAideError::AdbError(format!("Failed to wait with timeout for adb: {e}"))
        })? {
            Some(s) => s,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(SAideError::AdbError(
                    "Timeout while waiting for adb command".to_string(),
                ));
            }
        }
    } else {
        child
            .wait()
            .map_err(|e| SAideError::AdbError(format!("Failed to wait: {e}")))?
    };

    let output = read_child_stdout(&mut child)?;
    parse(status, &output)
}

/// Read child stdout safely (UTF-8 lossy)
fn read_child_stdout(child: &mut std::process::Child) -> Result<String> {
    let mut buf = Vec::new();
    if let Some(mut s) = child.stdout.take() {
        s.read_to_end(&mut buf)
            .map_err(|e| SAideError::AdbError(format!("Failed to read adb stdout: {}", e)))?;
    }
    Ok(String::from_utf8_lossy(&buf).to_string())
}

// Check if device serial is valid (non-empty)
fn check_serial(serial: &str) -> Result<()> {
    if serial.is_empty() {
        return Err(SAideError::AdbError(
            "Device serial cannot be empty".to_string(),
        ));
    }
    Ok(())
}
