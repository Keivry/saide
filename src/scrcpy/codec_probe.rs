//! Automatic Video Codec Options Compatibility Detection
//!
//! Probes device capabilities to find optimal low-latency configuration.
use {
    super::server::ServerParams,
    crate::{
        controller::AdbShell,
        decoder::{AutoDecoder, DecoderPreference, VideoDecoder, extract_resolution_from_stream},
        error::{IoError, Result, SAideError},
    },
    crossbeam_channel::Sender,
    serde::{Deserialize, Serialize},
    std::{collections::HashMap, fs, path::PathBuf, time::Duration},
    tracing::{debug, info},
};

#[derive(Debug, Clone)]
pub enum ProbeStep {
    DetectingDevice,
    DetectingEncoder,
    TestingProfile {
        index: usize,
        total: usize,
        name: String,
    },
    TestingOption {
        index: usize,
        total: usize,
        key: String,
    },
    Validating,
    Done(std::result::Result<Option<String>, String>),
}

const CODEC_OPTIONS_BASE: &[(&str, &str)] = &[
    ("i-frame-interval", "2"),
    ("latency", "0"),
    ("max-bframes", "0"),
    ("priority", "0"),
    ("prepend-sps-pps-to-idr-frames", "1"),
    ("intra-refresh-period", "60"),
    ("bitrate-mode", "1"),
];

const CODEC_PROFILES: &[(&str, &str)] =
    &[("profile", "65536"), ("profile", "1"), ("profile", "66")];
const PROBE_PACKET_TIMEOUT: Duration = Duration::from_secs(3);
const VALIDATION_PACKET_LIMIT: usize = 12;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncoderProfile {
    pub serial: String,
    pub model: String,
    pub platform: String,
    pub android_version: u32,
    pub video_encoder: Option<String>,
    pub supported_options: Vec<String>,
    pub supported_profile: Option<String>,
    pub optimal_config: Option<String>,
    pub tested_at: String,
}

impl EncoderProfile {
    pub fn new(serial: &str) -> Result<Self> {
        let model = AdbShell::get_prop(serial, "ro.product.model")?;
        let platform = AdbShell::get_platform(serial)?;
        let android_version = AdbShell::get_android_version(serial)?;

        Ok(Self {
            serial: serial.to_string(),
            model,
            platform,
            android_version,
            video_encoder: None,
            supported_options: Vec::new(),
            supported_profile: None,
            optimal_config: None,
            tested_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    pub fn build_options_string(&self) -> Option<String> {
        build_options_string_for(self.supported_profile.as_deref(), &self.supported_options)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EndToEndValidation {
    DeviceRejected,
    HostRejected,
    HostAccepted(DecoderPreference),
}

type FinalValidationResult = Option<(Option<&'static str>, Vec<String>, DecoderPreference)>;

fn build_options_string_for(
    supported_profile: Option<&str>,
    supported_options: &[String],
) -> Option<String> {
    let mut options: Vec<String> = Vec::new();

    if let Some(profile_value) = supported_profile {
        options.push(format!("profile={}", profile_value));
    }

    for (key, value) in CODEC_OPTIONS_BASE.iter() {
        if supported_options.iter().any(|supported| supported == key) {
            options.push(format!("{}={}", key, value));
        }
    }

    if options.is_empty() {
        None
    } else {
        Some(options.join(","))
    }
}

fn profile_fallbacks(selected_profile: Option<&str>) -> Vec<Option<&'static str>> {
    match selected_profile {
        Some(selected) => CODEC_PROFILES
            .iter()
            .skip_while(|(_, value)| *value != selected)
            .map(|(_, value)| Some(*value))
            .collect(),
        None => vec![None],
    }
}

fn trim_option_sets(supported_options: &[String]) -> Vec<Vec<String>> {
    (0..=supported_options.len())
        .rev()
        .map(|len| supported_options[..len].to_vec())
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EncoderProfileDatabase {
    profiles: HashMap<String, EncoderProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecoderProfile {
    pub serial: String,
    pub validated_decoder: String,
    pub encoder_fingerprint: String,
    pub tested_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DecoderProfileDatabase {
    profiles: HashMap<String, DecoderProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct LegacyProfileDatabase {
    profiles: HashMap<String, LegacyDeviceProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LegacyDeviceProfile {
    serial: String,
    model: String,
    platform: String,
    android_version: u32,
    video_encoder: Option<String>,
    validated_decoder: Option<String>,
    supported_options: Vec<String>,
    supported_profile: Option<String>,
    optimal_config: Option<String>,
    tested_at: String,
}

pub fn encoder_fingerprint(
    video_encoder: Option<&str>,
    optimal_config: Option<&str>,
) -> Option<String> {
    match (video_encoder, optimal_config) {
        (None, None) => None,
        _ => Some(format!(
            "encoder={}|options={}",
            video_encoder.unwrap_or_default(),
            optimal_config.unwrap_or_default()
        )),
    }
}

impl EncoderProfileDatabase {
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Self::load_legacy();
        }

        let content = fs::read_to_string(&path).map_err(|e| {
            SAideError::IoError(
                IoError::new(e).with_message("Failed to read encoder profile database"),
            )
        })?;
        let config = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(&self)?;
        fs::write(&path, content)?;

        info!("Saved encoder profiles to {:?}", path);
        Ok(())
    }

    fn config_path() -> Result<PathBuf> { profile_path("encoder_profile.toml") }

    fn legacy_path() -> Result<PathBuf> { profile_path("device_profiles.toml") }

    fn load_legacy() -> Result<Self> {
        let path = Self::legacy_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&path).map_err(|e| {
            SAideError::IoError(
                IoError::new(e).with_message("Failed to read legacy profile database"),
            )
        })?;
        let legacy: LegacyProfileDatabase = toml::from_str(&content)?;
        Ok(Self {
            profiles: legacy
                .profiles
                .into_iter()
                .map(|(serial, profile)| {
                    (
                        serial,
                        EncoderProfile {
                            serial: profile.serial,
                            model: profile.model,
                            platform: profile.platform,
                            android_version: profile.android_version,
                            video_encoder: profile.video_encoder,
                            supported_options: profile.supported_options,
                            supported_profile: profile.supported_profile,
                            optimal_config: profile.optimal_config,
                            tested_at: profile.tested_at,
                        },
                    )
                })
                .collect(),
        })
    }

    pub fn get(&self, serial: &str) -> Option<&EncoderProfile> { self.profiles.get(serial) }

    pub fn insert(&mut self, profile: EncoderProfile) {
        self.profiles.insert(profile.serial.clone(), profile);
    }
}

impl DecoderProfileDatabase {
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Self::load_legacy();
        }

        let content = fs::read_to_string(&path).map_err(|e| {
            SAideError::IoError(
                IoError::new(e).with_message("Failed to read decoder profile database"),
            )
        })?;
        let config = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(&self)?;
        fs::write(&path, content)?;

        info!("Saved decoder profiles to {:?}", path);
        Ok(())
    }

    fn config_path() -> Result<PathBuf> { profile_path("decoder_profile.toml") }

    fn legacy_path() -> Result<PathBuf> { profile_path("device_profiles.toml") }

    fn load_legacy() -> Result<Self> {
        let path = Self::legacy_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&path).map_err(|e| {
            SAideError::IoError(
                IoError::new(e).with_message("Failed to read legacy profile database"),
            )
        })?;
        let legacy: LegacyProfileDatabase = toml::from_str(&content)?;
        Ok(Self {
            profiles: legacy
                .profiles
                .into_iter()
                .filter_map(|(serial, profile)| {
                    profile.validated_decoder.map(|validated_decoder| {
                        (
                            serial,
                            DecoderProfile {
                                serial: profile.serial,
                                validated_decoder,
                                encoder_fingerprint: encoder_fingerprint(
                                    profile.video_encoder.as_deref(),
                                    profile.optimal_config.as_deref(),
                                )
                                .unwrap_or_default(),
                                tested_at: profile.tested_at,
                            },
                        )
                    })
                })
                .collect(),
        })
    }

    pub fn get(&self, serial: &str) -> Option<&DecoderProfile> { self.profiles.get(serial) }

    pub fn insert(&mut self, profile: DecoderProfile) {
        self.profiles.insert(profile.serial.clone(), profile);
    }

    pub fn remove(&mut self, serial: &str) { self.profiles.remove(serial); }
}

fn profile_path(file_name: &str) -> Result<PathBuf> {
    use crate::constant::{config_dir, fallback_data_path};
    config_dir()
        .and_then(|p: PathBuf| p.parent().map(|parent| parent.join(file_name)))
        .or_else(|| Some(fallback_data_path().join(file_name)))
        .ok_or_else(|| {
            SAideError::IoError(IoError::new_with_message("Unable to determine config path"))
        })
}

pub fn probe_device(
    serial: &str,
    server_jar: &str,
    progress_tx: Option<&Sender<ProbeStep>>,
) -> Result<Option<String>> {
    info!("🔍 Probing codec compatibility for device: {}", serial);

    let send = |step: ProbeStep| {
        if let Some(tx) = progress_tx {
            let _ = tx.send(step);
        }
    };

    send(ProbeStep::DetectingDevice);
    let mut profile = EncoderProfile::new(serial)?;
    info!(
        "Device: {} ({}), Android {}",
        profile.model, profile.platform, profile.android_version
    );

    send(ProbeStep::DetectingEncoder);
    profile.video_encoder = super::hwcodec::detect_h264_encoder(serial)?;
    if let Some(ref encoder) = profile.video_encoder {
        info!("Detected hardware encoder: {}", encoder);
    } else {
        info!("Using system default encoder");
    }

    info!("Starting end-to-end profile testing (65536 → 1 → 66)...");

    let total_profiles = CODEC_PROFILES.len();
    for (i, (key, value)) in CODEC_PROFILES.iter().enumerate() {
        send(ProbeStep::TestingProfile {
            index: i + 1,
            total: total_profiles,
            name: format!("{}={}", key, value),
        });
        info!("Testing {}={}...", key, value);
        let options = format!("{}={}", key, value);
        match validate_end_to_end(
            serial,
            server_jar,
            &options,
            profile.video_encoder.as_deref(),
        )? {
            EndToEndValidation::HostAccepted(decoder) => {
                info!(
                    "✅ Profile {}={} passed end-to-end validation via {}",
                    key,
                    value,
                    decoder.profile_name()
                );
                profile.supported_profile = Some(value.to_string());
                break;
            }
            EndToEndValidation::HostRejected => {
                info!(
                    "⚠️ Profile {}={} streams on device but failed host decoder validation",
                    key, value
                );
            }
            EndToEndValidation::DeviceRejected => {
                info!("❌ Profile {}={} not supported on device", key, value);
            }
        }
    }

    let candidate_options: Vec<(&str, &str)> = CODEC_OPTIONS_BASE
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
        .copied()
        .collect();

    info!("Testing {} codec options...", candidate_options.len());

    let total_options = candidate_options.len();
    for (i, (key, value)) in candidate_options.iter().enumerate() {
        send(ProbeStep::TestingOption {
            index: i + 1,
            total: total_options,
            key: key.to_string(),
        });
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

    profile.optimal_config = profile.build_options_string();

    if let Some(ref combined_config) = profile.optimal_config {
        send(ProbeStep::Validating);
        info!("🔄 Validating combined configuration...");
        info!("   Testing: {}", combined_config);

        if let Some((validated_profile, validated_options, decoder)) = validate_final_config(
            serial,
            server_jar,
            profile.video_encoder.as_deref(),
            profile.supported_profile.as_deref(),
            &profile.supported_options,
        )? {
            profile.supported_profile = validated_profile.map(str::to_string);
            profile.supported_options = validated_options;
            profile.optimal_config = build_options_string_for(
                profile.supported_profile.as_deref(),
                &profile.supported_options,
            );

            if let Some(ref final_config) = profile.optimal_config {
                info!("   ✅ Final end-to-end config: {}", final_config);
            }

            info!("   ✅ Validated host decoder: {}", decoder.profile_name());
            save_validated_decoder(
                serial,
                decoder,
                profile.video_encoder.as_deref(),
                profile.optimal_config.as_deref(),
            )?;
        } else {
            info!("   ❌ No end-to-end valid final configuration found, falling back to defaults");
            profile.optimal_config = None;
            profile.supported_options.clear();
            profile.supported_profile = None;
            clear_validated_decoder(serial)?;
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

    // Clear heuristic encoder name if no options were validated, so the
    // next connection falls back to scrcpy's default encoder instead of
    // using an unverified guess.
    if profile.supported_options.is_empty() && profile.supported_profile.is_none() {
        profile.video_encoder = None;
        clear_validated_decoder(serial)?;
    }

    let mut db = EncoderProfileDatabase::load()?;
    db.insert(profile.clone());
    db.save()?;

    send(ProbeStep::Done(Ok(profile.optimal_config.clone())));

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

fn validate_decoder(
    serial: &str,
    server_jar: &str,
    options: &str,
    video_encoder: Option<&str>,
) -> Result<Option<DecoderPreference>> {
    use crate::scrcpy::connection::ScrcpyConnection;

    let params = ServerParams {
        video: true,
        video_codec: "h264".to_string(),
        video_encoder: video_encoder.map(str::to_string),
        video_bit_rate: 4_000_000,
        max_size: 800,
        max_fps: 30,
        audio: false,
        control: false,
        send_device_meta: false,
        send_codec_meta: true,
        send_frame_meta: true,
        video_codec_options: Some(options.to_string()),
        ..Default::default()
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    rt.block_on(async {
        let mut conn = match ScrcpyConnection::connect(serial, server_jar, "127.0.0.1", params) {
            Ok(conn) => conn,
            Err(e) => {
                info!("  Decoder validation connection failed: {}", e);
                return Ok(None);
            }
        };

        let packets = collect_validation_packets(&mut conn);
        let _ = conn.shutdown();
        let packets = packets?;

        if packets.is_empty() {
            info!("  No video packets captured for decoder validation");
            return Ok(None);
        }

        for candidate in DecoderPreference::hardware_candidates() {
            if validate_decoder_candidate(*candidate, &packets) {
                return Ok(Some(*candidate));
            }
        }

        Ok(None)
    })
}

fn validate_end_to_end(
    serial: &str,
    server_jar: &str,
    options: &str,
    video_encoder: Option<&str>,
) -> Result<EndToEndValidation> {
    if !test_codec_options(serial, server_jar, options, video_encoder)? {
        return Ok(EndToEndValidation::DeviceRejected);
    }

    Ok(
        match validate_decoder(serial, server_jar, options, video_encoder)? {
            Some(decoder) => EndToEndValidation::HostAccepted(decoder),
            None => EndToEndValidation::HostRejected,
        },
    )
}

fn validate_final_config(
    serial: &str,
    server_jar: &str,
    video_encoder: Option<&str>,
    selected_profile: Option<&str>,
    supported_options: &[String],
) -> Result<FinalValidationResult> {
    for profile_candidate in profile_fallbacks(selected_profile) {
        if profile_candidate != selected_profile {
            info!(
                "   ↩️ Retrying with more conservative profile {}",
                profile_candidate.unwrap_or("default")
            );
        }

        for option_set in trim_option_sets(supported_options) {
            let Some(config) = build_options_string_for(profile_candidate, &option_set) else {
                continue;
            };

            if option_set.len() != supported_options.len() {
                info!(
                    "   ↩️ Retrying with {} option(s): {}",
                    option_set.len(),
                    config
                );
            }

            match validate_end_to_end(serial, server_jar, &config, video_encoder)? {
                EndToEndValidation::HostAccepted(decoder) => {
                    return Ok(Some((profile_candidate, option_set, decoder)));
                }
                EndToEndValidation::HostRejected => {
                    info!("   ⚠️ Host decoder validation failed for {}", config);
                }
                EndToEndValidation::DeviceRejected => {
                    info!("   ⚠️ Device rejected candidate config {}", config);
                }
            }
        }
    }

    Ok(None)
}

fn collect_validation_packets(
    conn: &mut crate::scrcpy::connection::ScrcpyConnection,
) -> Result<Vec<(Vec<u8>, i64)>> {
    let mut packets = Vec::new();
    let mut saw_config_or_sps = false;

    conn.set_video_read_timeout(Some(PROBE_PACKET_TIMEOUT))?;

    while packets.len() < VALIDATION_PACKET_LIMIT {
        let packet = match conn.read_video_packet() {
            Ok(packet) => packet,
            Err(e) => {
                info!("  Validation packet read failed: {}", e);
                break;
            }
        };

        if packet.data.is_empty() {
            continue;
        }

        let has_sps = extract_resolution_from_stream(&packet.data).is_some();
        if !saw_config_or_sps {
            if !packet.is_config && !has_sps {
                continue;
            }

            saw_config_or_sps = true;
            info!(
                "  Detected {} packet for validation",
                if packet.is_config { "config" } else { "SPS" }
            );
        }

        packets.push((packet.data, packet.pts_us as i64));
    }

    conn.set_video_read_timeout(None)?;

    if !saw_config_or_sps {
        info!(
            "  Timed out after {:?} waiting for config/SPS packet",
            PROBE_PACKET_TIMEOUT
        );
    }

    Ok(packets)
}

fn save_validated_decoder(
    serial: &str,
    decoder: DecoderPreference,
    video_encoder: Option<&str>,
    optimal_config: Option<&str>,
) -> Result<()> {
    let mut db = DecoderProfileDatabase::load()?;
    db.insert(DecoderProfile {
        serial: serial.to_string(),
        validated_decoder: decoder.profile_name().to_string(),
        encoder_fingerprint: encoder_fingerprint(video_encoder, optimal_config).unwrap_or_default(),
        tested_at: chrono::Utc::now().to_rfc3339(),
    });
    db.save()
}

fn clear_validated_decoder(serial: &str) -> Result<()> {
    let mut db = DecoderProfileDatabase::load()?;
    db.remove(serial);
    db.save()
}

fn validate_decoder_candidate(candidate: DecoderPreference, packets: &[(Vec<u8>, i64)]) -> bool {
    let Some((width, height)) = packets
        .iter()
        .find_map(|(packet, _)| extract_resolution_from_stream(packet))
    else {
        info!(
            "  {} validation skipped: SPS resolution unavailable",
            candidate.profile_name()
        );
        return false;
    };

    let mut decoder = match AutoDecoder::new_exact(width, height, candidate) {
        Ok(decoder) => decoder,
        Err(e) => {
            info!(
                "  {} validation skipped: decoder init failed: {}",
                candidate.profile_name(),
                e
            );
            return false;
        }
    };

    for (packet, pts) in packets {
        match decoder.decode(packet, *pts) {
            Ok(Some(_)) => {
                info!(
                    "  ✅ {} decoded a validation frame",
                    candidate.profile_name()
                );
                return true;
            }
            Ok(None) => continue,
            Err(e) => {
                info!("  {} validation failed: {}", candidate.profile_name(), e);
                return false;
            }
        }
    }

    info!(
        "  {} validation exhausted captured packets without producing a frame",
        candidate.profile_name()
    );
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_build_options_baseline() {
        let profile = EncoderProfile {
            serial: "test".to_string(),
            model: "Test".to_string(),
            platform: "test".to_string(),
            android_version: 14,
            video_encoder: Some("c2.test.avc.encoder".to_string()),
            supported_options: vec!["i-frame-interval".to_string(), "latency".to_string()],
            supported_profile: Some("66".to_string()),
            optimal_config: None,
            tested_at: "2025-01-01T00:00:00Z".to_string(),
        };

        let options = profile.build_options_string().unwrap();
        assert!(options.contains("profile=66"));
        assert!(options.contains("i-frame-interval=2"));
        assert!(options.contains("latency=0"));
    }

    #[test]
    fn test_profile_build_options_nvdec() {
        let profile = EncoderProfile {
            serial: "test".to_string(),
            model: "Test".to_string(),
            platform: "test".to_string(),
            android_version: 14,
            video_encoder: Some("c2.test.avc.encoder".to_string()),
            supported_options: vec!["i-frame-interval".to_string()],
            supported_profile: Some("65536".to_string()),
            optimal_config: None,
            tested_at: "2025-01-01T00:00:00Z".to_string(),
        };

        let options = profile.build_options_string().unwrap();
        assert!(options.contains("profile=65536"));
        assert!(options.contains("i-frame-interval=2"));
    }

    #[test]
    fn test_profile_build_options_profile_only() {
        let profile = EncoderProfile {
            serial: "test".to_string(),
            model: "Test".to_string(),
            platform: "test".to_string(),
            android_version: 14,
            video_encoder: Some("c2.test.avc.encoder".to_string()),
            supported_options: Vec::new(),
            supported_profile: Some("66".to_string()),
            optimal_config: None,
            tested_at: "2025-01-01T00:00:00Z".to_string(),
        };

        assert_eq!(
            profile.build_options_string().as_deref(),
            Some("profile=66")
        );
    }

    #[test]
    fn test_decoder_preference_roundtrip() {
        assert_eq!(
            DecoderPreference::from_profile_name("NVDEC").map(|it| it.profile_name()),
            Some("NVDEC")
        );
    }

    #[test]
    fn test_encoder_fingerprint_contents() {
        assert_eq!(
            encoder_fingerprint(Some("c2.qcom.avc.encoder"), Some("profile=66,latency=0")),
            Some("encoder=c2.qcom.avc.encoder|options=profile=66,latency=0".to_string())
        );
        assert_eq!(encoder_fingerprint(None, None), None);
    }

    #[test]
    fn test_profile_fallbacks_from_65536() {
        assert_eq!(
            profile_fallbacks(Some("65536")),
            vec![Some("65536"), Some("1"), Some("66")]
        );
    }

    #[test]
    fn test_trim_option_sets_descending() {
        let options = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(
            trim_option_sets(&options),
            vec![
                vec!["a".to_string(), "b".to_string(), "c".to_string()],
                vec!["a".to_string(), "b".to_string()],
                vec!["a".to_string()],
                vec![],
            ]
        );
    }
}
