//! Automatic decoder selection with cascade fallback
//!
//! This module provides an `AutoDecoder` that tries hardware decoders in priority order,
//! automatically falling back to the next available decoder on failure.
//!
//! Priority order (platform-specific):
//! - Linux: NVDEC → VAAPI → Software H.264
//! - Windows: NVDEC → D3D11VA → Software H.264

use {
    super::{DecodedFrame, H264Decoder, NvdecDecoder, VideoDecoder, error::Result},
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
    /// Create decoder with automatic cascade fallback
    ///
    /// Tries hardware decoders in priority order, automatically falling back on failure.
    /// Does NOT depend on GPU detection - decoders self-detect hardware availability.
    ///
    /// # Arguments
    /// * `width` - Video width
    /// * `height` - Video height
    /// * `hwdecode` - Enable hardware decoding. If false, force software decoder
    ///
    /// # Priority Order
    /// - Linux: NVDEC → VAAPI → Software
    /// - Windows: NVDEC → D3D11VA → Software
    #[cfg(not(target_os = "windows"))]
    pub fn new(width: u32, height: u32, hwdecode: bool) -> Result<Self> {
        if !hwdecode {
            info!("Hardware decoding disabled by config, using software decoder");
            return Ok(Self::Software(H264Decoder::new(width, height)?));
        }

        info!("Starting cascade decoder selection (NVDEC → VAAPI → Software)");

        if let Ok(decoder) = NvdecDecoder::new(width, height) {
            info!("✅ Using NVDEC hardware decoder");
            return Ok(Self::Nvdec(decoder));
        }
        warn!("NVDEC unavailable, trying VAAPI");

        if let Ok(decoder) = VaapiDecoder::new(width, height) {
            info!("✅ Using VAAPI hardware decoder");
            return Ok(Self::Vaapi(decoder));
        }
        warn!("VAAPI unavailable, falling back to software decoder");

        info!("Using software H.264 decoder");
        Ok(Self::Software(H264Decoder::new(width, height)?))
    }

    /// Create decoder with automatic cascade fallback (Windows)
    ///
    /// Tries hardware decoders in priority order, automatically falling back on failure.
    /// Does NOT depend on GPU detection - decoders self-detect hardware availability.
    ///
    /// # Arguments
    /// * `width` - Video width
    /// * `height` - Video height
    /// * `hwdecode` - Enable hardware decoding. If false, force software decoder
    ///
    /// # Priority Order
    /// - NVDEC (NVIDIA CUDA, cross-platform)
    /// - D3D11VA (DirectX 11, Intel/AMD/NVIDIA)
    /// - Software H.264
    #[cfg(target_os = "windows")]
    pub fn new(width: u32, height: u32, hwdecode: bool) -> Result<Self> {
        if !hwdecode {
            info!("Hardware decoding disabled by config, using software decoder");
            return Ok(Self::Software(H264Decoder::new(width, height)?));
        }

        info!("Starting cascade decoder selection (NVDEC → D3D11VA → Software)");

        if let Ok(decoder) = NvdecDecoder::new(width, height) {
            info!("✅ Using NVDEC hardware decoder");
            return Ok(Self::Nvdec(decoder));
        }
        warn!("NVDEC unavailable, trying D3D11VA");

        if let Ok(decoder) = D3d11vaDecoder::new(width, height) {
            info!("✅ Using D3D11VA hardware decoder");
            return Ok(Self::D3d11va(decoder));
        }
        warn!("D3D11VA unavailable, falling back to software decoder");

        info!("Using software H.264 decoder");
        Ok(Self::Software(H264Decoder::new(width, height)?))
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
