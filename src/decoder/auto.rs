//! Automatic GPU detection and decoder selection

use {
    super::{DecodedFrame, H264Decoder, NvdecDecoder, VaapiDecoder, VideoDecoder},
    anyhow::Result,
    std::{fs, path::Path},
    tracing::{debug, info, warn},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuType {
    Nvidia,
    Intel,
    Amd,
    Unknown,
}

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
                        warn!("Failed to initialize NVDEC: {}, falling back to software", e);
                        Ok(Self::Software(H264Decoder::new(width, height)?))
                    }
                }
            }
            GpuType::Intel | GpuType::Amd => {
                info!("Using VAAPI hardware decoder (Intel/AMD)");
                match VaapiDecoder::new(width, height) {
                    Ok(decoder) => Ok(Self::Vaapi(decoder)),
                    Err(e) => {
                        warn!("Failed to initialize VAAPI: {}, falling back to software", e);
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

/// Detect current GPU type
pub fn detect_gpu() -> GpuType {
    // 1. Check for NVIDIA GPU
    if is_nvidia_gpu_available() {
        return GpuType::Nvidia;
    }

    // 2. Check for Intel/AMD via DRM
    if let Some(gpu) = detect_drm_gpu() {
        return gpu;
    }

    // 3. Unknown
    GpuType::Unknown
}

/// Check if NVIDIA GPU is available and being used
fn is_nvidia_gpu_available() -> bool {
    // Method 1: Check NVIDIA driver
    if Path::new("/proc/driver/nvidia/version").exists() {
        debug!("Detected NVIDIA driver via /proc");
        return true;
    }

    // Method 2: Check if nvidia-smi works
    if let Ok(output) = std::process::Command::new("nvidia-smi")
        .arg("--query-gpu=name")
        .arg("--format=csv,noheader")
        .output()
    {
        if output.status.success() && !output.stdout.is_empty() {
            debug!("Detected NVIDIA GPU via nvidia-smi");
            return true;
        }
    }

    // Method 3: Check for NVIDIA render devices
    for entry in fs::read_dir("/dev/dri").ok().into_iter().flatten() {
        if let Ok(entry) = entry {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with("renderD") {
                // Check if this is NVIDIA device
                if let Some(vendor) = get_device_vendor(&entry.path()) {
                    if vendor == 0x10de {
                        // NVIDIA vendor ID
                        debug!("Detected NVIDIA GPU via DRM device");
                        return true;
                    }
                }
            }
        }
    }

    false
}

/// Detect Intel/AMD GPU via DRM
fn detect_drm_gpu() -> Option<GpuType> {
    // Check all card devices
    for entry in fs::read_dir("/sys/class/drm").ok()?.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        
        // Skip non-card devices
        if !name.to_string_lossy().starts_with("card") {
            continue;
        }

        // Skip card-X where X is not a digit (e.g., card0-HDMI-A-1)
        if let Some(card_name) = name.to_string_lossy().strip_prefix("card") {
            if !card_name.chars().next()?.is_ascii_digit() {
                continue;
            }
        }

        let vendor_path = path.join("device/vendor");
        if let Ok(vendor_str) = fs::read_to_string(&vendor_path) {
            if let Ok(vendor) = u32::from_str_radix(vendor_str.trim().trim_start_matches("0x"), 16) {
                debug!("Found GPU vendor: 0x{:04x} at {:?}", vendor, path);
                
                match vendor {
                    0x8086 => {
                        debug!("Detected Intel GPU");
                        return Some(GpuType::Intel);
                    }
                    0x1002 => {
                        debug!("Detected AMD GPU");
                        return Some(GpuType::Amd);
                    }
                    _ => {}
                }
            }
        }
    }

    None
}

/// Get device vendor ID from render device
fn get_device_vendor(device_path: &Path) -> Option<u32> {
    // Extract device number from renderDXXX
    let name = device_path.file_name()?.to_string_lossy();
    let num_str = name.strip_prefix("renderD")?;
    let device_num: u32 = num_str.parse().ok()?;
    
    // Map renderDXXX to cardY
    let card_num = device_num - 128; // renderD128 -> card0
    
    let vendor_path = format!("/sys/class/drm/card{}/device/vendor", card_num);
    let vendor_str = fs::read_to_string(&vendor_path).ok()?;
    
    u32::from_str_radix(vendor_str.trim().trim_start_matches("0x"), 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_gpu() {
        let gpu = detect_gpu();
        println!("Detected GPU: {:?}", gpu);
        assert_ne!(gpu, GpuType::Unknown);
    }
}
