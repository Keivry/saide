use {
    anyhow::{Context, Result},
    std::{
        mem,
        sync::Arc,
        thread,
        time::{Duration, Instant},
    },
    v4l::{
        Device,
        FourCC,
        buffer::Type,
        io::traits::{CaptureStream, Stream},
        prelude::*,
        video::Capture,
    },
};

/// YU12 frame data (Planar YUV 4:2:0)
#[derive(Clone)]
pub struct Yu12Frame {
    pub width: u32,
    pub height: u32,
    pub data: Arc<[u8]>,

    /// Frame sequence number
    pub seq: u32,
    /// Capture timestamp
    pub timestamp: Instant,
}

pub struct V4l2Capture {
    // Keep device alive for stream lifetime
    _device: Device,
    stream: MmapStream<'static>,
    width: u32,
    height: u32,
    seq: u32,
}

impl V4l2Capture {
    /// Open video device and configure for YU12 capture
    pub fn new(dev: &str, timeout: Duration) -> Result<Self> {
        let device = Device::with_path(dev).context("Failed to open video device")?;

        let start = Instant::now();
        let fmt = loop {
            if start.elapsed() > timeout {
                anyhow::bail!("Timeout waiting for video device to be ready");
            }

            // Check format and dimensions
            // (We expect YU12 format from scrcpy)
            if let Ok(fmt) = device.format() {
                if fmt.fourcc != FourCC::new(b"YU12") {
                    anyhow::bail!("Device is not in YU12 format");
                }
                break fmt;
            }
            thread::sleep(Duration::from_millis(100));
        };

        // Create memory-mapped stream with 2 buffers
        let stream = MmapStream::with_buffers(&device, Type::VideoCapture, 2)
            .context("Failed to create stream")?;

        // SAFETY: We ensure device outlives stream by storing both in struct
        let stream = unsafe { mem::transmute::<MmapStream<'_>, MmapStream<'static>>(stream) };

        Ok(Self {
            _device: device,
            stream,
            width: fmt.width,
            height: fmt.height,
            seq: 0,
        })
    }

    /// Capture a single frame (blocking)
    pub fn capture_frame(&mut self) -> Result<Yu12Frame> {
        let (buffer, _meta) = self.stream.next().context("Failed to capture frame")?;

        let timestamp = Instant::now();
        self.seq += 1;

        // Create Arc directly from slice (single allocation)
        let data: Arc<[u8]> = Arc::from(buffer);

        Ok(Yu12Frame {
            width: self.width,
            height: self.height,
            data,
            seq: self.seq,
            timestamp,
        })
    }

    pub fn stop_streaming(&mut self) -> Result<()> {
        self.stream.stop().context("Failed to stop streaming")
    }

    /// Get current frame dimensions
    pub fn dimensions(&self) -> (u32, u32) { (self.width, self.height) }
}

impl Drop for V4l2Capture {
    fn drop(&mut self) {
        // stream is dropped first, then device
    }
}
