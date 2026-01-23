//! Automatic decoder selection
//!
//! This module provides an `AutoDecoder` that selects the appropriate video decoder
//! based on the detected GPU type (NVIDIA, Intel/AMD, or software fallback).

use {
    super::{DecodedFrame, H264Decoder, NvdecDecoder, VideoDecoder, error::Result},
    crate::{GpuType, detect_gpu},
    tracing::{info, warn},
};

#[cfg(target_os = "windows")]
use super::D3d11vaDecoder;
#[cfg(not(target_os = "windows"))]
use super::VaapiDecoder;

/// Auto-selecting video decoder based on available GPU
#[cfg(not(target_os = "windows"))]
pub enum AutoDecoder {
    Nvdec(NvdecDecoder),
    Vaapi(VaapiDecoder),
    Software(H264Decoder),
}

/// Auto-selecting video decoder based on available GPU (Windows)
#[cfg(target_os = "windows")]
pub enum AutoDecoder {
    Nvdec(NvdecDecoder),
    D3d11va(D3d11vaDecoder),
    Software(H264Decoder),
}

impl AutoDecoder {
    /// Create decoder with automatic GPU detection
    ///
    /// # Arguments
    /// * `width` - Video width
    /// * `height` - Video height
    /// * `hwdecode` - Enable hardware decoding (VAAPI/NVDEC). If false, force software decoder
    #[cfg(not(target_os = "windows"))]
    pub fn new(width: u32, height: u32, hwdecode: bool) -> Result<Self> {
        if !hwdecode {
            info!("Hardware decoding disabled by config, using software decoder");
            return Ok(Self::Software(H264Decoder::new(width, height)?));
        }

        let gpu_type = detect_gpu();
        info!("Detected GPU type: {gpu_type:?}");

        match gpu_type {
            GpuType::Nvidia => {
                info!("Using NVIDIA NVDEC hardware decoder");
                match NvdecDecoder::new(width, height) {
                    Ok(decoder) => Ok(Self::Nvdec(decoder)),
                    Err(e) => {
                        warn!("Failed to initialize NVDEC: {e:?}, falling back to software",);
                        Ok(Self::Software(H264Decoder::new(width, height)?))
                    }
                }
            }
            GpuType::Intel | GpuType::Amd => {
                info!("Using VAAPI hardware decoder (Intel/AMD)");
                match VaapiDecoder::new(width, height) {
                    Ok(decoder) => Ok(Self::Vaapi(decoder)),
                    Err(e) => {
                        warn!("Failed to initialize VAAPI: {e:?}, falling back to software",);
                        Ok(Self::Software(H264Decoder::new(width, height)?))
                    }
                }
            }
            GpuType::Apple => {
                info!(
                    "Using software decoder (Apple Silicon - VideoToolbox decoder not yet implemented)"
                );
                Ok(Self::Software(H264Decoder::new(width, height)?))
            }
            GpuType::Software => {
                info!("Using software decoder (explicit)");
                Ok(Self::Software(H264Decoder::new(width, height)?))
            }
            GpuType::Unknown => {
                info!("Unknown GPU, using software decoder");
                Ok(Self::Software(H264Decoder::new(width, height)?))
            }
        }
    }

    /// Create decoder with automatic GPU detection (Windows)
    ///
    /// # Arguments
    /// * `width` - Video width
    /// * `height` - Video height
    /// * `hwdecode` - Enable hardware decoding (D3D11VA/NVDEC). If false, force software decoder
    #[cfg(target_os = "windows")]
    pub fn new(width: u32, height: u32, hwdecode: bool) -> Result<Self> {
        if !hwdecode {
            info!("Hardware decoding disabled by config, using software decoder");
            return Ok(Self::Software(H264Decoder::new(width, height)?));
        }

        let gpu_type = detect_gpu();
        info!("Detected GPU type: {gpu_type:?}");

        match gpu_type {
            GpuType::Nvidia => {
                info!("Using NVIDIA NVDEC hardware decoder");
                match NvdecDecoder::new(width, height) {
                    Ok(decoder) => Ok(Self::Nvdec(decoder)),
                    Err(e) => {
                        warn!("Failed to initialize NVDEC: {e:?}, falling back to D3D11VA",);
                        match D3d11vaDecoder::new(width, height) {
                            Ok(decoder) => Ok(Self::D3d11va(decoder)),
                            Err(e2) => {
                                warn!(
                                    "Failed to initialize D3D11VA: {e2:?}, falling back to software"
                                );
                                Ok(Self::Software(H264Decoder::new(width, height)?))
                            }
                        }
                    }
                }
            }
            GpuType::Intel | GpuType::Amd => {
                info!("Using D3D11VA hardware decoder (Intel/AMD)");
                match D3d11vaDecoder::new(width, height) {
                    Ok(decoder) => Ok(Self::D3d11va(decoder)),
                    Err(e) => {
                        warn!("Failed to initialize D3D11VA: {e:?}, falling back to software",);
                        Ok(Self::Software(H264Decoder::new(width, height)?))
                    }
                }
            }
            GpuType::Apple => {
                info!("Using software decoder (Apple GPU on Windows - unsupported)");
                Ok(Self::Software(H264Decoder::new(width, height)?))
            }
            GpuType::Software => {
                info!("Using software decoder (explicit)");
                Ok(Self::Software(H264Decoder::new(width, height)?))
            }
            GpuType::Unknown => {
                info!("Unknown GPU, trying D3D11VA then software");
                match D3d11vaDecoder::new(width, height) {
                    Ok(decoder) => Ok(Self::D3d11va(decoder)),
                    Err(e) => {
                        warn!("Failed to initialize D3D11VA: {e:?}, falling back to software");
                        Ok(Self::Software(H264Decoder::new(width, height)?))
                    }
                }
            }
        }
    }

    /// Get current decoder type as string
    #[cfg(not(target_os = "windows"))]
    pub fn decoder_type(&self) -> &'static str {
        match self {
            Self::Nvdec(_) => "NVDEC",
            Self::Vaapi(_) => "VAAPI",
            Self::Software(_) => "Software",
        }
    }

    /// Get current decoder type as string (Windows)
    #[cfg(target_os = "windows")]
    pub fn decoder_type(&self) -> &'static str {
        match self {
            Self::Nvdec(_) => "NVDEC",
            Self::D3d11va(_) => "D3D11VA",
            Self::Software(_) => "Software",
        }
    }
}

impl VideoDecoder for AutoDecoder {
    #[cfg(not(target_os = "windows"))]
    fn decode(&mut self, packet: &[u8], pts: i64) -> Result<Option<DecodedFrame>> {
        match self {
            Self::Nvdec(d) => d.decode(packet, pts),
            Self::Vaapi(d) => d.decode(packet, pts),
            Self::Software(d) => d.decode(packet, pts),
        }
    }

    #[cfg(target_os = "windows")]
    fn decode(&mut self, packet: &[u8], pts: i64) -> Result<Option<DecodedFrame>> {
        match self {
            Self::Nvdec(d) => d.decode(packet, pts),
            Self::D3d11va(d) => d.decode(packet, pts),
            Self::Software(d) => d.decode(packet, pts),
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn flush(&mut self) -> Result<Vec<DecodedFrame>> {
        match self {
            Self::Nvdec(d) => d.flush(),
            Self::Vaapi(d) => d.flush(),
            Self::Software(d) => d.flush(),
        }
    }

    #[cfg(target_os = "windows")]
    fn flush(&mut self) -> Result<Vec<DecodedFrame>> {
        match self {
            Self::Nvdec(d) => d.flush(),
            Self::D3d11va(d) => d.flush(),
            Self::Software(d) => d.flush(),
        }
    }
}
