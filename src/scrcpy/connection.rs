//! Scrcpy Connection Management
//!
//! Handles socket connections and ADB tunneling.
//! Reference: scrcpy/app/src/adb/adb_tunnel.c, scrcpy/app/src/server.c

use {
    super::server::{ServerParams, get_socket_name, push_server, start_server},
    anyhow::{Context, Result},
    std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        process::Child,
    },
    tracing::{debug, info, warn},
};

/// Default port range for ADB reverse/forward
const DEFAULT_PORT_RANGE: (u16, u16) = (27183, 27199);

/// Scrcpy connection with video, audio, and control channels
pub struct ScrcpyConnection {
    /// Session ID
    pub scid: u32,

    /// Video stream socket
    pub video_stream: Option<TcpStream>,

    /// Audio stream socket (optional)
    pub audio_stream: Option<TcpStream>,

    /// Control stream socket
    pub control_stream: Option<TcpStream>,

    /// Local TCP port used for tunneling
    pub local_port: u16,

    /// Server process handle
    server_process: Option<Child>,

    /// Device serial number
    serial: String,
}

impl ScrcpyConnection {
    /// Establish connection to scrcpy server
    ///
    /// # Steps
    /// 1. Push server JAR to device (if needed)
    /// 2. Setup ADB reverse tunnel
    /// 3. Start server process
    /// 4. Accept socket connections
    pub async fn connect(
        serial: &str,
        server_jar_path: &str,
        params: ServerParams,
    ) -> Result<Self> {
        let scid = params.scid;
        let socket_name = get_socket_name(scid);

        info!("Establishing connection to device: {}", serial);
        info!("SCID: {:08x}, socket: {}", scid, socket_name);

        // Step 1: Push server
        push_server(serial, server_jar_path).context("Failed to push server to device")?;

        // Step 2: Find available local port
        let local_port = find_available_port(DEFAULT_PORT_RANGE.0, DEFAULT_PORT_RANGE.1)
            .context("No available port in range")?;

        debug!("Using local port: {}", local_port);

        // Step 3: Setup ADB reverse tunnel
        setup_reverse_tunnel(serial, &socket_name, local_port)
            .context("Failed to setup reverse tunnel")?;

        // Step 4: Start listening on local port
        let listener = TcpListener::bind(format!("127.0.0.1:{}", local_port))
            .context("Failed to bind to local port")?;
        listener
            .set_nonblocking(false)
            .context("Failed to set blocking mode")?;

        debug!("Listening on 127.0.0.1:{}", local_port);

        // Step 5: Start server process
        let server_process =
            start_server(serial, &params).context("Failed to start server process")?;

        // Step 6: Accept connections in order: Video, Audio (optional), Control
        let video_stream = if params.video {
            Some(
                accept_connection(&listener, "video")
                    .context("Failed to accept video connection")?,
            )
        } else {
            None
        };

        let audio_stream = if params.audio {
            Some(
                accept_connection(&listener, "audio")
                    .context("Failed to accept audio connection")?,
            )
        } else {
            None
        };

        let control_stream = if params.control {
            Some(
                accept_connection(&listener, "control")
                    .context("Failed to accept control connection")?,
            )
        } else {
            None
        };

        info!("All sockets connected successfully");

        Ok(Self {
            scid,
            video_stream,
            audio_stream,
            control_stream,
            local_port,
            server_process: Some(server_process),
            serial: serial.to_string(),
        })
    }

    /// Send control message
    pub fn send_control(&mut self, data: &[u8]) -> Result<()> {
        if let Some(ref mut stream) = self.control_stream {
            stream
                .write_all(data)
                .context("Failed to write to control stream")?;
            stream.flush().context("Failed to flush control stream")?;
            Ok(())
        } else {
            anyhow::bail!("Control stream not available")
        }
    }

    /// Read video packet (blocking)
    pub fn read_video(&mut self, buf: &mut [u8]) -> Result<usize> {
        if let Some(ref mut stream) = self.video_stream {
            stream.read(buf).context("Failed to read from video stream")
        } else {
            anyhow::bail!("Video stream not available")
        }
    }

    /// Check if server process is still running
    pub fn is_server_alive(&mut self) -> bool {
        if let Some(ref mut process) = self.server_process {
            process.try_wait().ok().flatten().is_none()
        } else {
            false
        }
    }

    /// Gracefully shutdown connection
    pub fn shutdown(&mut self) -> Result<()> {
        debug!("Shutting down connection");

        // Close sockets
        self.video_stream.take();
        self.audio_stream.take();
        self.control_stream.take();

        // Kill server process
        if let Some(mut process) = self.server_process.take() {
            process.kill().ok();
            process.wait().ok();
        }

        // Remove reverse tunnel
        remove_reverse_tunnel(&self.serial, &get_socket_name(self.scid)).ok();

        info!("Connection closed");
        Ok(())
    }
}

impl Drop for ScrcpyConnection {
    fn drop(&mut self) { self.shutdown().ok(); }
}

/// Find an available TCP port in the given range
fn find_available_port(start: u16, end: u16) -> Result<u16> {
    for port in start..=end {
        if let Ok(listener) = TcpListener::bind(format!("127.0.0.1:{}", port)) {
            drop(listener);
            return Ok(port);
        }
    }
    anyhow::bail!("No available port in range {}..{}", start, end)
}

/// Setup ADB reverse tunnel
///
/// Command: `adb reverse localabstract:<socket_name> tcp:<local_port>`
fn setup_reverse_tunnel(serial: &str, socket_name: &str, local_port: u16) -> Result<()> {
    debug!(
        "Setting up reverse tunnel: {} -> tcp:{}",
        socket_name, local_port
    );

    let status = std::process::Command::new("adb")
        .args([
            "-s",
            serial,
            "reverse",
            &format!("localabstract:{}", socket_name),
            &format!("tcp:{}", local_port),
        ])
        .status()
        .context("Failed to execute adb reverse")?;

    if !status.success() {
        anyhow::bail!("adb reverse failed");
    }

    info!("Reverse tunnel established");
    Ok(())
}

/// Remove ADB reverse tunnel
fn remove_reverse_tunnel(serial: &str, socket_name: &str) -> Result<()> {
    debug!("Removing reverse tunnel: {}", socket_name);

    std::process::Command::new("adb")
        .args([
            "-s",
            serial,
            "reverse",
            "--remove",
            &format!("localabstract:{}", socket_name),
        ])
        .status()
        .ok();

    Ok(())
}

/// Accept a single connection with timeout and dummy byte handling
fn accept_connection(listener: &TcpListener, channel: &str) -> Result<TcpStream> {
    debug!("Waiting for {} connection...", channel);

    // Set accept timeout
    listener
        .set_nonblocking(false)
        .context("Failed to set blocking mode")?;

    // Accept connection
    let (mut stream, addr) = listener
        .accept()
        .context(format!("Failed to accept {} connection", channel))?;

    debug!("{} connection accepted from {}", channel, addr);

    // Read and verify dummy byte (should be 0x00)
    let mut dummy = [0u8; 1];
    stream
        .read_exact(&mut dummy)
        .context("Failed to read dummy byte")?;

    if dummy[0] != 0 {
        warn!(
            "{} channel: unexpected dummy byte value: 0x{:02x}",
            channel, dummy[0]
        );
    }

    debug!("{} channel ready", channel);
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_available_port() {
        let port = find_available_port(27183, 27199).unwrap();
        assert!(port >= 27183 && port <= 27199);
    }

    #[test]
    fn test_socket_name() {
        assert_eq!(get_socket_name(0x12345678), "scrcpy_12345678");
    }
}
