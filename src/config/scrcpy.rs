use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ScrcpyConfig {
    pub video: VideoConfig,
    pub audio: AudioConfig,
    pub options: OptionsConfig,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct VideoConfig {
    #[serde(default = "default_bitrate")]
    pub bit_rate: String,
    #[serde(default = "default_max_fps")]
    pub max_fps: u32,
    #[serde(default = "default_max_size")]
    pub max_size: u32,
    #[serde(default = "default_codec")]
    pub codec: String,
    #[serde(default = "default_encoder")]
    pub encoder: Option<String>,
}

fn default_bitrate() -> String { "8M".to_string() }
fn default_max_fps() -> u32 { 60 }
fn default_max_size() -> u32 { 1920 }
fn default_codec() -> String { "h264".to_string() }
fn default_encoder() -> Option<String> { None }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    #[serde(default = "default_audio_enabled")]
    pub enabled: bool,
    #[serde(default = "default_audio_codec")]
    pub codec: String,
    #[serde(default = "default_audio_source")]
    pub source: String,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            codec: default_audio_codec(),
            source: default_audio_source(),
        }
    }
}

fn default_audio_enabled() -> bool { true }
fn default_audio_codec() -> String { "opus".to_string() }
fn default_audio_source() -> String { "playback".to_string() }

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct OptionsConfig {
    #[serde(default = "default_true")]
    pub turn_screen_off: bool,
    #[serde(default = "default_true")]
    pub stay_awake: bool,
}

fn default_true() -> bool { true }
