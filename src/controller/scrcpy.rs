use {
    super::utils::{kill_pg, spawn_pg},
    crate::config::scrcpy::ScrcpyConfig,
    anyhow::{Context, anyhow},
    crossbeam_channel::unbounded,
    std::{
        io::{BufRead, BufReader},
        process::{Child, Command},
        sync::Arc,
        thread,
        time::{Duration, Instant},
    },
};

const EXPECT_OUTPUT: &str = r"INFO: v4l2 sink started to device: {device}";

pub enum ScrcpyState {
    NotStarted,
    Started,
    Ready,
    Exited,
    Timeout,
}

pub struct Scrcpy {
    pub config: Arc<ScrcpyConfig>,
    pub child: Option<Child>,
    pub pid: Option<i32>,
    pub state: ScrcpyState,
}

impl Scrcpy {
    /// Create a new ScrcpyManager
    pub fn new(config: Arc<ScrcpyConfig>) -> Self {
        Self {
            config,
            child: None,
            pid: None,
            state: ScrcpyState::NotStarted,
        }
    }

    pub fn spawn(&mut self) -> anyhow::Result<&mut Self> {
        let args = self.build_args();

        let (child, pid) =
            spawn_pg("scrcpy", &args).with_context(|| "Failed to spawn scrcpy process")?;

        self.child = Some(child);
        self.pid = Some(pid);

        self.state = ScrcpyState::Started;

        Ok(self)
    }

    pub fn terminate(&mut self) -> anyhow::Result<()> {
        if let Some(pgid) = self.pid {
            kill_pg(pgid, false).with_context(|| "Failed to terminate scrcpy process")?;
            self.child = None;
            self.pid = None;
            self.state = ScrcpyState::Exited;
        }
        Ok(())
    }

    pub fn is_running(&mut self) -> bool {
        if let Some(child) = self.child.as_mut() {
            match child.try_wait() {
                Ok(Some(_)) => false, // Process has exited
                Ok(None) => true,     // Process is still running
                Err(_) => false,      // Error occurred, assume not running
            }
        } else {
            false // No child process
        }
    }

    /// Wait for scrcpy to output the expected v4l2 sink readiness message.
    pub fn wait_for_ready(&mut self, timeout: Duration) -> anyhow::Result<()> {
        if !self.is_running() {
            self.state = ScrcpyState::Exited;
            return Err(anyhow!("scrcpy process has exited unexpectedly"));
        }

        if let Some(child) = self.child.as_mut() {
            let stdout = child
                .stdout
                .take()
                .context("Failed to take scrcpy stdout")?;

            let start = Instant::now();
            let expected_output = EXPECT_OUTPUT.replace("{device}", &self.config.v4l2.device);

            let (tx, rx) = unbounded::<ScrcpyState>();

            // Thread to read stdout lines
            thread::spawn({
                let mut reader = BufReader::new(stdout);
                let mut line = String::new();

                move || {
                    while let Ok(bytes_read) = reader.read_line(&mut line) {
                        if bytes_read == 0 {
                            break; // EOF
                        }

                        if line.contains(&expected_output) {
                            let _ = tx.send(ScrcpyState::Ready);
                            break;
                        }
                        line.clear();
                    }
                }
            });

            // Main loop to wait for events or timeout
            while start.elapsed() < timeout {
                if let Ok(state) = rx.recv_timeout(Duration::from_millis(100))
                    && let ScrcpyState::Ready = state
                {
                    self.state = ScrcpyState::Ready;
                    return Ok(());
                }
            }

            self.state = ScrcpyState::Timeout;
            return Err(anyhow!("Timeout to wait for scrcpy to be ready"));
        }

        // If we reach here, scrcpy process is not running
        Err(anyhow!("scrcpy process is not running"))
    }

    /// Build scrcpy command arguments
    fn build_args(&self) -> Vec<String> {
        let mut args = Vec::new();

        // Force no-window mode (critical parameter)
        args.push("--no-window".to_string());

        // Force --no-clipboard-autosync to avoid scrcpy crash
        args.push("--no-clipboard-autosync".to_string());

        // V4L2 related configuration
        if !self.config.v4l2.device.is_empty() {
            args.push("--v4l2-sink".to_string());
            args.push(self.config.v4l2.device.clone());
        }

        args.push("--v4l2-buffer".to_string());
        args.push(self.config.v4l2.buffer.to_string());

        args.push("--capture-orientation".to_string());
        args.push("@".to_string() + &self.config.v4l2.capture_orientation.to_string());

        // Video encoding parameters
        if !self.config.video.bit_rate.is_empty() {
            args.push("--video-bit-rate".to_string());
            args.push(self.config.video.bit_rate.clone());
        }

        if self.config.video.max_fps > 0 {
            args.push("--max-fps".to_string());
            args.push(self.config.video.max_fps.to_string());
        }

        if self.config.video.max_size > 0 {
            args.push("--max-size".to_string());
            args.push(self.config.video.max_size.to_string());
        }

        if !self.config.video.codec.is_empty() {
            args.push("--video-codec".to_string());
            args.push(self.config.video.codec.clone());
        }

        if self.config.video.encoder.is_some() {
            args.push("--video-encoder".to_string());
            args.push(self.config.video.encoder.clone().unwrap());
        }

        // Other options
        if self.config.options.turn_screen_off {
            args.push("--turn-screen-off".to_string());
        }

        if self.config.options.stay_awake {
            args.push("--stay-awake".to_string());
        }

        args
    }
}
