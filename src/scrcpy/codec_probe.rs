//! Automatic Video Codec Options Compatibility Detection
//!
//! Probes device capabilities to find optimal low-latency configuration.

use {
    super::server::ServerParams,
    crate::{
        GpuType,
        controller::AdbShell,
        detect_gpu,
        error::{IoError, Result, SAideError},
    },
    serde::{Deserialize, Serialize},
    std::{collections::HashMap, fs, path::PathBuf},
    tracing::{debug, info},
};

/// Candidate codec options to test (from most to least impactful)
///
/// NOTE: "profile" value is GPU-dependent and set dynamically:
/// - VAAPI (Intel/AMD): profile=66 (Baseline Profile, standard H.264)
/// - NVDEC (NVIDIA): profile=65536 (NVDEC-specific enum value)
const CODEC_OPTIONS_BASE: &[(&str, &str)] = &[
    ("i-frame-interval", "2"),              // Short GOP (high impact)
    ("latency", "0"),                       // Android 11+ low latency
    ("max-bframes", "0"),                   // Disable B-frames (Android 13+)
    ("priority", "0"),                      // Real-time priority
    ("prepend-sps-pps-to-idr-frames", "1"), // Dynamic resolution
    ("intra-refresh-period", "60"),         // Periodic refresh
    ("bitrate-mode", "1"),                  // CBR
];

/// Get profile value based on GPU type
fn get_profile_for_gpu(gpu_type: GpuType) -> (&'static str, &'static str) {
    match gpu_type {
        GpuType::Nvidia => ("profile", "65536"), // NVDEC enum value
        GpuType::Intel | GpuType::Amd => ("profile", "66"), // Baseline Profile (VAAPI)
        GpuType::Apple => ("profile", "66"),     // VideoToolbox uses Baseline
        GpuType::Software => ("profile", "66"),  // Software decoder uses Baseline
        GpuType::Unknown => ("profile", "66"),   // Fallback to standard Baseline
    }
}

/// Device codec compatibility profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceProfile {
    /// Device serial
    pub serial: String,

    /// Device model name
    pub model: String,

    /// SoC platform
    pub platform: String,

    /// Android version
    pub android_version: u32,

    /// Detected hardware encoder (if available)
    pub video_encoder: Option<String>,

    /// Supported codec option keys
    pub supported_options: Vec<String>,

    /// Optimal configuration string
    pub optimal_config: Option<String>,

    /// Last tested timestamp
    pub tested_at: String,
}

impl DeviceProfile {
    /// Create profile from device info
    pub fn new(serial: &str) -> Result<Self> {
        let model = AdbShell::get_prop(serial, "ro.product.model")?;
        let platform = AdbShell::get_platform(serial)?;
        let android_version = AdbShell::get_android_version(serial)?;

        Ok(Self {
            serial: serial.to_string(),
            model,
            platform,
            android_version,
            video_encoder: None, // Will be set during probing
            supported_options: Vec::new(),
            optimal_config: None,
            tested_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Build optimal codec options string
    pub fn build_options_string(&self, gpu_type: GpuType) -> Option<String> {
        if self.supported_options.is_empty() {
            return None;
        }

        let profile_option = get_profile_for_gpu(gpu_type);

        let mut options: Vec<String> = Vec::new();

        // Add profile first if supported
        if self.supported_options.contains(&"profile".to_string()) {
            options.push(format!("{}={}", profile_option.0, profile_option.1));
        }

        // Add other options
        for (key, value) in CODEC_OPTIONS_BASE.iter() {
            if self.supported_options.contains(&key.to_string()) {
                options.push(format!("{}={}", key, value));
            }
        }

        if options.is_empty() {
            None
        } else {
            Some(options.join(","))
        }
    }
}

/// Profile database
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProfileDatabase {
    profiles: HashMap<String, DeviceProfile>,
}

impl ProfileDatabase {
    /// Load from config file
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&path).map_err(|e| {
            SAideError::IoError(IoError::new(e).with_message("Failed to read profile database"))
        })?;
        let config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Save to config file
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(&self)?;
        fs::write(&path, content)?;

        info!("Saved device profiles to {:?}", path);
        Ok(())
    }

    /// Get config file path
    fn config_path() -> Result<PathBuf> {
        let home = std::env::var("HOME")?;
        Ok(PathBuf::from(home)
            .join(".config")
            .join("saide")
            .join("device_profiles.toml"))
    }

    /// Get profile for device
    pub fn get(&self, serial: &str) -> Option<&DeviceProfile> { self.profiles.get(serial) }

    /// Insert or update profile
    pub fn insert(&mut self, profile: DeviceProfile) {
        self.profiles.insert(profile.serial.clone(), profile);
    }
}

/// Probe device codec compatibility
///
/// Returns optimal codec options string (or None if no options supported)
pub fn probe_device(serial: &str, server_jar: &str) -> Result<Option<String>> {
    info!("🔍 Probing codec compatibility for device: {}", serial);

    let mut profile = DeviceProfile::new(serial)?;
    info!(
        "Device: {} ({}), Android {}",
        profile.model, profile.platform, profile.android_version
    );

    // Detect hardware encoder
    profile.video_encoder = super::hwcodec::detect_h264_encoder(serial)?;
    if let Some(ref encoder) = profile.video_encoder {
        info!("Detected hardware encoder: {}", encoder);
    } else {
        info!("Using system default encoder");
    }

    // Detect GPU type for profile selection
    let gpu_type = detect_gpu();
    let profile_option = get_profile_for_gpu(gpu_type);
    info!(
        "Detected GPU: {:?}, using {}={}",
        gpu_type, profile_option.0, profile_option.1
    );

    // Build candidate options list (profile + base options)
    let mut candidate_options: Vec<(&str, &str)> = vec![profile_option];
    candidate_options.extend(
        CODEC_OPTIONS_BASE
            .iter()
            .filter(|(key, _)| match *key {
                "latency" if profile.android_version < 11 => {
                    debug!("Skipping 'latency' (requires Android 11+)");
                    false
                }
                "max-bframes" if profile.android_version < 13 => {
                    debug!("Skipping 'max-bframes' (requires Android 13+)");
                    false
                }
                _ => true,
            })
            .copied(),
    );

    info!("Testing {} codec options...", candidate_options.len());

    // Test each option individually
    for (i, (key, value)) in candidate_options.iter().enumerate() {
        info!(
            "  [{}/{}] Testing {}={}...",
            i + 1,
            candidate_options.len(),
            key,
            value
        );

        let options = format!("{}={}", key, value);
        if test_codec_options(
            serial,
            server_jar,
            &options,
            profile.video_encoder.as_deref(),
        )? {
            info!("    ✅ Supported");
            profile.supported_options.push(key.to_string());
        } else {
            info!("    ❌ Not supported");
        }
    }

    // Build optimal config (GPU-aware)
    profile.optimal_config = profile.build_options_string(gpu_type);

    // Validate combined options work together
    if let Some(ref combined_config) = profile.optimal_config {
        info!("🔄 Validating combined configuration...");
        info!("   Testing: {}", combined_config);

        if test_codec_options(
            serial,
            server_jar,
            combined_config,
            profile.video_encoder.as_deref(),
        )? {
            info!("   ✅ Combined config works!");
        } else {
            info!("   ❌ Combined config failed, falling back to None");
            profile.optimal_config = None;
            profile.supported_options.clear();
        }
    }

    info!(
        "✅ Probe complete: {}/{} options supported",
        profile.supported_options.len(),
        candidate_options.len()
    );

    if let Some(ref config) = profile.optimal_config {
        info!("   Final config: {}", config);
    } else {
        info!("   No options supported, using defaults");
    }

    // Save to database
    let mut db = ProfileDatabase::load()?;
    db.insert(profile.clone());
    db.save()?;

    Ok(profile.optimal_config)
}

/// Test if codec options work on device
///
/// Returns true if encoder can be configured successfully
fn test_codec_options(
    serial: &str,
    server_jar: &str,
    options: &str,
    video_encoder: Option<&str>,
) -> Result<bool> {
    use crate::scrcpy::connection::ScrcpyConnection;

    // Create params with test options
    let params = ServerParams {
        video: true,
        video_codec: "h264".to_string(),
        video_encoder: video_encoder.map(|s| s.to_string()),
        video_bit_rate: 4_000_000,
        max_size: 800,
        max_fps: 30,
        audio: false,
        control: false, // Don't need control for testing
        send_device_meta: false,
        send_codec_meta: true,
        send_frame_meta: true,
        video_codec_options: Some(options.to_string()),
        ..Default::default()
    };

    if let Some(encoder) = video_encoder {
        info!(
            "  Testing: video_encoder={}, video_codec_options={}",
            encoder, options
        );
    } else {
        info!("  Testing: video_codec_options={}", options);
    }

    // Try to connect and read a few packets
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let result = rt.block_on(async {
        let mut conn = match ScrcpyConnection::connect(serial, server_jar, "127.0.0.1", params) {
            Ok(c) => c,
            Err(e) => {
                info!("  Connection failed: {}", e);
                return false;
            }
        };

        // Try to read at least one video packet
        match conn.read_video_packet() {
            Ok(_packet) => {
                info!("  ✅ Successfully read video packet");
                conn.shutdown().ok();
                true
            }
            Err(e) => {
                info!("  Failed to read packet: {}", e);
                conn.shutdown().ok();
                false
            }
        }
    });

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_build_options_vaapi() {
        let profile = DeviceProfile {
            serial: "test".to_string(),
            model: "Test".to_string(),
            platform: "test".to_string(),
            android_version: 14,
            video_encoder: Some("c2.test.avc.encoder".to_string()),
            supported_options: vec![
                "profile".to_string(),
                "i-frame-interval".to_string(),
                "latency".to_string(),
            ],
            optimal_config: None,
            tested_at: "2025-01-01T00:00:00Z".to_string(),
        };

        let options = profile.build_options_string(GpuType::Intel).unwrap();
        assert!(options.contains("profile=66"));
        assert!(options.contains("i-frame-interval=2"));
        assert!(options.contains("latency=0"));
    }

    #[test]
    fn test_profile_build_options_nvdec() {
        let profile = DeviceProfile {
            serial: "test".to_string(),
            model: "Test".to_string(),
            platform: "test".to_string(),
            android_version: 14,
            video_encoder: Some("c2.test.avc.encoder".to_string()),
            supported_options: vec!["profile".to_string(), "i-frame-interval".to_string()],
            optimal_config: None,
            tested_at: "2025-01-01T00:00:00Z".to_string(),
        };

        let options = profile.build_options_string(GpuType::Nvidia).unwrap();
        assert!(options.contains("profile=65536"));
        assert!(options.contains("i-frame-interval=2"));
    }
}
