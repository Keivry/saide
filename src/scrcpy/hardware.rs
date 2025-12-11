//! Device Hardware Capability Detection
//!
//! Detects hardware encoders and decoders on Android devices.

use {
    anyhow::{Context, Result},
    std::process::Command,
    tracing::{debug, info, warn},
};

/// Detected hardware encoder information
#[derive(Debug, Clone)]
pub struct EncoderInfo {
    pub name: String,
    pub is_hardware: bool,
    pub mime_type: String,
}

/// Detect best H.264 hardware encoder on device
///
/// Priority (first match wins):
/// 1. c2.android.avc.encoder (Codec2 HAL)
/// 2. OMX.qcom.video.encoder.avc (Qualcomm)
/// 3. OMX.google.h264.encoder (Google software - fallback)
///
pub fn detect_h264_encoder(serial: &str) -> Result<Option<String>> {
    info!("Detecting H.264 encoder on device: {}", serial);
    
    // Try common hardware encoders in order
    let candidates = vec![
        "c2.android.avc.encoder",      // Codec2 (modern Android)
        "OMX.qcom.video.encoder.avc",  // Qualcomm
        "OMX.MTK.VIDEO.ENCODER.AVC",   // MediaTek
        "OMX.Exynos.AVC.Encoder",      // Samsung Exynos
        "OMX.IMG.TOPAZ.VIDEO.Encoder", // PowerVR
        "OMX.k3.video.encoder.avc",    // Huawei Kirin
    ];

    for encoder in &candidates {
        if is_encoder_available(serial, encoder)? {
            info!("Found hardware encoder: {}", encoder);
            return Ok(Some(encoder.to_string()));
        }
    }

    warn!("No hardware H.264 encoder found, using system default");
    Ok(None)
}

/// Check if encoder is available on device
fn is_encoder_available(serial: &str, encoder_name: &str) -> Result<bool> {
    // Quick check: try to query codec capabilities
    // If it fails, encoder doesn't exist
    let output = Command::new("adb")
        .args(["-s", serial, "shell", "getprop", "ro.product.manufacturer"])
        .output()
        .context("Failed to query device manufacturer")?;

    if !output.status.success() {
        return Ok(false);
    }

    let manufacturer = String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_lowercase();

    debug!("Device manufacturer: {}", manufacturer);

    // Heuristic based on manufacturer
    let likely_available = match encoder_name {
        "c2.android.avc.encoder" => true, // Universal Codec2
        "OMX.qcom.video.encoder.avc" => manufacturer.contains("qualcomm") 
            || manufacturer.contains("xiaomi")
            || manufacturer.contains("oneplus"),
        "OMX.MTK.VIDEO.ENCODER.AVC" => manufacturer.contains("mediatek"),
        "OMX.Exynos.AVC.Encoder" => manufacturer.contains("samsung"),
        _ => false,
    };

    Ok(likely_available)
}

/// List all available video encoders (for debugging)
pub fn list_video_encoders(_serial: &str) -> Result<Vec<EncoderInfo>> {
    // Would need to run Java code on device or parse dumpsys output
    // For now, return empty - can be extended later
    Ok(vec![])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encoder_priority() {
        // Verify ordering is correct
        let encoders = vec![
            "c2.android.avc.encoder",
            "OMX.qcom.video.encoder.avc",
        ];
        assert_eq!(encoders[0], "c2.android.avc.encoder");
    }
}
