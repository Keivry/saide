use {
    crate::config::mapping::AdbAction,
    anyhow::{Context, Result, anyhow},
    parking_lot::RwLock,
    std::{
        io::{BufRead, BufReader, Write},
        process::{Child, ChildStdin, Command, Stdio},
        sync::{Arc, Condvar, Mutex},
        thread,
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
    /// Output buffer for shell responses (thread-safe)
    output_buffer: Arc<Mutex<Vec<String>>>,
    /// Condition variable for signaling new output
    output_condvar: Arc<Condvar>,
    /// Background thread handle
    reader_thread: RwLock<Option<thread::JoinHandle<()>>>,
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
            output_buffer: Arc::new(Mutex::new(Vec::new())),
            output_condvar: Arc::new(Condvar::new()),
            reader_thread: RwLock::new(None),
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

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Failed to take stdout from adb shell process"))?;

        // Clear output buffer
        {
            let mut buffer = self.output_buffer.lock().unwrap();
            buffer.clear();
        }

        // Start background reader thread (take ownership of stdout)
        let output_buffer = Arc::clone(&self.output_buffer);
        let output_condvar = Arc::clone(&self.output_condvar);
        let reader_thread = thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        let mut buffer = output_buffer.lock().unwrap();
                        buffer.push(line);
                        // Notify waiting threads
                        output_condvar.notify_one();
                    }
                    Err(_) => break,
                }
            }
        });

        {
            self.child.write().replace(child);
            self.stdin.write().replace(stdin);

            self.screen_size
                .write()
                .replace(Self::get_physical_screen_size()?);

            *self.last_activity.write() = Instant::now();
            *self.connected.write() = true;
            *self.reader_thread.write() = Some(reader_thread);
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

        // Join the reader thread
        {
            let mut reader_thread = self.reader_thread.write();
            if let Some(thread) = reader_thread.take() {
                drop(thread);
            }
        }

        {
            *self.connected.write() = false;
        }

        info!("Disconnected from ADB shell");
        Ok(())
    }

    /// Check if connected to ADB shell
    pub fn is_connected(&self) -> bool { *self.connected.read() }

    /// Get device screen size
    pub fn get_screen_size(&self) -> Option<(u32, u32)> { *self.screen_size.read() }

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

    /// Get Android device screen orientation using existing shell connection
    pub fn get_screen_orientation(&self) -> Result<u32> {
        // Check if shell is connected
        if !self.is_connected() {
            return Err(anyhow!("ADB shell not connected"));
        }

        // Generate unique marker using timestamp
        static COUNTER: parking_lot::Mutex<u64> = parking_lot::Mutex::new(0);
        let counter = {
            let mut c = COUNTER.lock();
            *c = c.wrapping_add(1);
            *c
        };

        let marker = format!(
            "SAIDE_ROTATION_{}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis(),
            counter
        );
        {
            let mut stdin = self.stdin.write();
            let Some(stdin) = stdin.as_mut() else {
                return Err(anyhow!("ADB shell stdin not available"));
            };

            // Send command with unique marker
            let cmd = format!(
                "echo '{}'; dumpsys window displays | grep mCurrentRotation; echo 'SAIDE_END'\n",
                marker
            );
            stdin.write_all(cmd.as_bytes())?;
            stdin.flush()?;
        }

        // Wait for and parse response
        let rotation =
            self.wait_for_response_with_marker(&marker, std::time::Duration::from_millis(500))?;

        Ok(rotation)
    }

    /// Wait for shell response with specific marker
    fn wait_for_response_with_marker(
        &self,
        marker: &str,
        timeout: std::time::Duration,
    ) -> Result<u32> {
        let start = std::time::Instant::now();

        loop {
            // Check timeout
            if start.elapsed() > timeout {
                return Err(anyhow!("Timeout waiting for shell response"));
            }

            // Lock buffer and check for marker
            let buffer = self.output_buffer.lock().unwrap();
            let lines: Vec<String> = buffer.clone();
            drop(buffer);

            // Look for our marker in recent output
            let marker_idx = lines.iter().position(|line: &String| line.contains(marker));

            if let Some(idx) = marker_idx {
                // Found marker, collect all lines after it
                let mut response_lines: Vec<String> = Vec::new();
                for line in lines.iter().skip(idx + 1) {
                    if line.contains("SAIDE_END") {
                        break;
                    }
                    if !line.is_empty() {
                        response_lines.push(line.clone());
                    }
                }

                // Clear processed lines from buffer
                {
                    let mut buffer = self.output_buffer.lock().unwrap();
                    if idx > 0 {
                        buffer.drain(0..idx);
                    }
                }

                // Parse rotation from response
                for line in &response_lines {
                    if line.contains("mCurrentRotation")
                        && let Some(rotation_part) = line.split('=').nth(1)
                    {
                        let rotation_str = rotation_part.trim();
                        // Match rotation strings like "ROTATION_0", "ROTATION_90", etc.
                        return Ok(match rotation_str {
                            "ROTATION_0" => 0,
                            "ROTATION_90" => 1,
                            "ROTATION_180" => 2,
                            "ROTATION_270" => 3,
                            other => return Err(anyhow!("Unknown rotation value: {}", other)),
                        });
                    }
                }

                return Err(anyhow!(
                    "Failed to parse rotation from response: {:?}",
                    response_lines
                ));
            }

            // Wait a bit before checking again
            thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}

impl Drop for AdbShell {
    fn drop(&mut self) { let _ = self.disconnect(); }
}
