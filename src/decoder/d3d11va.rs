//! D3D11VA hardware-accelerated H.264 decoder for Windows
//!
//! Uses FFmpeg's D3D11VA hardware acceleration via DirectX 11.
//! Supports Intel, AMD, and NVIDIA GPUs on Windows 8+.

use {
    super::{
        error::{Result, VideoError},
        DecodedFrame,
        VideoDecoder,
    },
    ffmpeg::{
        codec,
        format::Pixel,
        software::scaling::{context::Context as ScalerContext, flag::Flags},
        util::frame::video::Video as VideoFrame,
    },
    ffmpeg_next as ffmpeg,
    std::{ffi::CStr, ptr},
    tracing::{debug, info, trace, warn},
};

pub struct D3d11vaDecoder {
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
}

impl D3d11vaDecoder {
    pub fn new(width: u32, height: u32) -> Result<Self> {
        // Initialize FFmpeg
        ffmpeg::init()?;

        // Set FFmpeg log level to error only (suppress warnings)
        unsafe {
            ffmpeg::sys::av_log_set_level(ffmpeg::sys::AV_LOG_ERROR);
        }

        // Create D3D11VA device context
        let mut hw_device_ctx: *mut ffmpeg::sys::AVBufferRef = ptr::null_mut();

        info!("Initializing D3D11VA hardware decoder");

        unsafe {
            let ret = ffmpeg::sys::av_hwdevice_ctx_create(
                &mut hw_device_ctx,
                ffmpeg::sys::AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA,
                ptr::null(),
                ptr::null_mut(),
                0,
            );
            if ret < 0 {
                let mut errbuf = [0u8; 128];
                ffmpeg::sys::av_strerror(ret, errbuf.as_mut_ptr() as *mut i8, 128);
                let ffmpeg_err = CStr::from_ptr(errbuf.as_ptr() as *const i8)
                    .to_string_lossy()
                    .into_owned();

                let err_msg = if ret == -22 {
                    format!(
                        "D3D11VA: Invalid D3D11 device or driver not compatible (EINVAL). \
                         FFmpeg error: {}. \
                         Action: Update GPU drivers to latest version.",
                        ffmpeg_err
                    )
                } else if ret == -12 {
                    format!(
                        "D3D11VA: Out of memory when creating context (ENOMEM). \
                         FFmpeg error: {}. \
                         Action: Close GPU-intensive applications or increase BIOS UMA memory (AMD APU).",
                        ffmpeg_err
                    )
                } else {
                    format!(
                        "D3D11VA: Failed to create device context (code {}). \
                         FFmpeg error: {}. \
                         Action: Check GPU drivers, BIOS settings, or disable hwdecode.",
                        ret, ffmpeg_err
                    )
                };

                warn!("{}", err_msg);
                return Err(VideoError::InitializationError(err_msg));
            }
        }

        info!("D3D11VA device context created successfully");

        // Find H.264 decoder
        let codec = ffmpeg::decoder::find(codec::Id::H264).ok_or_else(|| {
            VideoError::InitializationError("H.264 decoder not found".to_string())
        })?;

        info!("Found H.264 decoder: {}", codec.name());

        // Create decoder context
        let mut context = codec::context::Context::new_with_codec(codec);

        unsafe {
            let ctx_ptr = context.as_mut_ptr();

            // Set hardware device context
            (*ctx_ptr).hw_device_ctx = ffmpeg::sys::av_buffer_ref(hw_device_ctx);

            // Set format callback to select D3D11VA
            (*ctx_ptr).get_format = Some(get_d3d11va_format);

            // Set initial dimensions as hints
            // D3D11VA will use these for initialization, then update from SPS/PPS
            (*ctx_ptr).width = width as i32;
            (*ctx_ptr).height = height as i32;

            // IMPORTANT: Request NV12 as software pixel format
            // D3D11VA will output NV12 after hw transfer
            (*ctx_ptr).sw_pix_fmt = ffmpeg::sys::AVPixelFormat::AV_PIX_FMT_NV12;

            // AMD GPU compatibility: Use conservative flags to avoid decoder rejection
            (*ctx_ptr).flags |= ffmpeg::sys::AV_CODEC_FLAG_LOW_DELAY as i32;
            (*ctx_ptr).strict_std_compliance = ffmpeg::sys::FF_COMPLIANCE_NORMAL;

            (*ctx_ptr).thread_count = 1;

            // AMD GPU workaround: Explicitly set hwaccel flags for better compatibility
            (*ctx_ptr).hwaccel_flags |= ffmpeg::sys::AV_HWACCEL_FLAG_ALLOW_PROFILE_MISMATCH;
        }

        let decoder = context.decoder().video().map_err(|e| {
            VideoError::InitializationError(format!(
                "Failed to create D3D11VA H.264 decoder: {e:?}"
            ))
        })?;

        debug!("D3D11VA H.264 decoder initialized: {width}x{height}");

        let mut decoder = Self {
            decoder,
            scaler: None,
            hw_device_ctx,
            width,
            height,
            output_format: Pixel::NV12,
            last_decoded_dimensions: None,
        };

        decoder.verify_hardware_support()?;

        Ok(decoder)
    }

    fn verify_hardware_support(&mut self) -> Result<()> {
        unsafe {
            let ctx_ptr = self.decoder.as_mut_ptr();

            // Iterate through all hardware configs (FFmpeg may list D3D11VA at any index)
            let mut i = 0;
            let mut found_d3d11va = false;

            debug!("D3D11VA: Enumerating available hardware configs for H.264 codec");

            loop {
                let hw_config = ffmpeg::sys::avcodec_get_hw_config((*ctx_ptr).codec, i);

                if hw_config.is_null() {
                    // End of config list
                    break;
                }

                let config = &*hw_config;
                debug!(
                    "D3D11VA: Found hw_config[{}]: device_type={:?}, pix_fmt={:?}, methods=0x{:x}",
                    i, config.device_type, config.pix_fmt, config.methods
                );

                if config.device_type == ffmpeg::sys::AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA {
                    info!("D3D11VA: Found D3D11VA config at index {}", i);
                    found_d3d11va = true;
                    break;
                }

                i += 1;
            }

            if !found_d3d11va {
                return Err(VideoError::InitializationError(
                    "D3D11VA: No D3D11VA config found in any hw_config index. \
                     This FFmpeg build may not support D3D11VA hardware acceleration."
                        .to_string(),
                ));
            }

            debug!(
                "D3D11VA hardware support verified (found at config index {})",
                i
            );
        }

        Ok(())
    }

    #[allow(dead_code)]
    fn ensure_scaler(&mut self) -> Result<()> {
        let input_format = self.decoder.format();
        let input_width = self.decoder.width();
        let input_height = self.decoder.height();

        let current_dimensions = (input_width, input_height);
        let needs_recreate = if let Some(last_dims) = self.last_decoded_dimensions {
            last_dims != current_dimensions
        } else {
            true
        };

        if needs_recreate {
            self.width = input_width;
            self.height = input_height;

            debug!(
                "Reinitializing scaler: {}x{} {:?} -> {}x{} {:?}",
                input_width,
                input_height,
                input_format,
                self.width,
                self.height,
                self.output_format
            );

            let scaler = ScalerContext::get(
                input_format,
                input_width,
                input_height,
                self.output_format,
                self.width,
                self.height,
                Flags::BILINEAR,
            )
            .map_err(|e| {
                VideoError::InitializationError(format!("Failed to create scaler context: {e:?}"))
            })?;

            self.scaler = Some(scaler);
            self.last_decoded_dimensions = Some(current_dimensions);
        }

        Ok(())
    }

    fn send_packet(&mut self, data: &[u8], pts: i64) -> Result<()> {
        super::packet::send_av_packet(&mut self.decoder, data, pts)
    }

    fn receive_frames(&mut self) -> Result<Vec<DecodedFrame>> {
        let mut frames = Vec::new();
        let mut consecutive_failures = 0;
        const MAX_CONSECUTIVE_FAILURES: u32 = 5;

        loop {
            let mut hw_frame = VideoFrame::empty();
            match self.decoder.receive_frame(&mut hw_frame) {
                Ok(_) => {
                    // Transfer from GPU to CPU
                    let mut sw_frame = VideoFrame::empty();
                    unsafe {
                        let ret = ffmpeg::sys::av_hwframe_transfer_data(
                            sw_frame.as_mut_ptr(),
                            hw_frame.as_ptr(),
                            0,
                        );
                        if ret < 0 {
                            warn!(
                                "D3D11VA: Failed to transfer frame from GPU (error {ret}). This may indicate GPU driver issues or insufficient BIOS UMA memory (AMD APU)."
                            );
                            consecutive_failures += 1;
                            if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                                return Err(VideoError::DecodingError(format!(
                                    "D3D11VA: {} consecutive GPU transfer failures. Common causes:\n\
                                     1. AMD APU: Increase BIOS UMA memory to 2GB (see docs/AMD_D3D11VA_TROUBLESHOOTING.md)\n\
                                     2. Update GPU drivers to latest version\n\
                                     3. Disable hardware decoding (hwdecode=false in config.toml)",
                                    consecutive_failures
                                )));
                            }
                            continue;
                        }
                    }

                    // Reset counter only after both decode AND GPU transfer succeed
                    consecutive_failures = 0;

                    trace!(
                        "Decoded frame (D3D11VA): {}x{} {:?} PTS={:?}",
                        sw_frame.width(),
                        sw_frame.height(),
                        sw_frame.format(),
                        sw_frame.timestamp()
                    );

                    // Update dimensions
                    self.width = sw_frame.width();
                    self.height = sw_frame.height();

                    // NV12 data extraction with proper linesize handling
                    let y_linesize = sw_frame.stride(0);
                    let uv_linesize = sw_frame.stride(1);
                    let width = self.width as usize;
                    let height = self.height as usize;

                    // Log linesize info (only once per resolution change)
                    if self.last_decoded_dimensions != Some((self.width, self.height)) {
                        info!(
                            "NV12 frame layout: {width}x{height}, Y linesize={y_linesize} UV linesize={uv_linesize}"
                        );
                        self.last_decoded_dimensions = Some((self.width, self.height));
                    }

                    // Y plane: copy line by line to remove padding
                    let y_size = width * height;
                    let uv_size = width * (height / 2); // NV12: UV interleaved, same width

                    let mut data = Vec::with_capacity(y_size + uv_size);

                    // Copy Y plane (remove linesize padding)
                    let y_data = sw_frame.data(0);
                    for row in 0..height {
                        let start = row * y_linesize;
                        let end = start + width;
                        data.extend_from_slice(&y_data[start..end]);
                    }

                    // Copy UV plane (remove linesize padding)
                    let uv_data = sw_frame.data(1);
                    let uv_height = height / 2;
                    for row in 0..uv_height {
                        let start = row * uv_linesize;
                        let end = start + width; // UV is interleaved, so same width in bytes
                        data.extend_from_slice(&uv_data[start..end]);
                    }

                    let pts = hw_frame.timestamp().unwrap_or(0);

                    frames.push(DecodedFrame {
                        width: self.width,
                        height: self.height,
                        data,
                        pts,
                        format: Pixel::NV12,
                    });
                }
                Err(ffmpeg::Error::Other { errno: 11 }) => {
                    break; // EAGAIN
                }
                Err(ffmpeg::Error::Eof) => {
                    debug!("Decoder EOF");
                    break;
                }
                Err(e) => {
                    consecutive_failures += 1;

                    if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                        return Err(VideoError::DecodingError(format!(
                            "D3D11VA: {} consecutive decode failures ({:?}). GPU hardware acceleration may be unsupported by this GPU/driver. Consider updating GPU drivers or disabling hardware decoding.",
                            consecutive_failures, e
                        )));
                    }

                    warn!(
                        "D3D11VA: Decode error ({:?}), attempt {}/{}. Retrying...",
                        e, consecutive_failures, MAX_CONSECUTIVE_FAILURES
                    );

                    continue;
                }
            }
        }

        Ok(frames)
    }
}

impl VideoDecoder for D3d11vaDecoder {
    fn decode(&mut self, packet_data: &[u8], pts: i64) -> Result<Option<DecodedFrame>> {
        if packet_data.is_empty() {
            return Ok(None);
        }

        self.send_packet(packet_data, pts)?;
        let frames = self.receive_frames()?;
        Ok(frames.into_iter().next())
    }

    fn flush(&mut self) -> Result<Vec<DecodedFrame>> {
        debug!("Flushing decoder");
        self.decoder.send_eof()?;
        self.receive_frames()
    }
}

impl Drop for D3d11vaDecoder {
    fn drop(&mut self) {
        unsafe {
            if !self.hw_device_ctx.is_null() {
                ffmpeg::sys::av_buffer_unref(&mut self.hw_device_ctx);
            }
        }
    }
}

// Callback for FFmpeg to select D3D11VA format
unsafe extern "C" fn get_d3d11va_format(
    _ctx: *mut ffmpeg::sys::AVCodecContext,
    pix_fmts: *const ffmpeg::sys::AVPixelFormat,
) -> ffmpeg::sys::AVPixelFormat {
    unsafe {
        if pix_fmts.is_null() {
            return ffmpeg::sys::AVPixelFormat::AV_PIX_FMT_NONE;
        }

        let mut p = pix_fmts;
        while *p != ffmpeg::sys::AVPixelFormat::AV_PIX_FMT_NONE {
            if *p == ffmpeg::sys::AVPixelFormat::AV_PIX_FMT_D3D11 {
                return *p;
            }
            p = p.offset(1);
        }
        ffmpeg::sys::AVPixelFormat::AV_PIX_FMT_NONE
    }
}
