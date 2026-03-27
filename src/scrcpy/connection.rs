// SPDX-License-Identifier: MIT OR Apache-2.0

use {
    crate::{
        error::{IoError, Result, SAideError},
        scrcpy::server::{
            ServerParams,
            get_socket_name,
            push_server,
            read_device_meta,
            start_server,
        },
    },
    adbshell::AdbShell,
    scrcpy_protocol::{
        GRACEFUL_WAIT_MS,
        MAX_PACKET_SIZE,
        protocol::{audio::AudioPacket, video::VideoPacket},
    },
    std::{
        fmt,
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        process::Child,
        sync::Arc,
        thread,
        time::{Duration, Instant},
    },
    tracing::{debug, info},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Channel {
    Video,
    Audio,
    Control,
}

/// Reason why audio was automatically disabled during [`ScrcpyConnection::connect`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioDisabledReason {
    UnsupportedAndroidVersion { api_level: u32 },
}

impl fmt::Display for AudioDisabledReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedAndroidVersion { api_level } => {
                write!(
                    f,
                    "Audio capture requires Android 11+ (API 30+). Device API level: {}.",
                    api_level
                )
            }
        }
    }
}

impl fmt::Display for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Video => write!(f, "video"),
            Self::Audio => write!(f, "audio"),
            Self::Control => write!(f, "control"),
        }
    }
}

/// An active connection to a scrcpy Android server.
///
/// Obtain a value by calling [`ScrcpyConnection::connect`].  The connection
/// owns the server process and all three TCP streams (video, audio, control).
/// Drop it via [`ScrcpyConnection::shutdown`] for a clean teardown; dropping
/// without calling `shutdown` kills the server process but leaves the ADB
/// reverse tunnel in place.
pub struct ScrcpyConnection {
    pub scid: u32,
    pub device_name: Option<String>,
    pub video_resolution: Option<(u32, u32)>,
    pub audio_disabled_reason: Option<AudioDisabledReason>,
    pub local_port: u16,
    /// Codec identifier received from the server during handshake (big-endian u32).
    /// `0x6f707573` = Opus (default). Only valid when audio is enabled and
    /// `send_codec_meta` was true at connect time.
    pub audio_codec_id: u32,
    video_stream: Option<TcpStream>,
    audio_stream: Option<TcpStream>,
    control_stream: Option<TcpStream>,
    server_process: Option<Child>,
    shell: Arc<AdbShell>,
}

impl ScrcpyConnection {
    pub fn connect(
        shell: Arc<AdbShell>,
        server_jar_path: &str,
        bind_address: &str,
        mut params: ServerParams,
    ) -> Result<Self> {
        let serial = shell.serial().to_string();
        let scid = params.scid;
        let socket_name = get_socket_name(scid);
        let mut audio_disabled_reason = None;
        let mut audio_codec_id: u32 = 0x6f70_7573; // Opus default

        if params.audio {
            let android_version = shell.get_android_version()?;
            if android_version < 30 {
                let reason = AudioDisabledReason::UnsupportedAndroidVersion {
                    api_level: android_version,
                };
                tracing::warn!("{} Disabling audio.", reason);
                audio_disabled_reason = Some(reason);
                params.audio = false;
            }
        }

        push_server(&shell, server_jar_path)?;
        let listener = find_available_port(bind_address)?;
        let local_port = listener.local_addr()?.port();
        setup_reverse_tunnel(&shell, &socket_name, local_port)?;
        let mut server_process = start_server(&shell, &params)?;

        let result = Self::accept_streams(
            &listener,
            &mut server_process,
            &serial,
            &socket_name,
            &mut params,
            &mut audio_codec_id,
            &mut audio_disabled_reason,
        );

        let (video_stream, audio_stream, control_stream, device_name, video_resolution) =
            match result {
                Ok(streams) => streams,
                Err(e) => {
                    let _ = server_process.kill();
                    let _ = remove_reverse_tunnel(&shell, &socket_name);
                    return Err(e);
                }
            };

        info!("All sockets connected successfully");

        Ok(Self {
            scid,
            device_name,
            video_resolution,
            audio_disabled_reason,
            local_port,
            audio_codec_id,
            video_stream,
            audio_stream,
            control_stream,
            server_process: Some(server_process),
            shell,
        })
    }

    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    fn accept_streams(
        listener: &TcpListener,
        _server_process: &mut std::process::Child,
        _serial: &str,
        _socket_name: &str,
        params: &mut ServerParams,
        audio_codec_id: &mut u32,
        audio_disabled_reason: &mut Option<AudioDisabledReason>,
    ) -> Result<(
        Option<TcpStream>,
        Option<TcpStream>,
        Option<TcpStream>,
        Option<String>,
        Option<(u32, u32)>,
    )> {
        let mut video_stream = if params.video {
            Some(accept_connection(listener, &Channel::Video)?)
        } else {
            None
        };
        let mut audio_stream = if params.audio {
            Some(accept_connection(listener, &Channel::Audio)?)
        } else {
            None
        };
        let control_stream = if params.control {
            Some(accept_connection(listener, &Channel::Control)?)
        } else {
            None
        };

        let device_name = if params.send_device_meta {
            if let Some(ref mut stream) = video_stream {
                Some(read_device_meta(stream).map_err(|e| {
                    SAideError::ProtocolError(format!(
                        "Failed to read device name from handshake: {}",
                        e
                    ))
                })?)
            } else {
                None
            }
        } else {
            None
        };

        let video_resolution = if params.send_codec_meta
            && let Some(ref mut stream) = video_stream
        {
            let mut codec_meta = [0u8; 12];
            stream.read_exact(&mut codec_meta).map_err(|e| {
                SAideError::ProtocolError(format!(
                    "Failed to read video codec metadata from handshake: {}",
                    e
                ))
            })?;
            let width = u32::from_be_bytes(codec_meta[4..8].try_into().map_err(|_| {
                SAideError::Other("Invalid video codec metadata width bytes".to_string())
            })?);
            let height = u32::from_be_bytes(codec_meta[8..12].try_into().map_err(|_| {
                SAideError::Other("Invalid video codec metadata height bytes".to_string())
            })?);
            Some((width, height))
        } else {
            None
        };

        if params.send_codec_meta
            && let Some(ref mut stream) = audio_stream
        {
            let mut codec_id_bytes = [0u8; 4];
            stream.read_exact(&mut codec_id_bytes).map_err(|e| {
                SAideError::ProtocolError(format!(
                    "Failed to read audio codec id from handshake: {}",
                    e
                ))
            })?;
            *audio_codec_id = u32::from_be_bytes(codec_id_bytes);
        }

        let _ = audio_disabled_reason;
        Ok((
            video_stream,
            audio_stream,
            control_stream,
            device_name,
            video_resolution,
        ))
    }

    pub fn send_control(&mut self, data: &[u8]) -> Result<()> {
        if let Some(ref mut stream) = self.control_stream {
            stream.write_all(data)?;
            stream.flush()?;
            Ok(())
        } else {
            Err(SAideError::Other(
                "Unexpected null control stream".to_string(),
            ))
        }
    }

    pub fn read_video(&mut self, buf: &mut [u8]) -> Result<usize> {
        if let Some(ref mut stream) = self.video_stream {
            Ok(stream.read(buf)?)
        } else {
            Err(SAideError::Other(
                "Unexpected null video stream".to_string(),
            ))
        }
    }

    pub fn read_video_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        if let Some(ref mut stream) = self.video_stream {
            Ok(stream.read_exact(buf)?)
        } else {
            Err(SAideError::Other(
                "Unexpected null video stream".to_string(),
            ))
        }
    }

    pub fn is_server_alive(&mut self) -> bool {
        if let Some(ref mut process) = self.server_process {
            process.try_wait().ok().flatten().is_none()
        } else {
            false
        }
    }

    pub fn read_video_packet(&mut self) -> Result<VideoPacket> {
        VideoPacket::read_from(
            self.video_stream
                .as_mut()
                .ok_or_else(|| SAideError::Other("Unexpected null video stream".to_string()))?,
        )
        .map_err(Into::into)
    }

    pub fn set_video_read_timeout(&self, timeout: Option<Duration>) -> Result<()> {
        if let Some(ref stream) = self.video_stream {
            stream.set_read_timeout(timeout)?;
            Ok(())
        } else {
            Err(SAideError::Other(
                "Unexpected null video stream".to_string(),
            ))
        }
    }

    pub fn read_audio_packet(&mut self) -> Result<AudioPacket> {
        if let Some(ref mut stream) = self.audio_stream {
            let mut header = [0u8; 12];
            stream.read_exact(&mut header)?;
            let packet_size = u32::from_be_bytes([header[8], header[9], header[10], header[11]]);

            if packet_size as usize > MAX_PACKET_SIZE {
                return Err(SAideError::ProtocolError(format!(
                    "Audio packet size {} exceeds maximum allowed {} (10MB)",
                    packet_size, MAX_PACKET_SIZE
                )));
            }

            let total_size = 12 + packet_size as usize;
            let mut data = vec![0u8; total_size];
            data[..12].copy_from_slice(&header);
            stream.read_exact(&mut data[12..])?;
            let mut packet = AudioPacket::from_bytes(&data).map_err(SAideError::from)?;
            packet.codec_id = self.audio_codec_id;
            Ok(packet)
        } else {
            Err(SAideError::Other(
                "Audio stream not available (disabled for Android < 11 or not requested)"
                    .to_string(),
            ))
        }
    }

    pub fn take_video_stream(&mut self) -> Option<TcpStream> { self.video_stream.take() }

    pub fn take_audio_stream(&mut self) -> Option<TcpStream> { self.audio_stream.take() }

    pub fn take_control_stream(&mut self) -> Option<TcpStream> { self.control_stream.take() }

    pub fn set_control_stream(&mut self, stream: TcpStream) { self.control_stream = Some(stream); }

    pub fn shutdown(&mut self) -> Result<()> {
        self.video_stream.take();
        self.audio_stream.take();
        self.control_stream.take();

        if let Some(mut process) = self.server_process.take() {
            let start = Instant::now();

            while start.elapsed().as_millis() < GRACEFUL_WAIT_MS as u128 {
                if process.try_wait()?.is_some() {
                    let _ = remove_reverse_tunnel(&self.shell, &get_socket_name(self.scid));
                    return Ok(());
                }
                thread::sleep(Duration::from_millis(10));
            }

            let _ = process.kill();
            let _ = process.wait();
            thread::sleep(Duration::from_millis(50));
        }

        let _ = remove_reverse_tunnel(&self.shell, &get_socket_name(self.scid));
        Ok(())
    }
}

impl Drop for ScrcpyConnection {
    fn drop(&mut self) {
        if let Some(mut process) = self.server_process.take() {
            tracing::warn!(
                "ScrcpyConnection dropped without calling shutdown(); \
                 server process killed but ADB reverse tunnel may remain active. \
                 Call shutdown() for clean teardown."
            );
            let _ = process.kill();
            let _ = process.wait();
        }
    }
}

fn find_available_port(bind_address: &str) -> Result<TcpListener> {
    TcpListener::bind(format!("{}:0", bind_address)).map_err(|e| {
        SAideError::IoError(IoError::new_with_message(format!(
            "Failed to bind listener on {}: {}",
            bind_address, e
        )))
    })
}

fn setup_reverse_tunnel(shell: &AdbShell, socket_name: &str, local_port: u16) -> Result<()> {
    shell
        .setup_reverse_tunnel(socket_name, local_port)
        .map_err(Into::into)
}

fn remove_reverse_tunnel(shell: &AdbShell, socket_name: &str) -> Result<()> {
    shell.remove_reverse_tunnel(socket_name).map_err(Into::into)
}

fn accept_connection(listener: &TcpListener, channel: &Channel) -> Result<TcpStream> {
    let timeout = match channel {
        Channel::Control => Duration::from_secs(2),
        _ => Duration::from_secs(5),
    };

    listener.set_nonblocking(true).map_err(|e| {
        SAideError::IoError(
            IoError::new(e).with_message("Failed to set listener to nonblocking mode"),
        )
    })?;

    let deadline = Instant::now() + timeout;
    let (stream, _) = loop {
        match listener.accept() {
            Ok(connection) => break connection,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err(SAideError::IoError(IoError::new_with_message(format!(
                        "Timed out waiting for {} connection after {:?}",
                        channel, timeout
                    ))));
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                return Err(SAideError::IoError(
                    IoError::new(e)
                        .with_message(format!("Failed to accept {} connection", channel)),
                ));
            }
        }
    };

    stream.set_nonblocking(false).map_err(|e| {
        SAideError::IoError(
            IoError::new(e)
                .with_message(format!("Failed to set {} stream to blocking mode", channel)),
        )
    })?;
    stream.set_nodelay(true).map_err(|e| {
        SAideError::IoError(IoError::new(e).with_message(format!(
            "Failed to set TCP_NODELAY for {} connection",
            channel
        )))
    })?;

    #[cfg(target_os = "linux")]
    {
        use std::os::unix::io::AsRawFd;

        let fd = stream.as_raw_fd();
        let quickack: libc::c_int = 1;
        let _ = unsafe {
            libc::setsockopt(
                fd,
                libc::IPPROTO_TCP,
                libc::TCP_QUICKACK,
                &quickack as *const _ as *const libc::c_void,
                std::mem::size_of::<libc::c_int>() as libc::socklen_t,
            )
        };
    }

    stream.set_read_timeout(Some(timeout)).map_err(|e| {
        SAideError::IoError(IoError::new(e).with_message(format!(
            "Failed to set read timeout for {} connection",
            channel
        )))
    })?;

    debug!("{} channel ready", channel);
    Ok(stream)
}
