use {
    anyhow::{Context, Result, anyhow},
    std::{
        process::{Command, Stdio},
        thread,
    },
    tracing::{debug, trace},
};

pub struct AdbShell;

impl AdbShell {
    /// Get Android device screen size using separate adb command
    pub fn get_physical_screen_size() -> Result<(u32, u32)> {
        // Use separate adb command to get screen size, not through shell session
        let output = Command::new("adb")
            .args(["shell", "wm size"])
            .output()
            .context("Failed to execute adb shell wm size command")?;

        let output_str = String::from_utf8_lossy(&output.stdout);
        debug!("wm size output: {}", output_str.trim());

        // Parse output like "Physical size: 1080x2340"
        let Some(line) = output_str
            .lines()
            .find(|line| line.contains("Physical size:"))
        else {
            return Err(anyhow!(
                "Failed to find 'Physical size:' in output: {}",
                output_str.trim()
            ));
        };

        let Some(size_part) = line.split(':').nth(1) else {
            return Err(anyhow!("Failed to parse size from line: {}", line));
        };

        let size_str = size_part.trim();
        let mut parts = size_str.split('x');
        let width = parts
            .next()
            .and_then(|w| w.trim().parse::<u32>().ok())
            .ok_or_else(|| anyhow!("Failed to parse width from: {}", size_str))?;
        let height = parts
            .next()
            .and_then(|h| h.trim().parse::<u32>().ok())
            .ok_or_else(|| anyhow!("Failed to parse height from: {}", size_str))?;

        Ok((width, height))
    }

    /// Get Android device screen orientation using separate adb command (with 3s timeout)
    pub fn get_screen_orientation() -> Result<u32> {
        use std::{sync::mpsc, time::Duration};

        let child = Command::new("adb")
            .args(["shell", "dumpsys window displays | grep mCurrentRotation"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .context("Failed to spawn adb command")?;

        // Wait with 3 second timeout
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = tx.send(child.wait_with_output());
        });

        let output = match rx.recv_timeout(Duration::from_secs(3)) {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => anyhow::bail!("adb command failed: {}", e),
            Err(_) => anyhow::bail!("adb command timed out after 3 seconds"),
        };

        if !output.status.success() {
            anyhow::bail!("adb command failed with status: {}", output.status);
        }

        let output_str = String::from_utf8_lossy(&output.stdout);
        trace!(
            "Check screen rotation command output: {}",
            output_str.trim()
        );

        // Parse output like "mCurrentRotation=ROTATION_0"
        let Some(line) = output_str
            .lines()
            .find(|line| line.contains("mCurrentRotation"))
        else {
            return Err(anyhow!(
                "Failed to find 'mCurrentRotation' in output: {}",
                output_str.trim()
            ));
        };

        let Some(rotation_part) = line.split('=').nth(1) else {
            return Err(anyhow!("Failed to parse rotation from line: {}", line));
        };

        let rotation_str = rotation_part.trim();
        // Match rotation strings like "ROTATION_0", "ROTATION_90", etc.
        Ok(match rotation_str {
            "ROTATION_0" => 0,
            "ROTATION_90" => 1,
            "ROTATION_180" => 2,
            "ROTATION_270" => 3,
            other => return Err(anyhow!("Unknown rotation value: {}", other)),
        })
    }

    /// Get android device input method state (with 3s timeout)
    pub fn get_ime_state() -> Result<bool> {
        use std::{sync::mpsc, time::Duration};

        let child = Command::new("adb")
            .args([
                "shell",
                "dumpsys window InputMethod | grep 'isVisible=true'",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("Failed to spawn adb command")?;

        // Wait with 3 second timeout
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = tx.send(child.wait_with_output());
        });

        let output = match rx.recv_timeout(Duration::from_secs(3)) {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => anyhow::bail!("adb command failed: {}", e),
            Err(_) => anyhow::bail!("adb command timed out after 3 seconds"),
        };

        Ok(output.status.success())
    }

    pub fn get_device_id() -> Result<String> {
        let output = Command::new("adb")
            .args(["get-serialno"])
            .output()
            .context("Failed to execute adb get-serialno command")?;

        let output_str = String::from_utf8_lossy(&output.stdout);
        let device_id = output_str.trim().to_string();
        Ok(device_id)
    }
}
