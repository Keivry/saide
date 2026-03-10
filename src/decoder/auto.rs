//! Automatic decoder selection with cascade fallback
//!
//! This module provides an `AutoDecoder` that tries hardware decoders in priority order,
//! automatically falling back to the next available decoder on failure.
//!
//! Priority order (platform-specific):
//! - Linux: NVDEC → VAAPI → Software H.264
//! - Windows: NVDEC → D3D11VA → Software H.264

use {
    super::{error::Result, DecodedFrame, H264Decoder, NvdecDecoder, VideoDecoder},
    tracing::{info, warn},
};

#[cfg(target_os = "windows")]
use super::D3d11vaDecoder;
#[cfg(not(target_os = "windows"))]
use super::VaapiDecoder;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecoderPreference {
    Nvdec,
    #[cfg(target_os = "windows")]
    D3d11va,
    #[cfg(not(target_os = "windows"))]
    Vaapi,
}

impl DecoderPreference {
    pub fn profile_name(self) -> &'static str {
        match self {
            Self::Nvdec => "NVDEC",
            #[cfg(target_os = "windows")]
            Self::D3d11va => "D3D11VA",
            #[cfg(not(target_os = "windows"))]
            Self::Vaapi => "VAAPI",
        }
    }

    pub fn from_profile_name(name: &str) -> Option<Self> {
        if name.eq_ignore_ascii_case("NVDEC") {
            return Some(Self::Nvdec);
        }

        #[cfg(target_os = "windows")]
        if name.eq_ignore_ascii_case("D3D11VA") {
            return Some(Self::D3d11va);
        }

        #[cfg(not(target_os = "windows"))]
        if name.eq_ignore_ascii_case("VAAPI") {
            return Some(Self::Vaapi);
        }

        None
    }

    #[cfg(target_os = "windows")]
    pub fn hardware_candidates() -> &'static [Self] {
        const CANDIDATES: &[DecoderPreference] =
            &[DecoderPreference::Nvdec, DecoderPreference::D3d11va];
        CANDIDATES
    }

    #[cfg(not(target_os = "windows"))]
    pub fn hardware_candidates() -> &'static [Self] {
        const CANDIDATES: &[DecoderPreference] =
            &[DecoderPreference::Nvdec, DecoderPreference::Vaapi];
        CANDIDATES
    }
}

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
    #[cfg(not(target_os = "windows"))]
    pub fn new_exact(width: u32, height: u32, preferred: DecoderPreference) -> Result<Self> {
        match preferred {
            DecoderPreference::Nvdec => Ok(Self::Nvdec(NvdecDecoder::new(width, height)?)),
            DecoderPreference::Vaapi => Ok(Self::Vaapi(VaapiDecoder::new(width, height)?)),
        }
    }

    #[cfg(target_os = "windows")]
    pub fn new_exact(width: u32, height: u32, preferred: DecoderPreference) -> Result<Self> {
        match preferred {
            DecoderPreference::Nvdec => Ok(Self::Nvdec(NvdecDecoder::new(width, height)?)),
            DecoderPreference::D3d11va => Ok(Self::D3d11va(D3d11vaDecoder::new(width, height)?)),
        }
    }

    /// Determine if orientation lock is needed for hardware decoding
    ///
    /// This is a conservative prediction made BEFORE decoder creation.
    /// Called during scrcpy-server startup to set capture_orientation parameter.
    ///
    /// # Rationale
    ///
    /// Some hardware decoders cannot handle resolution changes at runtime:
    /// - **NVDEC** (NVIDIA): Requires fixed resolution, needs orientation lock
    /// - **D3D11VA** (Windows DirectX): Requires fixed resolution, needs orientation lock
    /// - **VAAPI** (Linux Intel/AMD): Can handle dynamic resolution, no lock needed
    ///
    /// # Strategy
    ///
    /// - **Windows**: Always lock if `hwdecode=true`
    ///   - Both D3D11VA and NVDEC require orientation lock
    ///   - Conservative approach: lock for all hardware decoders on Windows
    ///
    /// - **Linux**: Lock only if NVIDIA GPU is detected
    ///   - NVDEC needs lock, VAAPI doesn't
    ///   - Use GPU detection to predict if NVDEC will be used
    ///   - If non-NVIDIA GPU → VAAPI will be selected → no lock needed
    ///
    /// # Arguments
    ///
    /// * `hwdecode` - Whether hardware decoding is enabled in config
    ///
    /// # Returns
    ///
    /// `true` if scrcpy-server should set `capture_orientation=0` (lock rotation)
    pub fn needs_orientation_lock(hwdecode: bool) -> bool {
        if !hwdecode {
            return false;
        }

        #[cfg(target_os = "windows")]
        {
            // Windows: Both D3D11VA and NVDEC require orientation lock
            // Conservative: lock for all hardware decoders on Windows
            info!("Windows hardware decoding enabled, will lock capture orientation");
            true
        }

        #[cfg(not(target_os = "windows"))]
        {
            use crate::gpu::{detect_gpu, GpuType};

            // Linux: Only NVDEC needs lock, VAAPI can handle dynamic resolution
            match detect_gpu() {
                GpuType::Nvidia => {
                    info!("NVIDIA GPU detected, will lock orientation for potential NVDEC usage");
                    true
                }
                _ => {
                    info!("Non-NVIDIA GPU detected, VAAPI will handle dynamic resolution");
                    false
                }
            }
        }
    }

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
    pub fn new(
        width: u32,
        height: u32,
        hwdecode: bool,
        preferred: Option<DecoderPreference>,
    ) -> Result<Self> {
        if !hwdecode {
            info!("Hardware decoding disabled by config, using software decoder");
            return Ok(Self::Software(H264Decoder::new(width, height)?));
        }

        info!("Starting cascade decoder selection (NVDEC → VAAPI → Software)");

        if let Some(preferred) = preferred {
            match Self::new_exact(width, height, preferred) {
                Ok(decoder) => {
                    info!(
                        "✅ Using probe-validated preferred {} hardware decoder",
                        preferred.profile_name()
                    );
                    return Ok(decoder);
                }
                Err(e) => {
                    warn!(
                        "Preferred {} decoder unavailable: {e}; falling back to cascade",
                        preferred.profile_name()
                    );
                }
            }
        }

        for candidate in DecoderPreference::hardware_candidates() {
            if Some(*candidate) == preferred {
                continue;
            }

            match Self::new_exact(width, height, *candidate) {
                Ok(decoder) => {
                    info!("✅ Using {} hardware decoder", candidate.profile_name());
                    return Ok(decoder);
                }
                Err(e) => {
                    warn!("{} unavailable: {e}", candidate.profile_name());
                }
            }
        }

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
    pub fn new(
        width: u32,
        height: u32,
        hwdecode: bool,
        preferred: Option<DecoderPreference>,
    ) -> Result<Self> {
        if !hwdecode {
            info!("Hardware decoding disabled by config, using software decoder");
            return Ok(Self::Software(H264Decoder::new(width, height)?));
        }

        info!("Starting cascade decoder selection (NVDEC → D3D11VA → Software)");

        if let Some(preferred) = preferred {
            match Self::new_exact(width, height, preferred) {
                Ok(decoder) => {
                    info!(
                        "✅ Using probe-validated preferred {} hardware decoder",
                        preferred.profile_name()
                    );
                    return Ok(decoder);
                }
                Err(e) => {
                    warn!(
                        "Preferred {} decoder unavailable: {e}; falling back to cascade",
                        preferred.profile_name()
                    );
                }
            }
        }

        for candidate in DecoderPreference::hardware_candidates() {
            if Some(*candidate) == preferred {
                continue;
            }

            match Self::new_exact(width, height, *candidate) {
                Ok(decoder) => {
                    info!("✅ Using {} hardware decoder", candidate.profile_name());
                    return Ok(decoder);
                }
                Err(e) => {
                    warn!("{} unavailable: {e}", candidate.profile_name());
                }
            }
        }

        info!("Using software H.264 decoder");
        Ok(Self::Software(H264Decoder::new(width, height)?))
    }

    /// Get current decoder type as string
    #[cfg(not(target_os = "windows"))]
    pub fn decoder_type(&self) -> &'static str {
        match self {
            Self::Nvdec(_) => DecoderPreference::Nvdec.profile_name(),
            Self::Vaapi(_) => DecoderPreference::Vaapi.profile_name(),
            Self::Software(_) => "Software",
        }
    }

    /// Get current decoder type as string (Windows)
    #[cfg(target_os = "windows")]
    pub fn decoder_type(&self) -> &'static str {
        match self {
            Self::Nvdec(_) => DecoderPreference::Nvdec.profile_name(),
            Self::D3d11va(_) => DecoderPreference::D3d11va.profile_name(),
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
