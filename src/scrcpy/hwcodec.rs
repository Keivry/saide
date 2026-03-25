// SPDX-License-Identifier: MIT OR Apache-2.0

use {crate::error::Result, adbshell::AdbShell, tracing::info};

/// Detect the preferred vendor H.264 hardware encoder for the given device.
///
/// Queries the device's SoC platform identifier via ADB and matches it against
/// known vendor encoder names (Qualcomm, MediaTek, Samsung Exynos, HiSilicon
/// Kirin).  Returns `None` when no vendor match is found, in which case the
/// caller should fall back to the Android software encoder.
pub fn detect_h264_encoder(serial: &str) -> Result<Option<String>> {
    info!("Detecting H.264 encoder on device: {}", serial);
    let platform = get_soc_platform(serial)?;
    info!("Device SoC platform: {}", platform);
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

fn get_soc_platform(serial: &str) -> Result<String> {
    let platform = AdbShell::get_prop(serial, "ro.board.platform")?;
    if !platform.is_empty() && platform != "unknown" {
        return Ok(platform);
    }

    let hardware = AdbShell::get_prop(serial, "ro.hardware")?;
    if !hardware.is_empty() && hardware != "unknown" {
        return Ok(hardware);
    }

    Ok("unknown".to_string())
}

fn match_encoder_for_platform(platform: &str) -> Option<String> {
    let platform_lower = platform.to_lowercase();

    if platform_lower.starts_with("mt") || platform_lower.contains("dimensity") {
        return Some("c2.mtk.avc.encoder".to_string());
    }

    if platform_lower.starts_with("msm")
        || platform_lower.starts_with("sm")
        || platform_lower.starts_with("sdm")
        || platform_lower.starts_with("qsm")
        || platform_lower.contains("lahaina")
        || platform_lower.contains("taro")
        || platform_lower.contains("kalama")
        || platform_lower.contains("pineapple")
    {
        return Some("c2.qcom.avc.encoder".to_string());
    }

    if platform_lower.starts_with("exynos") || platform_lower.starts_with("s5e") {
        return Some("c2.exynos.avc.encoder".to_string());
    }

    if platform_lower.starts_with("kirin") || platform_lower.starts_with("hi") {
        return Some("OMX.hisi.video.encoder.avc".to_string());
    }

    None
}
