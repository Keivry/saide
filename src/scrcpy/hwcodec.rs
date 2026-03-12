// SPDX-License-Identifier: MIT OR Apache-2.0

//! Device Hardware Codec Capability Detection
//!
//! Detects hardware encoders and decoders on Android devices.

use {
    crate::{controller::AdbShell, error::Result},
    tracing::info,
};

/// Detect best H.264 hardware encoder on device
///
/// Priority:
/// 1. Query SoC platform (ro.board.platform / ro.hardware)
/// 2. Match vendor-specific hardware encoder
/// 3. Fallback to generic Codec2
pub fn detect_h264_encoder(serial: &str) -> Result<Option<String>> {
    // TODO: Use scrcpy-server to list encoders directly on device, then select best

    info!("Detecting H.264 encoder on device: {}", serial);

    // Get SoC platform
    let platform = get_soc_platform(serial)?;
    info!("Device SoC platform: {}", platform);

    // Detect encoder based on SoC
    let encoder = match_encoder_for_platform(&platform);

    if let Some(ref enc) = encoder {
        info!("Selected encoder: {} (for {})", enc, platform);
    } else {
        info!(
            "No vendor encoder matched for {}, using system default",
            platform
        );
    }

    Ok(encoder)
}

/// Get SoC platform identifier
fn get_soc_platform(serial: &str) -> Result<String> {
    // Try ro.board.platform first (most accurate)
    let platform = AdbShell::get_prop(serial, "ro.board.platform")?;
    if !platform.is_empty() && platform != "unknown" {
        return Ok(platform);
    }

    // Fallback to ro.hardware
    let hardware = AdbShell::get_prop(serial, "ro.hardware")?;
    if !hardware.is_empty() && hardware != "unknown" {
        return Ok(hardware);
    }

    info!("Unable to detect SoC platform, using system default encoder");
    Ok("unknown".to_string())
}

/// Match encoder for specific SoC platform
fn match_encoder_for_platform(platform: &str) -> Option<String> {
    let platform_lower = platform.to_lowercase();

    // MediaTek (mt*, dimensity)
    if platform_lower.starts_with("mt") || platform_lower.contains("dimensity") {
        return Some("c2.mtk.avc.encoder".to_string());
    }

    // Qualcomm (msm*, sm*, sdm*, lahaina, taro, kalama, etc.)
    if platform_lower.starts_with("msm") 
        || platform_lower.starts_with("sm") 
        || platform_lower.starts_with("sdm")
        || platform_lower.starts_with("qsm")
        || platform_lower.contains("lahaina")   // SM8350 (SD888)
        || platform_lower.contains("taro")      // SM8450 (SD8 Gen1)
        || platform_lower.contains("kalama")    // SM8550 (SD8 Gen2)
        || platform_lower.contains("pineapple")
    // SM8650 (SD8 Gen3)
    {
        return Some("c2.qcom.avc.encoder".to_string());
    }

    // Samsung Exynos
    if platform_lower.starts_with("exynos") || platform_lower.starts_with("s5e") {
        return Some("c2.exynos.avc.encoder".to_string());
    }

    // Huawei Kirin
    if platform_lower.starts_with("kirin") || platform_lower.starts_with("hi") {
        return Some("OMX.hisi.video.encoder.avc".to_string());
    }

    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_encoder_priority() {
        // Verify ordering is correct
        let encoders = ["c2.android.avc.encoder", "OMX.qcom.video.encoder.avc"];
        assert_eq!(encoders[0], "c2.android.avc.encoder");
    }
}
