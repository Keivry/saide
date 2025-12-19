//! System-related utilities for GPU detection

use {
    std::{fs, path::Path, process::Command},
    tracing::debug,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuType {
    Nvidia,
    Intel,
    Amd,
    Unknown,
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
    if let Ok(output) = Command::new("nvidia-smi")
        .arg("--query-gpu=name")
        .arg("--format=csv,noheader")
        .output()
        && output.status.success()
        && !output.stdout.is_empty()
    {
        debug!("Detected NVIDIA GPU via nvidia-smi");
        return true;
    }

    // Method 3: Check for NVIDIA render devices
    for entry in fs::read_dir("/dev/dri")
        .ok()
        .into_iter()
        .flatten()
        .flatten()
    {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with("renderD") {
            // Check if this is NVIDIA device
            if let Some(vendor) = get_device_vendor(&entry.path())
                && vendor == 0x10de
            {
                // NVIDIA vendor ID
                debug!("Detected NVIDIA GPU via DRM device");
                return true;
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

        // Skip card devices that are not physical GPU cards.
        // The intent: only accept cardN where N starts with a digit (e.g. "card0",
        // "card1-HDMI-A-1"). If the suffix after "card" is empty or starts with a
        // non-digit, skip it.
        if let Some(card_name) = name.to_string_lossy().strip_prefix("card") {
            // Check first character exists and is a digit; if not, skip.
            if !card_name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                continue;
            }
        }

        let vendor_path = path.join("device/vendor");
        if let Ok(vendor_str) = fs::read_to_string(&vendor_path)
            && let Ok(vendor) = u32::from_str_radix(vendor_str.trim().trim_start_matches("0x"), 16)
        {
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
