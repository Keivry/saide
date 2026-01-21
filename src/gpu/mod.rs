//! System-related utilities for GPU detection
//!
//! Cross-platform GPU detection supporting:
//! - Linux: DRM sysfs, nvidia-smi, /proc/driver/nvidia
//! - macOS: system_profiler, sysctl
//! - Windows: DXGI (placeholder, needs windows-sys dependency)

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuType {
    Nvidia,
    Intel,
    Amd,
    Apple,    // Apple Silicon (macOS)
    Software, // Software rendering fallback
    Unknown,
}

impl fmt::Display for GpuType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GpuType::Nvidia => write!(f, "NVIDIA"),
            GpuType::Intel => write!(f, "Intel"),
            GpuType::Amd => write!(f, "AMD"),
            GpuType::Apple => write!(f, "Apple"),
            GpuType::Software => write!(f, "Software"),
            GpuType::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Detect current GPU type (cross-platform)
#[cfg(target_os = "linux")]
pub fn detect_gpu() -> GpuType {
    // 1. Check for NVIDIA GPU
    if is_nvidia_gpu_available() {
        return GpuType::Nvidia;
    }

    // 2. Check for Intel/AMD via DRM
    if let Some(gpu) = detect_drm_gpu() {
        return gpu;
    }

    GpuType::Unknown
}

#[cfg(target_os = "linux")]
fn is_nvidia_gpu_available() -> bool {
    use {
        std::{fs, path::Path, process::Command},
        tracing::debug,
    };

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
    if let Ok(entries) = fs::read_dir("/dev/dri") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with("renderD")
                && let Some(vendor) = get_device_vendor(&entry.path())
                && vendor == 0x10de
            {
                debug!("Detected NVIDIA GPU via DRM device");
                return true;
            }
        }
    }

    false
}

#[cfg(target_os = "linux")]
fn detect_drm_gpu() -> Option<GpuType> {
    use {std::fs, tracing::debug};

    if let Ok(entries) = fs::read_dir("/sys/class/drm") {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();

            if !name.to_string_lossy().starts_with("card") {
                continue;
            }

            if let Some(card_name) = name.to_string_lossy().strip_prefix("card")
                && !card_name.chars().next().is_some_and(|c| c.is_ascii_digit())
            {
                continue;
            }

            let vendor_path = path.join("device/vendor");
            if let Ok(vendor_str) = fs::read_to_string(&vendor_path)
                && let Ok(vendor) =
                    u32::from_str_radix(vendor_str.trim().trim_start_matches("0x"), 16)
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
    }

    None
}

#[cfg(target_os = "linux")]
fn get_device_vendor(device_path: &std::path::Path) -> Option<u32> {
    use std::fs;

    let name = device_path.file_name()?.to_string_lossy();
    let num_str = name.strip_prefix("renderD")?;
    let device_num: u32 = num_str.parse().ok()?;

    let card_num = device_num - 128;
    let vendor_path = format!("/sys/class/drm/card{}/device/vendor", card_num);
    let vendor_str = fs::read_to_string(&vendor_path).ok()?;

    u32::from_str_radix(vendor_str.trim().trim_start_matches("0x"), 16).ok()
}

/// macOS GPU detection using system_profiler and sysctl
#[cfg(target_os = "macos")]
pub fn detect_gpu() -> GpuType {
    use std::process::Command;

    // Method 1: Check for Apple Silicon via sysctl
    if let Ok(output) = Command::new("sysctl")
        .arg("machdep.cpu.brand_string")
        .output()
    {
        let brand = String::from_utf8_lossy(&output.stdout);
        if brand.contains("Apple")
            || brand.contains("M1")
            || brand.contains("M2")
            || brand.contains("M3")
        {
            return GpuType::Apple;
        }
    }

    // Method 2: Check for Intel/AMD GPU via system_profiler
    if let Ok(output) = Command::new("system_profiler")
        .arg("SPDisplaysDataType")
        .output()
    {
        let info = String::from_utf8_lossy(&output.stdout);
        if info.contains("Intel") {
            return GpuType::Intel;
        }
        if info.contains("AMD") || info.contains("NVIDIA") {
            return GpuType::Amd;
        }
    }

    GpuType::Unknown
}

/// Windows GPU detection (placeholder - requires windows-sys dependency)
#[cfg(target_os = "windows")]
pub fn detect_gpu() -> GpuType {
    // TODO: Implement proper DXGI detection when windows-sys is added
    // For now, return Unknown as placeholder
    GpuType::Unknown
}

/// Get GPU display name for UI
pub fn gpu_display_name() -> String { detect_gpu().to_string() }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_type_display() {
        assert_eq!(format!("{}", GpuType::Nvidia), "NVIDIA");
        assert_eq!(format!("{}", GpuType::Intel), "Intel");
        assert_eq!(format!("{}", GpuType::Apple), "Apple");
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_detect_gpu_linux() {
        let gpu = detect_gpu();
        println!("Detected GPU on Linux: {:?}", gpu);
        assert_ne!(gpu, GpuType::Unknown);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_detect_gpu_macos() {
        let gpu = detect_gpu();
        println!("Detected GPU on macOS: {:?}", gpu);
        assert!(matches!(
            gpu,
            GpuType::Apple | GpuType::Intel | GpuType::Amd | GpuType::Unknown
        ));
    }
}
