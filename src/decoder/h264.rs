// SPDX-License-Identifier: MIT OR Apache-2.0

//! H.264 video decoder using FFmpeg

use {
    super::{
        DecodedFrame,
        VideoDecoder,
        error::{Result, VideoError},
    },
    ffmpeg::{
        codec,
        format::Pixel,
        software::scaling::{context::Context as ScalerContext, flag::Flags},
        util::frame::video::Video as VideoFrame,
    },
    ffmpeg_next as ffmpeg,
    tracing::{debug, info, trace, warn},
};

pub struct H264Decoder {
    decoder: ffmpeg::decoder::Video,
    scaler: Option<ScalerContext>,
    width: u32,
    height: u32,
    output_format: Pixel,
    /// Track last decoded frame dimensions to detect resolution changes
    last_decoded_dimensions: Option<(u32, u32)>,
}

impl H264Decoder {
    pub fn new(width: u32, height: u32) -> Result<Self> {
        // Initialize FFmpeg (idempotent)
        ffmpeg::init()?;

        // Set FFmpeg log level to error only (suppress warnings)
        unsafe {
            ffmpeg::sys::av_log_set_level(ffmpeg::sys::AV_LOG_ERROR);
        }

        // Find H.264 decoder
        let codec = ffmpeg::decoder::find(codec::Id::H264).ok_or_else(|| {
            VideoError::InitializationError("H.264 decoder not found".to_string())
        })?;

        info!("Found H.264 decoder: {}", codec.name());

        // Create decoder context
        let mut context = codec::context::Context::new_with_codec(codec);

        // Set dimensions
        // Note: Actual dimensions may change based on stream SPS/PPS
        // We set initial values here; they will be updated upon decoding frames.
        unsafe {
            let ctx_ptr = context.as_mut_ptr();
            (*ctx_ptr).width = width as i32;
            (*ctx_ptr).height = height as i32;
            (*ctx_ptr).pix_fmt = ffmpeg::format::Pixel::YUV420P.into();

            (*ctx_ptr).flags |= ffmpeg::sys::AV_CODEC_FLAG_LOW_DELAY as i32;
            (*ctx_ptr).flags2 |= ffmpeg::sys::AV_CODEC_FLAG2_FAST;
            (*ctx_ptr).strict_std_compliance = ffmpeg::sys::FF_COMPLIANCE_EXPERIMENTAL;

            (*ctx_ptr).thread_count = 1;
        }

        // Open decoder
        let decoder = context.decoder().video().map_err(|e| {
            VideoError::InitializationError(format!("Failed to create H.264 decoder: {e:?}"))
        })?;

        debug!("H.264 decoder initialized: {width}x{height}");

        Ok(Self {
            decoder,
            scaler: None,
            width,
            height,
            output_format: Pixel::RGBA,
            last_decoded_dimensions: None,
        })
    }

    /// Initialize scaler when we know the actual input format
    fn ensure_scaler(&mut self) -> Result<()> {
        let input_format = self.decoder.format();
        let input_width = self.decoder.width();
        let input_height = self.decoder.height();

        // Check if resolution changed
        let current_dimensions = (input_width, input_height);
        let needs_recreate = if let Some(last_dims) = self.last_decoded_dimensions {
            last_dims != current_dimensions
        } else {
            true
        };

        if needs_recreate {
            // Update output dimensions to match input (no scaling)
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
                VideoError::InitializationError(format!("Failed to create scaler: {e:?}"))
            })?;

            self.scaler = Some(scaler);
            self.last_decoded_dimensions = Some(current_dimensions);
        }

        Ok(())
    }

    /// Send packet to decoder
    fn send_packet(&mut self, data: &[u8], pts: i64) -> Result<()> {
        super::packet::send_av_packet(&mut self.decoder, data, pts)
    }

    /// Receive decoded frames
    fn receive_frames(&mut self) -> Result<Vec<DecodedFrame>> {
        let mut frames = Vec::new();

        loop {
            let mut decoded = VideoFrame::empty();
            match self.decoder.receive_frame(&mut decoded) {
                Ok(_) => {
                    trace!(
                        "Decoded frame: {}x{} {:?} PTS={:?}",
                        decoded.width(),
                        decoded.height(),
                        decoded.format(),
                        decoded.timestamp()
                    );

                    // Ensure scaler is initialized
                    self.ensure_scaler()?;

                    // Convert to RGBA
                    let mut rgb_frame = VideoFrame::empty();
                    if let Some(scaler) = &mut self.scaler {
                        scaler.run(&decoded, &mut rgb_frame).map_err(|e| {
                            VideoError::DecodingError(format!("Failed to scale frame: {e:?}"))
                        })?;

                        // RGBA data extraction with proper linesize handling
                        let linesize = rgb_frame.stride(0);
                        let width = self.width as usize;
                        let height = self.height as usize;
                        let bytes_per_pixel = 4;

                        // Log linesize info (only once per resolution change)
                        if self.last_decoded_dimensions != Some((self.width, self.height)) {
                            info!(
                                "RGBA frame layout: {}x{}, linesize={} (expected={})",
                                width,
                                height,
                                linesize,
                                width * bytes_per_pixel
                            );
                            self.last_decoded_dimensions = Some((self.width, self.height));
                        }

                        // Copy line by line to remove padding
                        let expected_stride = width * bytes_per_pixel;
                        let data = if linesize == expected_stride {
                            // No padding - direct copy
                            rgb_frame.data(0)[0..(width * height * bytes_per_pixel)].to_vec()
                        } else {
                            // Has padding - copy line by line
                            let mut data = Vec::with_capacity(width * height * bytes_per_pixel);
                            let src = rgb_frame.data(0);
                            for row in 0..height {
                                let start = row * linesize;
                                let end = start + expected_stride;
                                data.extend_from_slice(&src[start..end]);
                            }
                            data
                        };

                        let pts = decoded.timestamp().unwrap_or(0);

                        frames.push(DecodedFrame {
                            width: self.width,
                            height: self.height,
                            data,
                            pts,
                            format: self.output_format,
                        });
                    }
                }
                Err(ffmpeg::Error::Other { errno: 11 }) => {
                    // EAGAIN - need more data
                    break;
                }
                Err(ffmpeg::Error::Eof) => {
                    debug!("Decoder EOF");
                    break;
                }
                Err(e) => {
                    return Err(VideoError::DecodingError(format!(
                        "Failed to receive frame from decoder: {e:?}",
                    )));
                }
            }
        }

        Ok(frames)
    }
}

impl VideoDecoder for H264Decoder {
    fn decode(&mut self, packet_data: &[u8], pts: i64) -> Result<Option<DecodedFrame>> {
        if packet_data.is_empty() {
            warn!("Empty packet received");
            return Ok(None);
        }

        trace!("Decoding packet: {} bytes, PTS={}", packet_data.len(), pts);

        // Send packet
        self.send_packet(packet_data, pts)?;

        // Try to receive frames
        let frames = self.receive_frames()?;

        // Return first frame if available
        Ok(frames.into_iter().next())
    }

    fn flush(&mut self) -> Result<Vec<DecodedFrame>> {
        debug!("Flushing decoder");

        // Send EOF
        self.decoder.send_eof()?;

        // Receive all remaining frames
        self.receive_frames()
    }
}
