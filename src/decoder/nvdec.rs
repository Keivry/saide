//! NVIDIA NVDEC hardware-accelerated H.264 decoder

use {
    super::{DecodedFrame, VideoDecoder},
    anyhow::{Context as AnyhowContext, Result, bail},
    ffmpeg::{
        codec,
        format::Pixel,
        software::scaling::context::Context as ScalerContext,
        util::frame::video::Video as VideoFrame,
    },
    ffmpeg_next as ffmpeg,
    std::ptr,
    tracing::{debug, info, warn},
};

pub struct NvdecDecoder {
    decoder: ffmpeg::decoder::Video,
    #[allow(dead_code)]
    scaler: Option<ScalerContext>,
    hw_device_ctx: *mut ffmpeg::sys::AVBufferRef,
    width: u32,
    height: u32,
    #[allow(dead_code)]
    output_format: Pixel,
    #[allow(dead_code)]
    last_decoded_dimensions: Option<(u32, u32)>,
    /// Counter for consecutive empty frame returns (indicates decode failure)
    consecutive_empty_frames: u32,
}

impl NvdecDecoder {
    pub fn new(width: u32, height: u32) -> Result<Self> {
        // Initialize FFmpeg
        ffmpeg::init().context("Failed to initialize FFmpeg")?;

        // Create CUDA device context
        let mut hw_device_ctx: *mut ffmpeg::sys::AVBufferRef = ptr::null_mut();

        unsafe {
            let ret = ffmpeg::sys::av_hwdevice_ctx_create(
                &mut hw_device_ctx,
                ffmpeg::sys::AVHWDeviceType::AV_HWDEVICE_TYPE_CUDA,
                ptr::null(), // Use default device (GPU 0)
                ptr::null_mut(),
                0,
            );
            if ret < 0 {
                bail!("Failed to create CUDA device context: {}", ret);
            }
        }

        info!("CUDA device context created");

        // Find h264_cuvid decoder
        let codec = ffmpeg::decoder::find_by_name("h264_cuvid")
            .context("h264_cuvid decoder not found (FFmpeg not compiled with NVDEC support)")?;

        info!("Found H.264 CUVID decoder: {}", codec.name());

        // Create decoder context
        let mut context = codec::context::Context::new_with_codec(codec);

        unsafe {
            let ctx_ptr = context.as_mut_ptr();

            // Set hardware device context
            (*ctx_ptr).hw_device_ctx = ffmpeg::sys::av_buffer_ref(hw_device_ctx);

            // Set format callback to select CUDA
            (*ctx_ptr).get_format = Some(get_cuda_format);

            // DON'T set width/height here - let CUVID auto-detect from stream
            // Setting them causes "AVHWFramesContext is already initialized" error
            // when using ConstrainedBaseline profile with dynamic resolution

            // 🚀 LOW LATENCY OPTIMIZATIONS
            // 1. Low delay flag - disables frame reordering
            (*ctx_ptr).flags |= ffmpeg::sys::AV_CODEC_FLAG_LOW_DELAY as i32;

            // 2. Single thread - NVDEC decodes in hardware
            (*ctx_ptr).thread_count = 1;
        }

        let decoder = context
            .decoder()
            .video()
            .context("Failed to open h264_cuvid decoder")?;

        debug!(
            "NVDEC H.264 decoder initialized (will auto-detect {}x{} from stream)",
            width, height
        );

        Ok(Self {
            decoder,
            scaler: None,
            hw_device_ctx,
            width,
            height,
            output_format: Pixel::NV12, // NVDEC outputs NV12
            last_decoded_dimensions: None,
            consecutive_empty_frames: 0,
        })
    }

    fn send_packet(&mut self, data: &[u8], pts: i64) -> Result<()> {
        let mut packet = ffmpeg::Packet::new(data.len());
        packet.data_mut().unwrap().copy_from_slice(data);
        packet.set_pts(Some(pts));
        packet.set_dts(Some(pts));

        self.decoder
            .send_packet(&packet)
            .context("Failed to send packet to decoder")?;

        Ok(())
    }

    fn receive_frames(&mut self) -> Result<Vec<DecodedFrame>> {
        let mut frames = Vec::new();

        loop {
            let mut hw_frame = VideoFrame::empty();
            match self.decoder.receive_frame(&mut hw_frame) {
                Ok(_) => {
                    // Transfer from GPU to CPU (CUDA -> System memory)
                    let mut sw_frame = VideoFrame::empty();
                    let hw_pts = hw_frame.timestamp(); // Capture PTS before transfer

                    unsafe {
                        let ret = ffmpeg::sys::av_hwframe_transfer_data(
                            sw_frame.as_mut_ptr(),
                            hw_frame.as_ptr(),
                            0,
                        );
                        if ret < 0 {
                            warn!("Failed to transfer frame from GPU: {}", ret);
                            continue;
                        }
                    }

                    let width = sw_frame.width();
                    let height = sw_frame.height();
                    let format = sw_frame.format();

                    debug!(
                        "Decoded frame (NVDEC): {}x{} {:?} PTS={:?}",
                        width,
                        height,
                        format,
                        sw_frame.timestamp()
                    );

                    // Extract NV12 data
                    let y_plane = sw_frame.data(0);
                    let uv_plane = sw_frame.data(1);
                    let y_linesize = sw_frame.stride(0);
                    let uv_linesize = sw_frame.stride(1);

                    info!(
                        "NV12 frame layout: {}x{}, Y linesize={} UV linesize={}",
                        width, height, y_linesize, uv_linesize
                    );

                    // Copy NV12 data (Y + UV interleaved)
                    let mut data = Vec::with_capacity(y_linesize * height as usize * 3 / 2);

                    // Copy Y plane
                    for row in 0..height as usize {
                        let offset = row * y_linesize;
                        data.extend_from_slice(&y_plane[offset..offset + width as usize]);
                    }

                    // Copy UV plane (interleaved)
                    for row in 0..(height as usize / 2) {
                        let offset = row * uv_linesize;
                        data.extend_from_slice(&uv_plane[offset..offset + width as usize]);
                    }

                    frames.push(DecodedFrame {
                        data,
                        width,
                        height,
                        format: Pixel::NV12,
                        pts: hw_pts.unwrap_or(0), // Use PTS from hw_frame
                    });
                }
                Err(ffmpeg::Error::Eof) => break,
                Err(ffmpeg::Error::Other { errno: 11 }) => break, // EAGAIN
                Err(e) => {
                    bail!("Failed to receive frame: {:?}", e);
                }
            }
        }

        Ok(frames)
    }
}

impl VideoDecoder for NvdecDecoder {
    fn decode(&mut self, packet_data: &[u8], pts: i64) -> Result<Option<DecodedFrame>> {
        if packet_data.is_empty() {
            return Ok(None);
        }

        self.send_packet(packet_data, pts)?;
        let frames = self.receive_frames()?;

        // Check for consecutive empty frames (indicates decode failure)
        if frames.is_empty() {
            self.consecutive_empty_frames += 1;
            
            // After 3 consecutive empty frames, assume resolution changed
            if self.consecutive_empty_frames >= 3 {
                bail!("NVDEC decoder stuck: {} consecutive empty frames (likely resolution change)", 
                      self.consecutive_empty_frames);
            }
            
            return Ok(None);
        }

        // Reset counter on successful decode
        self.consecutive_empty_frames = 0;

        // Check if decoder dimensions changed after receiving frames
        let decoder_width = self.decoder.width();
        let decoder_height = self.decoder.height();

        if decoder_width != self.width || decoder_height != self.height {
            info!(
                "Decoder dimensions updated: {}x{} -> {}x{}",
                self.width, self.height, decoder_width, decoder_height
            );
            self.width = decoder_width;
            self.height = decoder_height;
        }

        Ok(frames.into_iter().next())
    }

    fn flush(&mut self) -> Result<Vec<DecodedFrame>> {
        debug!("Flushing decoder");
        self.decoder.send_eof().context("Failed to send EOF")?;
        self.receive_frames()
    }
}

impl NvdecDecoder {
    /// Get current decoder width (may be updated after processing SPS)
    pub fn width(&self) -> u32 { self.width }

    /// Get current decoder height (may be updated after processing SPS)
    pub fn height(&self) -> u32 { self.height }
}

impl Drop for NvdecDecoder {
    fn drop(&mut self) {
        unsafe {
            if !self.hw_device_ctx.is_null() {
                ffmpeg::sys::av_buffer_unref(&mut self.hw_device_ctx);
            }
        }
    }
}

// Callback for FFmpeg to select CUDA format
unsafe extern "C" fn get_cuda_format(
    _ctx: *mut ffmpeg::sys::AVCodecContext,
    pix_fmts: *const ffmpeg::sys::AVPixelFormat,
) -> ffmpeg::sys::AVPixelFormat {
    unsafe {
        let mut p = pix_fmts;
        while *p != ffmpeg::sys::AVPixelFormat::AV_PIX_FMT_NONE {
            if *p == ffmpeg::sys::AVPixelFormat::AV_PIX_FMT_CUDA {
                return *p;
            }
            p = p.offset(1);
        }
        ffmpeg::sys::AVPixelFormat::AV_PIX_FMT_NONE
    }
}
