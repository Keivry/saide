//! Automatic decoder selection
//!
//! This module provides an `AutoDecoder` that selects the appropriate video decoder
//! based on the detected GPU type (NVIDIA, Intel/AMD, or software fallback).

use {
    super::{DecodedFrame, H264Decoder, NvdecDecoder, VaapiDecoder, VideoDecoder},
    crate::{GpuType, detect_gpu, error::Result},
    tracing::{info, warn},
};

/// Auto-selecting video decoder based on available GPU
pub enum AutoDecoder {
    Nvdec(NvdecDecoder),
    Vaapi(VaapiDecoder),
    Software(H264Decoder),
}

impl AutoDecoder {
    /// Create decoder with automatic GPU detection
    pub fn new(width: u32, height: u32) -> Result<Self> {
        let gpu_type = detect_gpu();
        info!("Detected GPU type: {:?}", gpu_type);

        match gpu_type {
            GpuType::Nvidia => {
                info!("Using NVIDIA NVDEC hardware decoder");
                match NvdecDecoder::new(width, height) {
                    Ok(decoder) => Ok(Self::Nvdec(decoder)),
                    Err(e) => {
                        warn!(
                            "Failed to initialize NVDEC: {}, falling back to software",
                            e
                        );
                        Ok(Self::Software(H264Decoder::new(width, height)?))
                    }
                }
            }
            GpuType::Intel | GpuType::Amd => {
                info!("Using VAAPI hardware decoder (Intel/AMD)");
                match VaapiDecoder::new(width, height) {
                    Ok(decoder) => Ok(Self::Vaapi(decoder)),
                    Err(e) => {
                        warn!(
                            "Failed to initialize VAAPI: {}, falling back to software",
                            e
                        );
                        Ok(Self::Software(H264Decoder::new(width, height)?))
                    }
                }
            }
            GpuType::Unknown => {
                info!("Unknown GPU, using software decoder");
                Ok(Self::Software(H264Decoder::new(width, height)?))
            }
        }
    }

    /// Get current decoder type as string
    pub fn decoder_type(&self) -> &'static str {
        match self {
            Self::Nvdec(_) => "NVDEC",
            Self::Vaapi(_) => "VAAPI",
            Self::Software(_) => "Software",
        }
    }
}

impl VideoDecoder for AutoDecoder {
    fn decode(&mut self, packet: &[u8], pts: i64) -> Result<Option<DecodedFrame>> {
        match self {
            Self::Nvdec(d) => d.decode(packet, pts),
            Self::Vaapi(d) => d.decode(packet, pts),
            Self::Software(d) => d.decode(packet, pts),
        }
    }

    fn flush(&mut self) -> Result<Vec<DecodedFrame>> {
        match self {
            Self::Nvdec(d) => d.flush(),
            Self::Vaapi(d) => d.flush(),
            Self::Software(d) => d.flush(),
        }
    }
}
