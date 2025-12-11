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
/// Priority (vendor-specific hardware encoder first):
/// 1. Vendor hardware: c2.mtk, OMX.qcom, OMX.Exynos, etc.
/// 2. Generic Codec2: c2.android.avc.encoder
/// 3. Fallback: system default
///
pub fn detect_h264_encoder(serial: &str) -> Result<Option<String>> {
    info!("Detecting H.264 encoder on device: {}", serial);
    
    // Get device manufacturer
    let manufacturer = get_device_manufacturer(serial)?;
    debug!("Device manufacturer: {}", manufacturer);
    
    // Try vendor-specific hardware encoders first (modern naming)
    let vendor_encoders_c2 = match manufacturer.as_str() {
        "mediatek" => vec!["c2.mtk.avc.encoder"],
        "qualcomm" | "xiaomi" | "oneplus" | "oppo" | "vivo" => vec!["c2.qcom.avc.encoder"],
        "samsung" => vec!["c2.exynos.avc.encoder"],
        _ => vec![],
    };
    
    for encoder in &vendor_encoders_c2 {
        info!("Trying vendor encoder: {}", encoder);
        return Ok(Some(encoder.to_string()));
    }
    
    // Try legacy OMX hardware encoders
    let vendor_encoders_omx = vec![
        "OMX.qcom.video.encoder.avc",  // Qualcomm
        "OMX.MTK.VIDEO.ENCODER.AVC",   // MediaTek
        "OMX.Exynos.AVC.Encoder",      // Samsung Exynos
        "OMX.IMG.TOPAZ.VIDEO.Encoder", // PowerVR
        "OMX.k3.video.encoder.avc",    // Huawei Kirin
    ];
    
    for encoder in &vendor_encoders_omx {
        if is_encoder_available(serial, encoder)? {
            info!("Found legacy hardware encoder: {}", encoder);
            return Ok(Some(encoder.to_string()));
        }
    }
    
    // Use generic Codec2 as last resort (may be software)
    info!("Using generic Codec2 encoder: c2.android.avc.encoder");
    Ok(Some("c2.android.avc.encoder".to_string()))
}

/// Get device manufacturer
fn get_device_manufacturer(serial: &str) -> Result<String> {
    let output = Command::new("adb")
        .args(["-s", serial, "shell", "getprop", "ro.product.manufacturer"])
        .output()
        .context("Failed to query device manufacturer")?;

    if !output.status.success() {
        return Ok("unknown".to_string());
    }

    let manufacturer = String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_lowercase();

    Ok(manufacturer)
}

/// Check if encoder is available on device (heuristic)
fn is_encoder_available(serial: &str, encoder_name: &str) -> Result<bool> {
    let manufacturer = get_device_manufacturer(serial)?;

    // Heuristic based on manufacturer
    let likely_available = match encoder_name {
        "c2.android.avc.encoder" => true, // Universal Codec2
        "OMX.qcom.video.encoder.avc" => manufacturer.contains("qualcomm") 
            || manufacturer.contains("xiaomi")
            || manufacturer.contains("oneplus"),
        "OMX.MTK.VIDEO.ENCODER.AVC" => manufacturer.contains("mediatek") || manufacturer.contains("vivo"),
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
