use {
    crate::config::mapping::{AdbAction, WheelDirection},
    anyhow::{Context, Result, anyhow},
    parking_lot::Mutex,
    std::{
        io::{BufRead, BufReader, Write},
        process::{Child, ChildStdin, Command, Stdio},
        sync::{Arc, Condvar},
        thread,
    },
    tracing::{debug, info, trace, warn},
};

const MODIFIER_ALT: u8 = 57; // KEYCODE_ALT_LEFT
const MODIFIER_CTRL: u8 = 113; // KEYCODE_CTRL_LEFT
const MODIFIER_SHIFT: u8 = 59; // KEYCODE_SHIFT_LEFT

/// ADB Shell connection manager for sending input commands to Android device
pub struct AdbShell {
    /// ADB shell child process
    child: Mutex<Option<Child>>,
    /// Stdin of the shell process for sending commands
    stdin: Mutex<Option<ChildStdin>>,
    /// Connection state
    connected: Mutex<bool>,
    /// Output buffer for shell responses (thread-safe)
    output_buffer: Arc<Mutex<Option<Vec<String>>>>,
    /// Condition variable for signaling new output
    output_condvar: Arc<Option<Condvar>>,
    /// Background thread handle
    reader_thread: Mutex<Option<thread::JoinHandle<()>>>,

    /// Capture output flag
    capture_output: bool,
}

impl AdbShell {
    /// Create a new ADB shell connection manager
    pub fn new(capture_output: bool) -> Self {
        Self {
            child: Mutex::new(None),
            stdin: Mutex::new(None),
            connected: Mutex::new(false),
            output_buffer: Arc::new(Mutex::new(capture_output.then_some(Vec::new()))),
            output_condvar: Arc::new(capture_output.then_some(Condvar::new())),
            reader_thread: Mutex::new(None),
            capture_output,
        }
    }

    /// Connect to ADB shell
    pub fn connect(&self) -> Result<()> {
        info!("Connecting to ADB shell...");

        {
            // Kill any existing connection
            if let Some(mut child) = self.child.lock().take() {
                let _ = child.kill();
            }
        }

        // Spawn new adb shell process
        let mut child = Command::new("stdbuf")
            .arg("-o0")
            .arg("adb")
            .arg("shell")
            .stdin(Stdio::piped())
            .stdout(if self.capture_output {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stderr(if self.capture_output {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .spawn()
            .context("Failed to spawn adb shell process")?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("Failed to take stdin from adb shell process"))?;

        // Start output capture thread if enabled
        if self.capture_output {
            let stdout = child
                .stdout
                .take()
                .ok_or_else(|| anyhow!("Failed to take stdout from adb shell process"))?;

            // Clear output buffer
            {
                let mut buffer = self.output_buffer.lock();
                buffer.as_mut().unwrap().clear();
            }

            // Start background reader thread (take ownership of stdout)
            let output_buffer = Arc::clone(&self.output_buffer);
            let output_condvar = Arc::clone(&self.output_condvar);
            let reader_thread = thread::spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    match line {
                        Ok(line) => {
                            let mut buffer = output_buffer.lock();
                            buffer.as_mut().unwrap().push(line);
                            if let Some(condvar) = &*output_condvar {
                                condvar.notify_one();
                            }
                        }
                        Err(_) => break,
                    }
                }
            });

            {
                *self.reader_thread.lock() = Some(reader_thread);
            }
        }

        // Update connection state
        {
            self.child.lock().replace(child);
            self.stdin.lock().replace(stdin);
            *self.connected.lock() = true;
        }

        info!("Successfully connected to ADB shell");
        Ok(())
    }

    /// Disconnect from ADB shell
    pub fn disconnect(&self) -> Result<()> {
        info!("Disconnecting from ADB shell...");

        {
            if let Some(mut child) = self.child.lock().take() {
                let _ = child.kill();
            }
        }

        // Join the reader thread
        {
            let mut reader_thread = self.reader_thread.lock();
            if let Some(thread) = reader_thread.take() {
                drop(thread);
            }
        }

        {
            *self.connected.lock() = false;
        }

        info!("Disconnected from ADB shell");
        Ok(())
    }

    /// Check if connected to ADB shell
    pub fn is_connected(&self) -> bool { *self.connected.lock() }

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
            AdbAction::TouchDown { x, y } => {
                format!("input motionevent DOWN {} {}\n", x, y)
            }
            AdbAction::TouchMove { x, y } => {
                format!("input motionevent MOVE {} {}\n", x, y)
            }
            AdbAction::TouchUp { x, y } => {
                format!("input motionevent UP {} {}\n", x, y)
            }
            AdbAction::Scroll { x, y, direction } => match direction {
                WheelDirection::Up => format!("input mouse scroll {} {} --axis VSCROLL,-5\n", x, y),
                WheelDirection::Down => {
                    format!("input mouse scroll {} {} --axis VSCROLL,5\n", x, y)
                }
            },
            AdbAction::Key { keycode } => {
                format!("input keyevent {}\n", keycode)
            }
            AdbAction::KeyCombo { modifiers, keycode } => {
                // Build key combination command
                let mut cmd = "input keycombination ".to_string();
                if modifiers.alt {
                    cmd.push_str(&format!("{} ", MODIFIER_ALT));
                }
                if modifiers.ctrl || modifiers.command {
                    cmd.push_str(&format!("{} ", MODIFIER_CTRL));
                }
                if modifiers.shift {
                    cmd.push_str(&format!("{} ", MODIFIER_SHIFT));
                }
                cmd.push_str(&format!("{}\n", keycode));
                cmd
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
            let mut stdin = self.stdin.lock();
            let Some(stdin) = stdin.as_mut() else {
                return Err(anyhow!("ADB shell stdin not available"));
            };

            stdin
                .write_all(cmd_str.as_bytes())
                .context("Failed to write input command")?;

            stdin.flush().context("Failed to flush input command")?;
        }

        debug!("Sent ADB input command: {}", cmd_str.trim());
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
        debug!("wm size output: {}", output_str.trim());

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

    /// Get Android device screen orientation using separate adb command
    pub fn get_screen_orientation() -> Result<u32> {
        // Use separate adb command to get screen orientation, not through shell session
        let output = Command::new("adb")
            .args(["shell", "dumpsys window displays | grep mCurrentRotation"])
            .output()
            .context("Failed to execute adb shell dumpsys window displays command")?;

        let output_str = String::from_utf8_lossy(&output.stdout);
        trace!(
            "Check screen rotation command output: {}",
            output_str.trim()
        );

        // Parse output like "mCurrentRotation=ROTATION_0"
        if let Some(line) = output_str
            .lines()
            .find(|line| line.contains("mCurrentRotation"))
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

        Err(anyhow!(
            "Failed to parse screen orientation from output: {}",
            output_str.trim()
        ))
    }

    // Get android device input method state
    pub fn get_ime_state() -> Result<bool> {
        let exit_status = Command::new("adb")
            .args([
                "shell",
                "dumpsys window InputMethod | grep 'isVisible=true'",
            ])
            .stdout(Stdio::null())
            .status()
            .context("Failed to execute adb shell dumpsys window InputMethod command")?;

        if exit_status.success() {
            return Ok(true);
        }
        Ok(false)
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

impl Drop for AdbShell {
    fn drop(&mut self) { let _ = self.disconnect(); }
}
