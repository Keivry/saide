// SPDX-License-Identifier: MIT OR Apache-2.0

use {
    crate::{
        constant::CODEC_PROBE_VERSION,
        error::{IoError, Result, SAideError},
        scrcpy::{connection::ScrcpyConnection, hwcodec, server::ServerParams},
    },
    adbshell::AdbShell,
    crossbeam_channel::Sender,
    scrcpy_protocol::h264::extract_resolution_from_stream,
    serde::{Deserialize, Serialize},
    std::{
        collections::HashMap,
        fs,
        path::{Path, PathBuf},
        time::Duration,
    },
    tracing::{debug, info},
};

fn now_utc_rfc3339() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let h = time_of_day / 3600;
    let m = (time_of_day % 3600) / 60;
    let s = time_of_day % 60;

    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let yr = if mo <= 2 { y + 1 } else { y };

    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", yr, mo, d, h, m, s)
}

/// Progress step emitted during [`probe_device`] to report what the prober is currently doing.
///
/// Consumers can display this in a progress bar or log it.
#[derive(Debug, Clone)]
pub enum ProbeStep {
    /// Querying device properties (model, platform, Android version).
    DetectingDevice,
    /// Detecting the preferred vendor H.264 hardware encoder for the device.
    DetectingEncoder,
    /// Testing a codec profile (e.g. `profile=65536`).
    TestingProfile {
        /// 1-based index of this profile within the total candidates.
        index: usize,
        /// Total number of profile candidates being tested.
        total: usize,
        /// Human-readable name of the profile being tested (e.g. `"profile=1"`).
        name: String,
    },
    /// Testing a single codec option (e.g. `latency=0`).
    TestingOption {
        /// 1-based index of this option within the total candidates.
        index: usize,
        /// Total number of option candidates being tested.
        total: usize,
        /// The codec option key being tested (e.g. `"max-bframes"`).
        key: String,
    },
    /// Validating the combined optimal configuration end-to-end (device + host decoder).
    Validating,
    /// Probing is complete.  The inner value is the resulting `video_codec_options` string,
    /// or an `Err` string describing why no optimal config was found.
    Done(std::result::Result<Option<String>, String>),
}

/// Hardware decoder preference used during codec validation.
///
/// Passed to [`DecoderProbe::validate`] to identify which decoder backend is being tested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecoderPreference {
    /// NVIDIA NVDEC hardware decoder.
    Nvdec,
    #[cfg(target_os = "windows")]
    /// Direct3D 11 Video Acceleration (Windows only).
    D3d11va,
    #[cfg(not(target_os = "windows"))]
    /// VA-API hardware decoder (Linux/non-Windows).
    Vaapi,
}

impl DecoderPreference {
    pub fn profile_name(self) -> &'static str {
        match self {
            Self::Nvdec => "NVDEC",
            #[cfg(target_os = "windows")]
            Self::D3d11va => "D3D11VA",
            #[cfg(not(target_os = "windows"))]
            Self::Vaapi => "VAAPI",
        }
    }
}

/// Trait for testing whether captured H.264 packets decode successfully on the host.
///
/// Implement this to plug in a host-side hardware decoder for end-to-end validation
/// during [`probe_device`].
pub trait DecoderProbe {
    /// Return the list of hardware decoder backends to try, in preference order.
    fn hardware_candidates(&self) -> &'static [DecoderPreference];

    /// Return `true` if `candidate` can successfully decode `packets`.
    ///
    /// `packets` is a slice of `(raw_nalu_data, pts_us)` pairs captured from the device.
    fn validate(&self, candidate: DecoderPreference, packets: &[(Vec<u8>, i64)]) -> bool;
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

/// Validated encoder configuration and device metadata for a specific Android device.
///
/// Produced by [`probe_device`] and stored in [`EncoderProfileDatabase`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncoderProfile {
    /// Schema version of this profile entry; see
    /// [`CODEC_PROBE_VERSION`](crate::constant::CODEC_PROBE_VERSION).
    #[serde(default)]
    pub version: u32,
    /// ADB serial of the device.
    pub serial: String,
    /// Device model string (e.g. `"Pixel 7"`).
    pub model: String,
    /// SoC platform identifier (e.g. `"kalama"`).
    pub platform: String,
    /// Android API level (e.g. `34` for Android 14).
    pub android_version: u32,
    /// Vendor H.264 hardware encoder name, or `None` to use the system default.
    pub video_encoder: Option<String>,
    /// Codec option keys that the device accepted during probing (e.g. `["latency",
    /// "max-bframes"]`).
    pub supported_options: Vec<String>,
    /// Validated H.264 profile value string (e.g. `"65536"`), or `None` if none passed.
    pub supported_profile: Option<String>,
    /// Combined `video_codec_options` string ready to pass to the scrcpy server, or `None`.
    pub optimal_config: Option<String>,
    /// RFC 3339 UTC timestamp of when this profile was last probed.
    pub tested_at: String,
}

impl EncoderProfile {
    pub fn new(serial: &str) -> Result<Self> {
        Ok(Self {
            serial: serial.to_string(),
            model: AdbShell::get_prop(serial, "ro.product.model")?,
            platform: AdbShell::get_platform(serial)?,
            android_version: AdbShell::get_android_version(serial)?,
            video_encoder: None,
            supported_options: Vec::new(),
            supported_profile: None,
            optimal_config: None,
            tested_at: now_utc_rfc3339(),
            version: CODEC_PROBE_VERSION,
        })
    }

    pub fn build_options_string(&self) -> Option<String> {
        build_options_string_for(self.supported_profile.as_deref(), &self.supported_options)
    }
}

/// Persistent cache of [`EncoderProfile`] records, keyed by device serial.
///
/// Loaded from and saved to a TOML file via [`EncoderProfileDatabase::load`] /
/// [`EncoderProfileDatabase::save`].
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EncoderProfileDatabase {
    profiles: HashMap<String, EncoderProfile>,
}

/// Validated decoder configuration for a specific device+encoder combination.
///
/// Produced during [`probe_device`] after a successful end-to-end validation and stored
/// in [`DecoderProfileDatabase`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecoderProfile {
    /// Schema version of this profile entry; see
    /// [`CODEC_PROBE_VERSION`](crate::constant::CODEC_PROBE_VERSION).
    #[serde(default)]
    pub version: u32,
    /// ADB serial of the device.
    pub serial: String,
    /// Name of the validated hardware decoder (e.g. `"NVDEC"`, `"VAAPI"`).
    pub validated_decoder: String,
    /// Fingerprint of the encoder settings at validation time; used to detect stale profiles.
    pub encoder_fingerprint: String,
    /// RFC 3339 UTC timestamp of when this profile was last validated.
    pub tested_at: String,
}

/// Persistent cache of [`DecoderProfile`] records, keyed by device serial.
///
/// Loaded from and saved to a TOML file via [`DecoderProfileDatabase::load`] /
/// [`DecoderProfileDatabase::save`].
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EndToEndValidation {
    DeviceRejected,
    HostRejected,
    HostAccepted(DecoderPreference),
}

type FinalValidationResult = Option<(Option<&'static str>, Vec<String>, DecoderPreference)>;

/// Compute a fingerprint string for a given encoder configuration.
///
/// The fingerprint encodes both the encoder name and the option string so that
/// a cached [`DecoderProfile`] can be invalidated when either changes.  Returns
/// `None` when both arguments are `None` (i.e. no encoder is in use).
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
    pub fn load(config_dir: &Path) -> Result<Self> {
        let path = Self::config_path(config_dir);
        if !path.exists() {
            return Self::load_legacy(config_dir);
        }

        let content = fs::read_to_string(&path).map_err(|e| {
            SAideError::IoError(
                IoError::new(e).with_message("Failed to read encoder profile database"),
            )
        })?;
        Ok(toml::from_str(&content)?)
    }

    pub fn save(&self, config_dir: &Path) -> Result<()> {
        let path = Self::config_path(config_dir);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, toml::to_string_pretty(self)?)?;
        Ok(())
    }

    fn config_path(config_dir: &Path) -> PathBuf {
        profile_path(config_dir, "encoder_profile.toml")
    }

    fn legacy_path(config_dir: &Path) -> PathBuf {
        profile_path(config_dir, "device_profiles.toml")
    }

    fn load_legacy(config_dir: &Path) -> Result<Self> {
        let path = Self::legacy_path(config_dir);
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
                            version: 0,
                        },
                    )
                })
                .collect(),
        })
    }

    pub fn get(&self, serial: &str) -> Option<&EncoderProfile> {
        self.profiles
            .get(serial)
            .filter(|p| p.version >= CODEC_PROBE_VERSION)
    }

    pub fn is_stale_for(&self, serial: &str) -> bool {
        self.profiles
            .get(serial)
            .is_some_and(|p| p.version < CODEC_PROBE_VERSION)
    }

    pub fn insert(&mut self, profile: EncoderProfile) {
        self.profiles.insert(profile.serial.clone(), profile);
    }
}

impl DecoderProfileDatabase {
    pub fn load(config_dir: &Path) -> Result<Self> {
        let path = Self::config_path(config_dir);
        if !path.exists() {
            return Self::load_legacy(config_dir);
        }

        let content = fs::read_to_string(&path).map_err(|e| {
            SAideError::IoError(
                IoError::new(e).with_message("Failed to read decoder profile database"),
            )
        })?;
        Ok(toml::from_str(&content)?)
    }

    pub fn save(&self, config_dir: &Path) -> Result<()> {
        let path = Self::config_path(config_dir);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, toml::to_string_pretty(self)?)?;
        Ok(())
    }

    fn config_path(config_dir: &Path) -> PathBuf {
        profile_path(config_dir, "decoder_profile.toml")
    }

    fn legacy_path(config_dir: &Path) -> PathBuf {
        profile_path(config_dir, "device_profiles.toml")
    }

    fn load_legacy(config_dir: &Path) -> Result<Self> {
        let path = Self::legacy_path(config_dir);
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
                                version: 0,
                            },
                        )
                    })
                })
                .collect(),
        })
    }

    pub fn get(&self, serial: &str) -> Option<&DecoderProfile> {
        self.profiles
            .get(serial)
            .filter(|p| p.version >= CODEC_PROBE_VERSION)
    }

    pub fn insert(&mut self, profile: DecoderProfile) {
        self.profiles.insert(profile.serial.clone(), profile);
    }

    pub fn remove(&mut self, serial: &str) { self.profiles.remove(serial); }
}

fn profile_path(config_dir: &Path, file_name: &str) -> PathBuf { config_dir.join(file_name) }

/// Probe a device and determine the best codec configuration.
///
/// Detects the device's hardware H.264 encoder, then iterates over known codec
/// profiles to find the first one accepted by both the device and the host
/// decoder.  The result is persisted to the [`EncoderProfileDatabase`] under
/// `config_dir` so that subsequent calls can skip the probe.
///
/// Returns `Ok(Some(config))` with the optimal encoder option string, or
/// `Ok(None)` if no profile was validated.
///
/// `progress_tx` receives [`ProbeStep`] updates during the probe; pass `None`
/// to suppress progress reporting.
pub fn probe_device(
    decoder_probe: &impl DecoderProbe,
    serial: &str,
    server_jar: &str,
    config_dir: &Path,
    progress_tx: Option<&Sender<ProbeStep>>,
) -> Result<Option<String>> {
    let send = |step: ProbeStep| {
        if let Some(tx) = progress_tx {
            let _ = tx.send(step);
        }
    };

    send(ProbeStep::DetectingDevice);
    let mut profile = EncoderProfile::new(serial)?;
    send(ProbeStep::DetectingEncoder);
    profile.video_encoder = hwcodec::detect_h264_encoder(serial)?;

    let total_profiles = CODEC_PROFILES.len();
    for (i, (key, value)) in CODEC_PROFILES.iter().enumerate() {
        send(ProbeStep::TestingProfile {
            index: i + 1,
            total: total_profiles,
            name: format!("{}={}", key, value),
        });

        let options = format!("{}={}", key, value);
        match validate_end_to_end(
            decoder_probe,
            serial,
            server_jar,
            &options,
            profile.video_encoder.as_deref(),
        )? {
            EndToEndValidation::HostAccepted(decoder) => {
                info!(
                    "Profile {}={} passed via {}",
                    key,
                    value,
                    decoder.profile_name()
                );
                profile.supported_profile = Some(value.to_string());
                break;
            }
            EndToEndValidation::HostRejected | EndToEndValidation::DeviceRejected => {}
        }
    }

    let candidate_options: Vec<(&str, &str)> = CODEC_OPTIONS_BASE
        .iter()
        .filter(|(key, _)| match *key {
            "latency" if profile.android_version < 11 => false,
            "max-bframes" if profile.android_version < 13 => false,
            _ => true,
        })
        .copied()
        .collect();

    let total_options = candidate_options.len();
    for (i, (key, value)) in candidate_options.iter().enumerate() {
        send(ProbeStep::TestingOption {
            index: i + 1,
            total: total_options,
            key: key.to_string(),
        });

        let options = format!("{}={}", key, value);
        if test_codec_options(
            serial,
            server_jar,
            &options,
            profile.video_encoder.as_deref(),
        )? {
            profile.supported_options.push(key.to_string());
        }
    }

    profile.optimal_config = profile.build_options_string();

    if profile.optimal_config.is_some() {
        send(ProbeStep::Validating);
        if let Some((validated_profile, validated_options, decoder)) = validate_final_config(
            decoder_probe,
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
            save_validated_decoder(
                serial,
                decoder,
                profile.video_encoder.as_deref(),
                profile.optimal_config.as_deref(),
                config_dir,
            )?;
        } else {
            profile.optimal_config = None;
            profile.supported_options.clear();
            profile.supported_profile = None;
            clear_validated_decoder(serial, config_dir)?;
        }
    }

    if profile.supported_options.is_empty() && profile.supported_profile.is_none() {
        profile.video_encoder = None;
        clear_validated_decoder(serial, config_dir)?;
    }

    let mut db = EncoderProfileDatabase::load(config_dir)?;
    db.insert(profile.clone());
    db.save(config_dir)?;
    send(ProbeStep::Done(Ok(profile.optimal_config.clone())));

    Ok(profile.optimal_config)
}

fn build_options_string_for(
    supported_profile: Option<&str>,
    supported_options: &[String],
) -> Option<String> {
    let mut options = Vec::new();

    if let Some(profile_value) = supported_profile {
        options.push(format!("profile={}", profile_value));
    }

    for (key, value) in CODEC_OPTIONS_BASE {
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

fn test_codec_options(
    serial: &str,
    server_jar: &str,
    options: &str,
    video_encoder: Option<&str>,
) -> Result<bool> {
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

    let mut conn = match ScrcpyConnection::connect(serial, server_jar, "127.0.0.1", params) {
        Ok(c) => c,
        Err(_) => return Ok(false),
    };

    let ok = conn.read_video_packet().is_ok();
    let _ = conn.shutdown();
    Ok(ok)
}

fn validate_decoder(
    decoder_probe: &impl DecoderProbe,
    serial: &str,
    server_jar: &str,
    options: &str,
    video_encoder: Option<&str>,
) -> Result<Option<DecoderPreference>> {
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

    let mut conn = match ScrcpyConnection::connect(serial, server_jar, "127.0.0.1", params) {
        Ok(conn) => conn,
        Err(_) => return Ok(None),
    };

    let packets = collect_validation_packets(&mut conn)?;
    let _ = conn.shutdown();
    if packets.is_empty() {
        return Ok(None);
    }

    for candidate in decoder_probe.hardware_candidates() {
        if decoder_probe.validate(*candidate, &packets) {
            return Ok(Some(*candidate));
        }
    }

    Ok(None)
}

fn validate_end_to_end(
    decoder_probe: &impl DecoderProbe,
    serial: &str,
    server_jar: &str,
    options: &str,
    video_encoder: Option<&str>,
) -> Result<EndToEndValidation> {
    if !test_codec_options(serial, server_jar, options, video_encoder)? {
        return Ok(EndToEndValidation::DeviceRejected);
    }

    Ok(
        match validate_decoder(decoder_probe, serial, server_jar, options, video_encoder)? {
            Some(decoder) => EndToEndValidation::HostAccepted(decoder),
            None => EndToEndValidation::HostRejected,
        },
    )
}

fn validate_final_config(
    decoder_probe: &impl DecoderProbe,
    serial: &str,
    server_jar: &str,
    video_encoder: Option<&str>,
    selected_profile: Option<&str>,
    supported_options: &[String],
) -> Result<FinalValidationResult> {
    for profile_candidate in profile_fallbacks(selected_profile) {
        for option_set in trim_option_sets(supported_options) {
            let Some(config) = build_options_string_for(profile_candidate, &option_set) else {
                continue;
            };

            match validate_end_to_end(decoder_probe, serial, server_jar, &config, video_encoder)? {
                EndToEndValidation::HostAccepted(decoder) => {
                    return Ok(Some((profile_candidate, option_set, decoder)));
                }
                EndToEndValidation::HostRejected | EndToEndValidation::DeviceRejected => {}
            }
        }
    }

    Ok(None)
}

fn collect_validation_packets(conn: &mut ScrcpyConnection) -> Result<Vec<(Vec<u8>, i64)>> {
    let mut packets = Vec::new();
    let mut saw_config_or_sps = false;
    conn.set_video_read_timeout(Some(PROBE_PACKET_TIMEOUT))?;

    while packets.len() < VALIDATION_PACKET_LIMIT {
        let packet = match conn.read_video_packet() {
            Ok(packet) => packet,
            Err(_) => break,
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
        }

        packets.push((packet.data, packet.pts_us as i64));
    }

    conn.set_video_read_timeout(None)?;
    if !saw_config_or_sps {
        debug!("Timed out waiting for config/SPS packet");
    }
    Ok(packets)
}

fn save_validated_decoder(
    serial: &str,
    decoder: DecoderPreference,
    video_encoder: Option<&str>,
    optimal_config: Option<&str>,
    config_dir: &Path,
) -> Result<()> {
    let mut db = DecoderProfileDatabase::load(config_dir)?;
    db.insert(DecoderProfile {
        serial: serial.to_string(),
        validated_decoder: decoder.profile_name().to_string(),
        encoder_fingerprint: encoder_fingerprint(video_encoder, optimal_config).unwrap_or_default(),
        tested_at: now_utc_rfc3339(),
        version: CODEC_PROBE_VERSION,
    });
    db.save(config_dir)
}

fn clear_validated_decoder(serial: &str, config_dir: &Path) -> Result<()> {
    let mut db = DecoderProfileDatabase::load(config_dir)?;
    db.remove(serial);
    db.save(config_dir)
}
