use {
    crate::config::mapping::{AdbAction, WheelDirection},
    anyhow::{Context, Result, anyhow},
    parking_lot::Mutex,
    std::{
        io::{BufRead, BufReader, Write},
        process::{Child, ChildStdin, Command, Stdio},
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
            mpsc,
        },
        thread,
        time::Duration,
    },
    tracing::{debug, trace, warn},
};

const MODIFIER_ALT: u8 = 57; // KEYCODE_ALT_LEFT
const MODIFIER_CTRL: u8 = 113; // KEYCODE_CTRL_LEFT
const MODIFIER_SHIFT: u8 = 59; // KEYCODE_SHIFT_LEFT

const THREAD_JOIN_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Default)]
struct AdbShellState {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
}

/// Output capture state (only created when capture_output is enabled)
struct OutputCapture {
    buffer: Arc<Mutex<Vec<String>>>,
    reader_thread: Option<thread::JoinHandle<()>>,
    shutdown_signal: Arc<AtomicBool>,
}

/// ADB Shell connection manager for sending input commands to Android device
pub struct AdbShell {
    /// State of the ADB shell connection
    state: Mutex<AdbShellState>,

    /// Output capture (only if enabled)
    output_capture: Mutex<Option<OutputCapture>>,
}

impl AdbShell {
    /// Create a new ADB shell connection manager
    pub fn new(capture_output: bool) -> Self {
        Self {
            state: Mutex::new(AdbShellState::default()),
            output_capture: Mutex::new(if capture_output {
                Some(OutputCapture {
                    buffer: Arc::new(Mutex::new(Vec::new())),
                    reader_thread: None,
                    shutdown_signal: Arc::new(AtomicBool::new(false)),
                })
            } else {
                None
            }),
        }
    }

    /// Connect to ADB shell
    pub fn connect(&self) -> Result<()> {
        debug!("Connecting to ADB shell...");

        // Hold lock for entire connect operation to prevent race conditions
        let mut state = self.state.lock();

        // Kill any existing connection
        if state.child.is_some() {
            drop(state); // Release lock before disconnect
            self.disconnect()?;
            state = self.state.lock(); // Re-acquire lock
        }

        // Spawn new adb shell process
        let mut child = Command::new("stdbuf")
            .arg("-o0")
            .arg("adb")
            .arg("shell")
            .stdin(Stdio::piped())
            .stdout(if self.output_capture.lock().is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stderr(Stdio::null())
            .spawn()
            .context("Failed to spawn adb shell process")?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("Failed to take stdin from adb shell process"))?;

        // Start output capture thread if enabled
        if let Some(ref mut capture) = *self.output_capture.lock() {
            let stdout = child
                .stdout
                .take()
                .ok_or_else(|| anyhow!("Failed to take stdout from adb shell process"))?;

            // Clear output buffer and reset shutdown signal
            capture.buffer.lock().clear();
            capture.shutdown_signal.store(false, Ordering::SeqCst);

            // Start background reader thread
            let output_buffer = Arc::clone(&capture.buffer);
            let shutdown_signal = Arc::clone(&capture.shutdown_signal);
            let reader_thread = thread::spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    if shutdown_signal.load(Ordering::SeqCst) {
                        debug!("ADB output reader received shutdown signal");
                        break;
                    }
                    match line {
                        Ok(line) => {
                            output_buffer.lock().push(line);
                        }
                        Err(e) => {
                            debug!("ADB output reader error: {}", e);
                            break;
                        }
                    }
                }
                debug!("ADB output reader thread exiting");
            });

            capture.reader_thread = Some(reader_thread);
        }

        // Update connection state
        state.child = Some(child);
        state.stdin = Some(stdin);

        debug!("Successfully connected to ADB shell");
        Ok(())
    }

    /// Disconnect from ADB shell
    pub fn disconnect(&self) -> Result<()> {
        debug!("Disconnecting from ADB shell...");

        // Signal and join reader thread with timeout
        if let Some(ref mut capture) = *self.output_capture.lock() {
            // Signal shutdown
            capture.shutdown_signal.store(true, Ordering::SeqCst);

            // Close stdin to unblock reader (stdin.read will return EOF)
            self.state.lock().stdin.take();

            if let Some(thread) = capture.reader_thread.take() {
                // Wait for thread with timeout using channel
                let (tx, rx) = mpsc::channel();
                thread::spawn(move || {
                    let _ = thread.join();
                    let _ = tx.send(());
                });

                match rx.recv_timeout(THREAD_JOIN_TIMEOUT) {
                    Ok(_) => debug!("Reader thread exited cleanly"),
                    Err(_) => {
                        warn!(
                            "Reader thread join timed out after {:?}",
                            THREAD_JOIN_TIMEOUT
                        );
                    }
                }
            }
        }

        let mut state = self.state.lock();
        if let Some(mut child) = state.child.take() {
            drop(state); // Release lock before potentially long-running wait

            // Send SIGTERM first for graceful shutdown
            #[cfg(unix)]
            {
                if let Ok(pid) = child.id().try_into() {
                    unsafe {
                        libc::kill(pid, libc::SIGTERM);
                    }
                }
            }

            // Wait with timeout using channel
            let (tx, rx) = mpsc::channel();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(500));
                let _ = child.kill();
                let result = child.wait();
                let _ = tx.send(result);
            });

            match rx.recv_timeout(Duration::from_secs(2)) {
                Ok(Ok(_)) => debug!("ADB shell process exited cleanly"),
                Ok(Err(e)) => warn!("Failed to wait for adb shell process: {}", e),
                Err(_) => warn!("Wait for adb shell process timed out"),
            }
        }

        debug!("Disconnected from ADB shell");
        Ok(())
    }

    /// Get buffered output lines (if capture_output was enabled)
    #[allow(dead_code)]
    pub fn read_output(&self) -> Option<Vec<String>> {
        self.output_capture
            .lock()
            .as_ref()
            .map(|capture| capture.buffer.lock().drain(..).collect())
    }

    /// Send input command to Android device
    pub fn send_input(&self, command: &AdbAction) -> Result<()> {
        let cmd_str = match command {
            AdbAction::Tap { x, y } => {
                format!(
                    "input motionevent DOWN {} {} && input motionevent UP {} {}\n",
                    x, y, x, y
                )
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
                let mut cmd = String::with_capacity(64);
                cmd.push_str("input keycombination ");
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
                format!("input text {}\n", text)
            }
            AdbAction::Back => "input keyevent BACK\n".to_string(),
            AdbAction::Home => "input keyevent HOME\n".to_string(),
            AdbAction::Menu => "input keyevent MENU\n".to_string(),
            AdbAction::Power => "input keyevent POWER\n".to_string(),
            AdbAction::Ignore => {
                return Ok(());
            }
        };

        let mut state = self.state.lock();
        let Some(ref mut stdin) = state.stdin else {
            return Err(anyhow!("ADB shell not connected"));
        };

        stdin
            .write_all(cmd_str.as_bytes())
            .context("Failed to write input command")?;

        stdin.flush().context("Failed to flush input command")?;
        drop(state);

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

    /// Get android device input method state
    pub fn get_ime_state() -> Result<bool> {
        let exit_status = Command::new("adb")
            .args([
                "shell",
                "dumpsys window InputMethod | grep 'isVisible=true'",
            ])
            .stdout(Stdio::null())
            .status()
            .context("Failed to execute adb shell dumpsys window InputMethod command")?;

        Ok(exit_status.success())
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
