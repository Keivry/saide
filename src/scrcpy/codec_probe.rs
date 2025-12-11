//! Automatic Video Codec Options Compatibility Detection
//!
//! Probes device capabilities to find optimal low-latency configuration.

use {
    anyhow::{Context, Result},
    serde::{Deserialize, Serialize},
    std::{
        collections::HashMap,
        fs,
        path::PathBuf,
        process::Command,
        time::Duration,
    },
    tracing::{debug, info},
};

/// Candidate codec options to test (from most to least impactful)
const CODEC_OPTIONS: &[(&str, &str)] = &[
    ("profile", "66"),                          // Baseline Profile
    ("i-frame-interval", "2"),                  // Short GOP (high impact)
    ("latency", "0"),                           // Android 11+ low latency
    ("max-bframes", "0"),                       // Disable B-frames (Android 13+)
    ("priority", "0"),                          // Real-time priority
    ("prepend-sps-pps-to-idr-frames", "1"),     // Dynamic resolution
    ("intra-refresh-period", "60"),             // Periodic refresh
    ("bitrate-mode", "1"),                      // CBR
];

/// Device codec compatibility profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceProfile {
    /// Device serial number
    pub serial: String,

    /// Device model name
    pub model: String,

    /// SoC platform
    pub platform: String,

    /// Android version
    pub android_version: u32,

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
        let model = get_prop(serial, "ro.product.model")?;
        let platform = get_platform(serial)?;
        let android_version = get_android_version(serial)?;

        Ok(Self {
            serial: serial.to_string(),
            model,
            platform,
            android_version,
            supported_options: Vec::new(),
            optimal_config: None,
            tested_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Build optimal codec options string
    pub fn build_options_string(&self) -> Option<String> {
        if self.supported_options.is_empty() {
            return None;
        }

        let options: Vec<String> = CODEC_OPTIONS
            .iter()
            .filter(|(key, _)| self.supported_options.contains(&key.to_string()))
            .map(|(key, value)| format!("{}={}", key, value))
            .collect();

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

        let content = fs::read_to_string(&path).context("Failed to read profile database")?;
        serde_json::from_str(&content).context("Failed to parse profile database")
    }

    /// Save to config file
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).context("Failed to create config directory")?;
        }

        let content = serde_json::to_string_pretty(&self)?;
        fs::write(&path, content).context("Failed to write profile database")?;

        info!("Saved device profiles to {:?}", path);
        Ok(())
    }

    /// Get config file path
    fn config_path() -> Result<PathBuf> {
        let home = std::env::var("HOME").context("HOME not set")?;
        Ok(PathBuf::from(home)
            .join(".config")
            .join("saide")
            .join("device_profiles.json"))
    }

    /// Get profile for device
    pub fn get(&self, serial: &str) -> Option<&DeviceProfile> {
        self.profiles.get(serial)
    }

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

    // Android version-based filtering
    let candidate_options: Vec<_> = CODEC_OPTIONS
        .iter()
        .filter(|(key, _)| {
            match *key {
                "latency" if profile.android_version < 11 => {
                    debug!("Skipping 'latency' (requires Android 11+)");
                    false
                }
                "max-bframes" if profile.android_version < 13 => {
                    debug!("Skipping 'max-bframes' (requires Android 13+)");
                    false
                }
                _ => true,
            }
        })
        .collect();

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
        if test_codec_options(serial, server_jar, &options)? {
            info!("    ✅ Supported");
            profile.supported_options.push(key.to_string());
        } else {
            info!("    ❌ Not supported");
        }
    }

    // Build optimal config
    profile.optimal_config = profile.build_options_string();

    info!(
        "✅ Probe complete: {}/{} options supported",
        profile.supported_options.len(),
        candidate_options.len()
    );

    if let Some(ref config) = profile.optimal_config {
        info!("   Optimal config: {}", config);
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
fn test_codec_options(serial: &str, _server_jar: &str, options: &str) -> Result<bool> {
    // Start scrcpy server with test options
    let mut cmd = Command::new("adb");
    cmd.args(["-s", serial, "shell"])
        .arg(format!("CLASSPATH={}", "/data/local/tmp/scrcpy-server.jar"))
        .arg("app_process")
        .arg("/")
        .arg("com.genymobile.scrcpy.Server")
        .arg("3.3.3")
        .arg(format!("scid={:08x}", rand::random::<u32>()))
        .arg("log_level=error") // Suppress logs
        .arg("video_bit_rate=4000000")
        .arg("max_size=800") // Low resolution for fast testing
        .arg("max_fps=30")
        .arg(format!("video_codec_options={}", options))
        .arg("audio=false")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    debug!("Test command: {:?}", cmd);

    let mut child = cmd.spawn().context("Failed to spawn test server")?;

    // Wait up to 2 seconds for encoder initialization
    std::thread::sleep(Duration::from_millis(2000));

    // Check if still running (success) or crashed (failure)
    let success = match child.try_wait()? {
        None => {
            // Still running = encoder initialized successfully
            child.kill().ok();
            true
        }
        Some(status) => {
            // Exited = encoder failed
            debug!("Server exited with status: {}", status);
            false
        }
    };

    // Cleanup
    child.wait().ok();

    Ok(success)
}

/// Get device platform
fn get_platform(serial: &str) -> Result<String> {
    let platform = get_prop(serial, "ro.board.platform")?;
    if !platform.is_empty() && platform != "unknown" {
        return Ok(platform);
    }

    get_prop(serial, "ro.hardware")
}

/// Get Android version (SDK int)
fn get_android_version(serial: &str) -> Result<u32> {
    let version_str = get_prop(serial, "ro.build.version.sdk")?;
    version_str
        .trim()
        .parse()
        .context("Failed to parse Android version")
}

/// Get Android system property
fn get_prop(serial: &str, prop_name: &str) -> Result<String> {
    let output = Command::new("adb")
        .args(["-s", serial, "shell", "getprop", prop_name])
        .output()
        .context(format!("Failed to query {}", prop_name))?;

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_build_options() {
        let mut profile = DeviceProfile {
            serial: "test".to_string(),
            model: "Test".to_string(),
            platform: "test".to_string(),
            android_version: 14,
            supported_options: vec!["i-frame-interval".to_string(), "latency".to_string()],
            optimal_config: None,
            tested_at: "2025-01-01T00:00:00Z".to_string(),
        };

        let options = profile.build_options_string().unwrap();
        assert!(options.contains("i-frame-interval=2"));
        assert!(options.contains("latency=0"));
    }
}
