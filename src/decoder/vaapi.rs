//! VAAPI hardware-accelerated H.264 decoder

use {
    super::{DecodedFrame, VideoDecoder},
    anyhow::{Context as AnyhowContext, Result, bail},
    ffmpeg::{
        codec, format::Pixel,
        software::scaling::{context::Context as ScalerContext, flag::Flags},
        util::frame::video::Video as VideoFrame,
    },
    ffmpeg_next as ffmpeg,
    std::ptr,
    tracing::{debug, info, warn},
};

pub struct VaapiDecoder {
    decoder: ffmpeg::decoder::Video,
    scaler: Option<ScalerContext>,
    hw_device_ctx: *mut ffmpeg::sys::AVBufferRef,
    width: u32,
    height: u32,
    output_format: Pixel,
    last_decoded_dimensions: Option<(u32, u32)>,
}

impl VaapiDecoder {
    pub fn new(width: u32, height: u32) -> Result<Self> {
        // Initialize FFmpeg
        ffmpeg::init().context("Failed to initialize FFmpeg")?;

        // Create VAAPI device context
        let mut hw_device_ctx: *mut ffmpeg::sys::AVBufferRef = ptr::null_mut();
        let device_path = b"/dev/dri/renderD128\0".as_ptr() as *const i8;
        
        unsafe {
            let ret = ffmpeg::sys::av_hwdevice_ctx_create(
                &mut hw_device_ctx,
                ffmpeg::sys::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI,
                device_path,
                ptr::null_mut(),
                0,
            );
            if ret < 0 {
                bail!("Failed to create VAAPI device context: {}", ret);
            }
        }

        info!("VAAPI device context created: /dev/dri/renderD128");

        // Find H.264 decoder
        let codec = ffmpeg::decoder::find(codec::Id::H264)
            .context("H.264 decoder not found")?;

        info!("Found H.264 decoder: {}", codec.name());

        // Create decoder context
        let mut context = codec::context::Context::new_with_codec(codec);

        unsafe {
            let ctx_ptr = context.as_mut_ptr();
            (*ctx_ptr).hw_device_ctx = ffmpeg::sys::av_buffer_ref(hw_device_ctx);
            (*ctx_ptr).get_format = Some(get_vaapi_format);
            (*ctx_ptr).width = width as i32;
            (*ctx_ptr).height = height as i32;
        }

        let decoder = context
            .decoder()
            .video()
            .context("Failed to open H.264 decoder")?;

        debug!("VAAPI H.264 decoder initialized: {}x{}", width, height);

        Ok(Self {
            decoder,
            scaler: None,
            hw_device_ctx,
            width,
            height,
            output_format: Pixel::NV12, // VAAPI native format
            last_decoded_dimensions: None,
        })
    }

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
                input_width, input_height, input_format,
                self.width, self.height, self.output_format
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
            .context("Failed to create scaler")?;

            self.scaler = Some(scaler);
            self.last_decoded_dimensions = Some(current_dimensions);
        }

        Ok(())
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
                    // Transfer from GPU to CPU
                    let mut sw_frame = VideoFrame::empty();
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

                    debug!(
                        "Decoded frame (VAAPI): {}x{} {:?} PTS={:?}",
                        sw_frame.width(),
                        sw_frame.height(),
                        sw_frame.format(),
                        sw_frame.timestamp()
                    );

                    // Update dimensions
                    self.width = sw_frame.width();
                    self.height = sw_frame.height();

                    // NV12 data: Y plane + UV interleaved
                    // For now, just extract Y plane + U/V planes
                    let y_size = (self.width * self.height) as usize;
                    let uv_size = y_size / 2; // NV12: UV is half size but interleaved
                    
                    let mut data = Vec::with_capacity(y_size + uv_size);
                    data.extend_from_slice(&sw_frame.data(0)[..y_size]);
                    data.extend_from_slice(&sw_frame.data(1)[..uv_size]);

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
                    bail!("Decoder error: {}", e);
                }
            }
        }

        Ok(frames)
    }
}

impl VideoDecoder for VaapiDecoder {
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
        self.decoder.send_eof().context("Failed to send EOF")?;
        self.receive_frames()
    }
}

impl Drop for VaapiDecoder {
    fn drop(&mut self) {
        unsafe {
            if !self.hw_device_ctx.is_null() {
                ffmpeg::sys::av_buffer_unref(&mut self.hw_device_ctx);
            }
        }
    }
}

// Callback for FFmpeg to select VAAPI format
unsafe extern "C" fn get_vaapi_format(
    _ctx: *mut ffmpeg::sys::AVCodecContext,
    pix_fmts: *const ffmpeg::sys::AVPixelFormat,
) -> ffmpeg::sys::AVPixelFormat {
    unsafe {
        let mut p = pix_fmts;
        while *p != ffmpeg::sys::AVPixelFormat::AV_PIX_FMT_NONE {
            if *p == ffmpeg::sys::AVPixelFormat::AV_PIX_FMT_VAAPI {
                return *p;
            }
            p = p.offset(1);
        }
        ffmpeg::sys::AVPixelFormat::AV_PIX_FMT_NONE
    }
}
