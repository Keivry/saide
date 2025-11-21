use {
    anyhow::{Context, Result},
    std::sync::Arc,
    v4l::{buffer::Type, io::traits::CaptureStream, prelude::*, video::Capture, Device, FourCC},
};

/// YU12 frame data (Planar YUV 4:2:0)
#[derive(Clone)]
pub struct Yu12Frame {
    pub width: u32,
    pub height: u32,
    pub data: Arc<Vec<u8>>,
}

pub struct V4l2Capture {
    // Keep device alive for stream lifetime
    _device: Device,
    stream: MmapStream<'static>,
    width: u32,
    height: u32,
}

impl V4l2Capture {
    /// Open video device and configure for YU12 capture
    pub fn new(device_path: &str, width: u32, height: u32) -> Result<Self> {
        let device = Device::with_path(device_path).context("Failed to open video device")?;

        // Set format to YU12 (Planar YUV 4:2:0)
        let mut fmt = device.format()?;
        fmt.width = width;
        fmt.height = height;
        fmt.fourcc = FourCC::new(b"YU12");
        device.set_format(&fmt).context("Failed to set format")?;

        // Verify format was accepted
        let fmt = device.format()?;
        if fmt.fourcc != FourCC::new(b"YU12") {
            anyhow::bail!("Device does not support YU12 format");
        }
        if fmt.width != width || fmt.height != height {
            log::warn!(
                "Requested {}x{}, got {}x{}",
                width,
                height,
                fmt.width,
                fmt.height
            );
        }

        // Create memory-mapped stream with 4 buffers
        let stream = MmapStream::with_buffers(&device, Type::VideoCapture, 4)
            .context("Failed to create stream")?;

        // SAFETY: We ensure device outlives stream by storing both in struct
        let stream = unsafe { std::mem::transmute::<MmapStream<'_>, MmapStream<'static>>(stream) };

        Ok(Self {
            _device: device,
            stream,
            width: fmt.width,
            height: fmt.height,
        })
    }

    /// Capture a single frame (blocking)
    pub fn capture_frame(&mut self) -> Result<Yu12Frame> {
        let (buffer, _meta) = self.stream.next().context("Failed to capture frame")?;

        // Clone data to owned Vec
        let data = Arc::new(buffer.to_vec());

        Ok(Yu12Frame {
            width: self.width,
            height: self.height,
            data,
        })
    }

    /// Get current frame dimensions
    pub fn dimensions(&self) -> (u32, u32) { (self.width, self.height) }
}

impl Drop for V4l2Capture {
    fn drop(&mut self) {
        // stream is dropped first, then device
    }
}
