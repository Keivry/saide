//! Scrcpy Connection Management
//!
//! Handles socket connections and ADB tunneling.
//! Reference: scrcpy/app/src/adb/adb_tunnel.c, scrcpy/app/src/server.c

use {
    super::{
        protocol::{audio::AudioPacket, video::VideoPacket},
        server::{ServerParams, get_socket_name, push_server, read_device_meta, start_server},
    },
    crate::{
        constant::{DEFAULT_PORT_RANGE, GRACEFUL_WAIT_MS},
        controller::AdbShell,
        error::{Result, SAideError},
    },
    std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        process::Child,
        thread,
        time::{Duration, Instant},
    },
    tracing::{debug, info},
};

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
            let android_version = AdbShell::get_android_version(serial)?;

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
        push_server(serial, server_jar_path)?;

        // Step 2: Find available local port and start listening FIRST
        // (as per scrcpy: client must listen before server starts)
        let local_port = find_available_port(DEFAULT_PORT_RANGE.0, DEFAULT_PORT_RANGE.1)?;

        let listener = TcpListener::bind(format!("127.0.0.1:{}", local_port)).map_err(|e| {
            SAideError::Io(format!(
                "Failed to bind local listener on port {}: {}",
                local_port, e
            ))
        })?;
        listener.set_nonblocking(false).map_err(|e| {
            SAideError::Io(format!("Failed to set listener to blocking mode: {}", e))
        })?;

        debug!("Listening on 127.0.0.1:{}", local_port);

        // Step 3: Setup ADB reverse tunnel (after listener is ready)
        setup_reverse_tunnel(serial, &socket_name, local_port)?;

        // Step 4: Start server process (it will connect to our listener via tunnel)
        let server_process = start_server(serial, &params)?;

        debug!("Server started, waiting for connections...");

        // Step 5: Accept connections in order: Video, Audio (optional), Control
        let mut video_stream = if params.video {
            Some(accept_connection(&listener, "video")?)
        } else {
            None
        };

        let mut audio_stream = if params.audio {
            Some(accept_connection(&listener, "audio")?)
        } else {
            None
        };

        let control_stream = if params.control {
            Some(accept_connection(&listener, "control")?)
        } else {
            None
        };

        info!("All sockets connected successfully");

        // Step 6: Read device metadata from video stream (if enabled)
        let device_name = if params.send_device_meta {
            if let Some(ref mut stream) = video_stream {
                match read_device_meta(stream) {
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
            stream.write_all(data).map_err(|e| {
                SAideError::Channel(format!("Failed to write to control stream: {}", e))
            })?;
            stream.flush().map_err(|e| {
                SAideError::Channel(format!("Failed to flush control stream: {}", e))
            })?;
            Ok(())
        } else {
            Err(SAideError::Other(
                "Control stream not available".to_string(),
            ))
        }
    }

    /// Read video packet (blocking)
    pub fn read_video(&mut self, buf: &mut [u8]) -> Result<usize> {
        if let Some(ref mut stream) = self.video_stream {
            stream.read(buf).map_err(|e| {
                SAideError::Channel(format!("Failed to read from video stream: {}", e))
            })
        } else {
            Err(SAideError::Other("Video stream not available".to_string()))
        }
    }

    /// Read exact number of bytes from video stream (blocking until complete)
    pub fn read_video_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        if let Some(ref mut stream) = self.video_stream {
            stream.read_exact(buf).map_err(|e| {
                SAideError::Channel(format!("Failed to read exact from video stream: {}", e))
            })
        } else {
            Err(SAideError::Other("Video stream not available".to_string()))
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
    pub fn read_video_packet(&mut self) -> Result<VideoPacket> {
        VideoPacket::read_from(
            self.video_stream
                .as_mut()
                .ok_or_else(|| SAideError::Other("Video stream not available".to_string()))?,
        )
    }

    /// Read and parse an audio packet
    pub fn read_audio_packet(&mut self) -> Result<AudioPacket> {
        if let Some(ref mut stream) = self.audio_stream {
            // Read 12-byte header first
            let mut header = [0u8; 12];
            stream.read_exact(&mut header).map_err(|_| {
                SAideError::Channel("Failed to read audio packet header".to_string())
            })?;

            // Parse packet size from header
            let packet_size = u32::from_be_bytes([header[8], header[9], header[10], header[11]]);

            // Read full packet (header + payload)
            let total_size = 12 + packet_size as usize;
            let mut data = vec![0u8; total_size];
            data[..12].copy_from_slice(&header);
            stream.read_exact(&mut data[12..]).map_err(|_| {
                SAideError::Channel("Failed to read audio packet payload".to_string())
            })?;

            return AudioPacket::from_bytes(&data);
        }

        Err(SAideError::Other(
            "Audio stream not available (disabled for Android < 11 or not requested)".to_string(),
        ))
    }

    /// Gracefully shutdown connection
    pub fn shutdown(&mut self) -> Result<()> {
        debug!("Shutting down connection");

        // Step 1: Close sockets FIRST (triggers broken pipe)
        self.video_stream.take();
        self.audio_stream.take();
        self.control_stream.take();
        debug!("Sockets closed");

        // Step 2: Kill server process immediately (MUST be in main thread)
        if let Some(mut process) = self.server_process.take() {
            // Try graceful exit first (wait 100ms)
            let start = Instant::now();

            while start.elapsed().as_millis() < GRACEFUL_WAIT_MS as u128 {
                if let Ok(Some(status)) = process.try_wait() {
                    debug!("Server process exited gracefully with status: {:?}", status);
                    remove_reverse_tunnel(&self.serial, &get_socket_name(self.scid)).ok();
                    info!("Connection closed (graceful)");
                    return Ok(());
                }
                thread::sleep(Duration::from_millis(10));
            }

            // Force kill if still running
            debug!(
                "Server process still running after {}ms, force killing",
                GRACEFUL_WAIT_MS
            );
            match process.kill() {
                Ok(_) => {
                    debug!("Server process killed");
                    // Wait for reaping
                    thread::sleep(Duration::from_millis(50));
                    match process.try_wait() {
                        Ok(Some(status)) => debug!("Process reaped: {:?}", status),
                        Ok(None) => debug!("Process not yet reaped"),
                        Err(e) => debug!("Failed to reap: {}", e),
                    }
                }
                Err(e) => {
                    debug!("Failed to kill server process: {}", e);
                }
            }
        }

        // Step 3: Remove reverse tunnel
        remove_reverse_tunnel(&self.serial, &get_socket_name(self.scid)).ok();

        info!("Connection closed (cleanup detached)");
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
    Err(SAideError::Io(format!(
        "No available port found in range {}-{}",
        start, end
    )))
}

/// Setup ADB reverse tunnel
fn setup_reverse_tunnel(serial: &str, socket_name: &str, local_port: u16) -> Result<()> {
    debug!(
        "Setting up reverse tunnel: {} -> tcp:{}",
        socket_name, local_port
    );

    AdbShell::setup_reverse_tunnel(serial, socket_name, local_port)?;

    info!("Reverse tunnel established");
    Ok(())
}

/// Remove ADB reverse tunnel
fn remove_reverse_tunnel(serial: &str, socket_name: &str) -> Result<()> {
    debug!("Removing reverse tunnel: {}", socket_name);

    AdbShell::remove_reverse_tunnel(serial, socket_name)?;

    info!("Reverse tunnel removed");
    Ok(())
}

/// Accept a single connection with optional dummy byte handling
fn accept_connection(listener: &TcpListener, channel: &str) -> Result<TcpStream> {
    debug!("Waiting for {} connection...", channel);

    // Set accept timeout
    listener
        .set_nonblocking(false)
        .map_err(|e| SAideError::Io(format!("Failed to set listener to blocking mode: {}", e)))?;

    // Accept connection
    let (stream, addr) = listener
        .accept()
        .map_err(|e| SAideError::Io(format!("Failed to accept {} connection: {}", channel, e)))?;

    debug!("{} connection accepted from {}", channel, addr);

    // 🚀 LOW LATENCY: Enable TCP_NODELAY to disable Nagle's algorithm
    // This reduces latency by 5-10ms by sending packets immediately
    stream.set_nodelay(true).map_err(|_| {
        SAideError::Io(format!(
            "Failed to set TCP_NODELAY for {} connection",
            channel
        ))
    })?;

    debug!("{} connection: TCP_NODELAY enabled", channel);

    // 🛡️ CRITICAL: Set read timeout to detect USB disconnection
    // Without timeout, read() blocks forever when USB is unplugged
    let timeout = match channel {
        "control" => Duration::from_secs(2), // Faster detection for control
        _ => Duration::from_secs(5),         // Video/Audio can tolerate more delay
    };
    stream.set_read_timeout(Some(timeout)).map_err(|_| {
        SAideError::Io(format!(
            "Failed to set read timeout for {} connection",
            channel
        ))
    })?;

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
        assert!((27183..=27199).contains(&port));
    }

    #[test]
    fn test_socket_name() {
        assert_eq!(get_socket_name(0x12345678), "scrcpy_12345678");
    }
}
