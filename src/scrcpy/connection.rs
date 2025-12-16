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
    tracing::{debug, info},
};

/// Default port range for ADB reverse/forward
const DEFAULT_PORT_RANGE: (u16, u16) = (27183, 27199);

/// Scrcpy connection with video, audio, and control channels
pub struct ScrcpyConnection {
    /// Session ID
    pub scid: u32,

    /// Device name (from device meta)
    pub device_name: Option<String>,

    /// Video resolution (width, height) from codec meta
    pub video_resolution: Option<(u32, u32)>,

    /// Video stream socket
    pub video_stream: Option<TcpStream>,

    /// Audio stream socket (optional)
    pub audio_stream: Option<TcpStream>,

    /// Audio disabled reason (if audio was requested but unavailable)
    pub audio_disabled_reason: Option<String>,

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
        mut params: ServerParams,
    ) -> Result<Self> {
        let scid = params.scid;
        let socket_name = get_socket_name(scid);

        info!("Establishing connection to device: {}", serial);
        info!("SCID: {:08x}, socket: {}", scid, socket_name);

        // Step 0: Check Android version and disable audio if unsupported
        let mut audio_disabled_reason = None;

        if params.audio {
            let android_version = super::server::get_android_version(serial)
                .context("Failed to get Android version")?;

            // Audio capture requires Android 11 (API 30) or higher
            if android_version < 30 {
                let reason = format!(
                    "Audio capture requires Android 11+ (API 30+). Device is Android {} (API {}).",
                    if android_version >= 29 { "10" } else { "<10" },
                    android_version
                );
                tracing::warn!("{} Disabling audio.", reason);
                audio_disabled_reason = Some(reason);
                params.audio = false;
            } else {
                info!("Audio capture supported (Android API {})", android_version);
            }
        }

        // Step 1: Push server
        push_server(serial, server_jar_path).context("Failed to push server to device")?;

        // Step 2: Find available local port and start listening FIRST
        // (as per scrcpy: client must listen before server starts)
        let local_port = find_available_port(DEFAULT_PORT_RANGE.0, DEFAULT_PORT_RANGE.1)
            .context("No available port in range")?;

        let listener = TcpListener::bind(format!("127.0.0.1:{}", local_port))
            .context("Failed to bind to local port")?;
        listener
            .set_nonblocking(false)
            .context("Failed to set blocking mode")?;

        debug!("Listening on 127.0.0.1:{}", local_port);

        // Step 3: Setup ADB reverse tunnel (after listener is ready)
        setup_reverse_tunnel(serial, &socket_name, local_port)
            .context("Failed to setup reverse tunnel")?;

        // Step 4: Start server process (it will connect to our listener via tunnel)
        let server_process =
            start_server(serial, &params).context("Failed to start server process")?;

        debug!("Server started, waiting for connections...");

        // Step 5: Accept connections in order: Video, Audio (optional), Control
        let mut video_stream = if params.video {
            Some(
                accept_connection(&listener, "video")
                    .context("Failed to accept video connection")?,
            )
        } else {
            None
        };

        let mut audio_stream = if params.audio {
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

        // Step 6: Read device metadata from video stream (if enabled)
        let device_name = if params.send_device_meta {
            if let Some(ref mut stream) = video_stream {
                match super::server::read_device_meta(stream) {
                    Ok(name) => {
                        debug!("Device name: {}", name);
                        Some(name)
                    }
                    Err(e) => {
                        debug!("Failed to read device metadata: {}", e);
                        None
                    }
                }
            } else {
                None
            }
        } else {
            None
        };

        // Step 7: Read codec metadata from video stream (if enabled)
        // Codec meta: 4 bytes codec_id + 4 bytes width + 4 bytes height
        let video_resolution = if params.send_codec_meta
            && let Some(ref mut stream) = video_stream
        {
            let mut codec_meta = [0u8; 12];
            if let Err(e) = stream.read_exact(&mut codec_meta) {
                debug!("Failed to read video codec metadata: {}", e);
                None
            } else {
                let codec_id = u32::from_be_bytes(codec_meta[0..4].try_into().unwrap());
                let width = u32::from_be_bytes(codec_meta[4..8].try_into().unwrap());
                let height = u32::from_be_bytes(codec_meta[8..12].try_into().unwrap());
                debug!(
                    "Video codec meta: id=0x{:08x}, {}x{}",
                    codec_id, width, height
                );
                Some((width, height))
            }
        } else {
            None
        };

        // Step 8: Read codec metadata from audio stream (if enabled)
        // Audio codec meta: 4 bytes codec_id only (no width/height for audio)
        if params.send_codec_meta
            && let Some(ref mut stream) = audio_stream
        {
            let mut codec_id_bytes = [0u8; 4];
            if let Err(e) = stream.read_exact(&mut codec_id_bytes) {
                debug!("Failed to read audio codec metadata: {}", e);
            } else {
                let codec_id = u32::from_be_bytes(codec_id_bytes);
                debug!("Audio codec meta: id=0x{:08x}", codec_id);

                // Check for special codec_id values (as per demuxer.c):
                // 0 = stream explicitly disabled by device
                // 1 = stream configuration error
                if codec_id == 0 {
                    info!("Audio stream explicitly disabled by device");
                } else if codec_id == 1 {
                    info!("Audio stream configuration error on device");
                }
            }
        }

        Ok(Self {
            scid,
            device_name,
            video_resolution,
            video_stream,
            audio_stream,
            audio_disabled_reason,
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

    /// Read exact number of bytes from video stream (blocking until complete)
    pub fn read_video_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        if let Some(ref mut stream) = self.video_stream {
            stream
                .read_exact(buf)
                .context("Failed to read exact bytes from video stream")
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

    /// Read and parse a video packet
    pub fn read_video_packet(&mut self) -> Result<super::protocol::video::VideoPacket> {
        use super::protocol::video::VideoPacket;
        VideoPacket::read_from(
            self.video_stream
                .as_mut()
                .context("Video stream not available")?,
        )
    }

    /// Read and parse an audio packet
    pub fn read_audio_packet(&mut self) -> Result<super::protocol::audio::AudioPacket> {
        use super::protocol::audio::AudioPacket;

        if let Some(ref mut stream) = self.audio_stream {
            // Read 12-byte header first
            let mut header = [0u8; 12];
            stream
                .read_exact(&mut header)
                .context("Failed to read audio packet header")?;

            // Parse packet size from header
            let packet_size = u32::from_be_bytes([header[8], header[9], header[10], header[11]]);

            // Read full packet (header + payload)
            let total_size = 12 + packet_size as usize;
            let mut data = vec![0u8; total_size];
            data[..12].copy_from_slice(&header);
            stream
                .read_exact(&mut data[12..])
                .context("Failed to read audio packet payload")?;

            AudioPacket::from_bytes(&data)
        } else {
            anyhow::bail!("Audio stream not available (disabled for Android < 11 or not requested)")
        }
    }

    /// Gracefully shutdown connection
    pub fn shutdown(&mut self) -> Result<()> {
        debug!("Shutting down connection");

        // Step 1: Remove reverse tunnel first (so server can't reconnect)
        remove_reverse_tunnel(&self.serial, &get_socket_name(self.scid)).ok();

        // Step 2: Close sockets (triggers server to exit)
        self.video_stream.take();
        self.audio_stream.take();
        self.control_stream.take();

        // Step 3: Wait for server process to exit gracefully (with timeout)
        if let Some(mut process) = self.server_process.take() {
            // Try to wait with timeout (non-blocking)
            for _ in 0..5 {
                // 5 * 200ms = 1 second max
                if process.try_wait().ok().flatten().is_some() {
                    debug!("Server process exited gracefully");
                    return Ok(());
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
            }

            // Force kill if still running
            debug!("Force killing server process (timeout)");
            process.kill().ok();

            // Final wait with very short timeout
            std::thread::sleep(std::time::Duration::from_millis(100));
            let _ = process.try_wait();
        }

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

    let status = std::process::Command::new("adb")
        .args([
            "-s",
            serial,
            "reverse",
            "--remove",
            &format!("localabstract:{}", socket_name),
        ])
        .output(); // Use output() instead of status() to capture stderr

    match status {
        Ok(output) if output.status.success() => {
            debug!("Reverse tunnel removed successfully");
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Ignore "not found" errors (tunnel already removed)
            if !stderr.contains("not found") && !stderr.is_empty() {
                debug!("Failed to remove tunnel: {}", stderr.trim());
            }
        }
        Err(e) => {
            debug!("Error removing tunnel: {}", e);
        }
    }

    Ok(())
}

/// Accept a single connection with optional dummy byte handling
fn accept_connection(listener: &TcpListener, channel: &str) -> Result<TcpStream> {
    debug!("Waiting for {} connection...", channel);

    // Set accept timeout
    listener
        .set_nonblocking(false)
        .context("Failed to set blocking mode")?;

    // Accept connection
    let (stream, addr) = listener
        .accept()
        .context(format!("Failed to accept {} connection", channel))?;

    debug!("{} connection accepted from {}", channel, addr);

    // 🚀 LOW LATENCY: Enable TCP_NODELAY to disable Nagle's algorithm
    // This reduces latency by 5-10ms by sending packets immediately
    stream
        .set_nodelay(true)
        .context("Failed to set TCP_NODELAY")?;

    debug!("{} connection: TCP_NODELAY enabled", channel);

    // 🛡️ CRITICAL: Set read timeout to detect USB disconnection
    // Without timeout, read() blocks forever when USB is unplugged
    let timeout = match channel {
        "control" => std::time::Duration::from_secs(2), // Faster detection for control
        _ => std::time::Duration::from_secs(5),         // Video/Audio can tolerate more delay
    };
    stream
        .set_read_timeout(Some(timeout))
        .with_context(|| format!("Failed to set {} stream read timeout", channel))?;

    debug!("{} connection: read timeout set to {:?}", channel, timeout);

    // NOTE: In adb reverse mode (default), the server does NOT send dummy byte
    // Dummy byte is only sent in tunnel_forward mode (when server listens)
    // The first byte is actual data, so we don't read it here

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
