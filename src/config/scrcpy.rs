use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ScrcpyConfig {
    pub v4l2: V4l2Config,
    pub video: VideoConfig,
    pub options: OptionsConfig,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct V4l2Config {
    #[serde(default = "default_v4l2_device")]
    pub device: String,
    #[serde(default, deserialize_with = "deserialize_capture_orientation")]
    pub capture_orientation: u32,
    #[serde(default)]
    pub buffer: i32,
}

fn deserialize_capture_orientation<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: Option<u32> = Option::deserialize(deserializer)?;
    match s {
        Some(0) => Ok(0),
        Some(90) => Ok(90),
        Some(180) => Ok(180),
        Some(270) => Ok(270),
        Some(other) => Err(serde::de::Error::custom(format!(
            "invalid capture_orientation: {}",
            other
        ))),
        None => Ok(0),
    }
}

fn default_v4l2_device() -> String { "/dev/video0".to_string() }

#[derive(Debug, Default, Serialize, Deserialize)]
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
fn default_max_size() -> u32 { 1280 }
fn default_codec() -> String { "h264".to_string() }
fn default_encoder() -> Option<String> { None }

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct OptionsConfig {
    #[serde(default = "default_true")]
    pub turn_screen_off: bool,
    #[serde(default = "default_true")]
    pub stay_awake: bool,
}

fn default_true() -> bool { true }
