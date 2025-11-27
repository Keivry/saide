use {
    crate::config::mapping::AdbAction,
    anyhow::{Context, Result, anyhow},
    parking_lot::RwLock,
    std::{
        io::Write,
        process::{Child, ChildStdin, Command, Stdio},
        time::Instant,
    },
    tracing::{info, warn},
};

/// ADB Shell connection manager for sending input commands to Android device
pub struct AdbShell {
    /// ADB shell child process
    child: RwLock<Option<Child>>,
    /// Stdin of the shell process for sending commands
    stdin: RwLock<Option<ChildStdin>>,
    /// Connection state
    connected: RwLock<bool>,
    /// Last activity timestamp
    last_activity: RwLock<Instant>,
    /// Device physical screen size
    screen_size: RwLock<Option<(u32, u32)>>,
}

impl AdbShell {
    /// Create a new ADB shell connection manager
    pub fn new() -> Self {
        Self {
            child: RwLock::new(None),
            stdin: RwLock::new(None),
            connected: RwLock::new(false),
            last_activity: RwLock::new(Instant::now()),
            screen_size: RwLock::new(None),
        }
    }

    /// Connect to ADB shell
    pub fn connect(&self) -> Result<()> {
        info!("Connecting to ADB shell...");

        {
            // Kill any existing connection
            if let Some(mut child) = self.child.write().take() {
                let _ = child.kill();
            }
        }

        // Spawn new adb shell process
        let mut child = Command::new("stdbuf")
            .arg("-o0")
            .arg("adb")
            .arg("shell")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to spawn adb shell process")?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("Failed to take stdin from adb shell process"))?;

        {
            self.child.write().replace(child);
            self.stdin.write().replace(stdin);

            self.screen_size
                .write()
                .replace(Self::get_physical_screen_size()?);

            *self.last_activity.write() = Instant::now();
            *self.connected.write() = true;
        }

        info!("Successfully connected to ADB shell");
        Ok(())
    }

    /// Disconnect from ADB shell
    pub fn disconnect(&self) -> Result<()> {
        info!("Disconnecting from ADB shell...");

        {
            if let Some(mut child) = self.child.write().take() {
                let _ = child.kill();
            }
        }

        {
            *self.connected.write() = false;
        }

        info!("Disconnected from ADB shell");
        Ok(())
    }

    /// Check if connected to ADB shell
    pub fn is_connected(&self) -> bool { self.connected.read().clone() }

    /// Get device screen size
    pub fn get_screen_size(&self) -> Option<(u32, u32)> { self.screen_size.read().clone() }

    /// Send input command to Android device
    pub fn send_input(&self, command: &AdbAction) -> Result<()> {
        if !self.is_connected() {
            warn!("ADB shell not connected, attempting to reconnect...");
            if let Err(e) = self.connect() {
                return Err(anyhow!("Failed to reconnect to ADB shell: {}", e));
            }
        }

        let cmd_str = match command {
            AdbAction::Tap { x, y } => {
                format!("input tap {} {}\n", x, y)
            }
            AdbAction::Swipe {
                x1,
                y1,
                x2,
                y2,
                duration,
            } => {
                format!("input swipe {} {} {} {} {}\n", x1, y1, x2, y2, duration)
            }
            AdbAction::Key { keycode } => {
                format!("input keyevent {}\n", keycode)
            }
            AdbAction::Text { text } => {
                let escaped = text.replace(' ', "%s");
                format!("input text {}\n", escaped)
            }
            AdbAction::Back => "input keyevent BACK\n".to_string(),
            AdbAction::Home => "input keyevent HOME\n".to_string(),
            AdbAction::Menu => "input keyevent MENU\n".to_string(),
            AdbAction::Power => "input keyevent POWER\n".to_string(),
            AdbAction::Ignore => {
                return Ok(());
            }
        };

        {
            let mut stdin = self.stdin.write();
            let Some(stdin) = stdin.as_mut() else {
                return Err(anyhow!("ADB shell stdin not available"));
            };

            stdin
                .write_all(cmd_str.as_bytes())
                .context("Failed to write input command")?;

            stdin.flush().context("Failed to flush input command")?;
        }

        {
            *self.last_activity.write() = Instant::now();
        }

        info!("Sent ADB input command: {}", cmd_str.trim());
        Ok(())
    }

    /// Get Android device screen size using separate adb command
    pub fn get_physical_screen_size() -> Result<(u32, u32)> {
        // Use separate adb command to get screen size, not through shell session
        let output = Command::new("adb")
            .args(["shell", "wm size"])
            .output()
            .context("Failed to execute adb shell wm size command")?;

        let output_str = String::from_utf8_lossy(&output.stdout);
        info!("wm size output: {}", output_str.trim());

        // Parse output like "Physical size: 1080x2340"
        if let Some(line) = output_str
            .lines()
            .find(|line| line.contains("Physical size:"))
            && let Some(size_part) = line.split(':').nth(1)
        {
            let size_str = size_part.trim();
            let parts: Vec<&str> = size_str.split('x').collect();
            if parts.len() == 2 {
                let width = parts[0].trim().parse::<u32>()?;
                let height = parts[1].trim().parse::<u32>()?;
                return Ok((width, height));
            }
        }

        Err(anyhow!(
            "Failed to parse screen size from output: {}",
            output_str.trim()
        ))
    }

    pub fn get_screen_orientation(&self) -> Result<u32> {
        {
            let mut stdin = self.stdin.write();
            if let Some(stdin) = stdin.as_mut() {
                stdin.write_all(b"dumpsys window displays | grep 'mCurrentRotation'\n")?;
                stdin.flush()?;
            }
        }

        {
            let child = self.child.write();
            let Some(child) = child.as_ref() else {
                return Err(anyhow!("ADB shell child process not available"));
            };

            let output = child
                .wait_with_output()
                .context("Failed to read dumpsys output")?;

            let output_str = String::from_utf8_lossy(&output.stdout);

            // Parse the output to find the current rotation
            for line in output_str.lines() {
                if line.contains("mCurrentRotation") {
                    if let Some(rotation_part) = line.split('=').nth(1) {
                        let rotation_str = rotation_part.trim();
                        if let Ok(rotation) = rotation_str.parse::<u32>() {
                            return Ok(match rotation {
                                0 => 0,
                                90 => 1,
                                180 => 2,
                                270 => 3,
                                _ => return Err(anyhow!("Unknown rotation value: {}", rotation)),
                            });
                        }
                    }
                }
            }
        }

        Ok(0) // Placeholder: Implement actual parsing of orientation from dumpsys output
    }
}

impl Drop for AdbShell {
    fn drop(&mut self) { let _ = self.disconnect(); }
}
