use {
    crate::config::mapping::AdbAction,
    anyhow::{Context, Result, anyhow},
    std::{
        io::Write,
        process::{Child, ChildStdin, Command, Stdio},
        time::{Duration, Instant},
    },
    tracing::{info, warn},
};

const ADB_SHELL_TIMEOUT: Duration = Duration::from_secs(5);
const RECONNECT_INTERVAL: Duration = Duration::from_secs(30);

/// ADB Shell connection manager for sending input commands to Android device
pub struct AdbShell {
    /// ADB shell child process
    child: Option<Child>,
    /// Stdin of the shell process for sending commands
    stdin: Option<ChildStdin>,
    /// Connection state
    connected: bool,
    /// Last activity timestamp
    last_activity: Instant,
    /// Device physical screen size
    screen_size: Option<(u32, u32)>,
}

impl AdbShell {
    /// Create a new ADB shell connection manager
    pub fn new() -> Self {
        Self {
            child: None,
            stdin: None,
            connected: false,
            last_activity: Instant::now(),
            screen_size: None,
        }
    }

    /// Connect to ADB shell
    pub fn connect(&mut self) -> Result<()> {
        info!("Connecting to ADB shell...");

        // Kill any existing connection
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
        }

        // Spawn new adb shell process
        let mut child = Command::new("adb")
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

        self.child = Some(child);
        self.stdin = Some(stdin);

        self.screen_size = Some(Self::get_physical_screen_size()?);

        self.last_activity = Instant::now();
        self.connected = true;

        info!("Successfully connected to ADB shell");
        Ok(())
    }

    /// Disconnect from ADB shell
    pub fn disconnect(&mut self) -> Result<()> {
        info!("Disconnecting from ADB shell...");

        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
        }

        self.connected = false;
        info!("Disconnected from ADB shell");
        Ok(())
    }

    /// Check if connected to ADB shell
    pub fn is_connected(&self) -> bool { self.connected }

    /// Get device screen size
    pub fn get_screen_size(&self) -> Option<(u32, u32)> { self.screen_size }

    /// Send input command to Android device
    pub fn send_input(&mut self, command: &AdbAction) -> Result<()> {
        if !self.is_connected() {
            warn!("ADB shell not connected, attempting to reconnect...");
            if let Err(e) = self.connect() {
                return Err(anyhow!("Failed to reconnect to ADB shell: {}", e));
            }
        }

        let Some(ref mut stdin) = self.stdin else {
            return Err(anyhow!("ADB shell stdin not available"));
        };

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

        stdin
            .write_all(cmd_str.as_bytes())
            .context("Failed to write input command")?;

        stdin.flush().context("Failed to flush input command")?;

        self.last_activity = Instant::now();

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
}

impl Drop for AdbShell {
    fn drop(&mut self) { let _ = self.disconnect(); }
}
