use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrcpyConfig {
    pub v4l2: V4l2Config,
    pub video: VideoConfig,
    pub options: OptionsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V4l2Config {
    pub device: String,
    pub capture_orientation: String,
    pub buffer: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoConfig {
    pub bit_rate: String,
    pub max_fps: u32,
    pub max_size: u32,
    pub codec: String,
    pub encoder: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionsConfig {
    pub turn_screen_off: bool,
    pub stay_awake: bool,
}

impl ScrcpyConfig {
    pub fn from_toml_value(value: toml::Value) -> Result<Self, String> {
        value.try_into()
            .map_err(|e| format!("Failed to parse ScrcpyConfig: {}", e))
    }
}

impl Default for ScrcpyConfig {
    fn default() -> Self {
        Self {
            v4l2: V4l2Config {
                device: "/dev/video0".to_string(),
                capture_orientation: "90".to_string(),
                buffer: 0,
            },
            video: VideoConfig {
                bit_rate: "24M".to_string(),
                max_fps: 60,
                max_size: 1280,
                codec: "h264".to_string(),
                encoder: "c2.mtk.avc.encoder".to_string(),
            },
            options: OptionsConfig {
                turn_screen_off: true,
                stay_awake: true,
            },
        }
    }
}

impl Default for V4l2Config {
    fn default() -> Self {
        Self {
            device: "/dev/video0".to_string(),
            capture_orientation: "90".to_string(),
            buffer: 0,
        }
    }
}

impl Default for VideoConfig {
    fn default() -> Self {
        Self {
            bit_rate: "24M".to_string(),
            max_fps: 60,
            max_size: 1280,
            codec: "h264".to_string(),
            encoder: "c2.mtk.avc.encoder".to_string(),
        }
    }
}

impl Default for OptionsConfig {
    fn default() -> Self {
        Self {
            turn_screen_off: true,
            stay_awake: true,
        }
    }
}
