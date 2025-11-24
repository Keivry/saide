use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappingConfig {
    pub toggle: String,
    pub initial_state: bool,
    pub show_notification: bool,
    pub profiles: Vec<Profile>,
    pub mouse: MouseConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub mappings: Vec<KeyMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyMapping {
    pub key: String,
    pub action: String,
    pub pos: Position,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MouseConfig {
    pub initial_state: bool,
    pub mappings: Vec<MouseMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MouseMapping {
    pub button: String,
    pub action: String,
    pub dir: Option<String>,
}

impl MappingConfig {
    pub fn from_toml_value(value: toml::Value) -> Result<Self, String> {
        value
            .try_into()
            .map_err(|e| format!("Failed to parse MappingConfig: {}", e))
    }
}

impl Default for MappingConfig {
    fn default() -> Self {
        Self {
            toggle: "KEY_SCROLLLOCK".to_string(),
            initial_state: false,
            show_notification: true,
            profiles: Vec::new(),
            mouse: MouseConfig {
                initial_state: true,
                mappings: vec![
                    MouseMapping {
                        button: "BTN_LEFT".to_string(),
                        action: "TAP".to_string(),
                        dir: None,
                    },
                    MouseMapping {
                        button: "BTN_RIGHT".to_string(),
                        action: "BACK".to_string(),
                        dir: None,
                    },
                    MouseMapping {
                        button: "BTN_MIDDLE".to_string(),
                        action: "HOME".to_string(),
                        dir: None,
                    },
                    MouseMapping {
                        button: "WHEEL_UP".to_string(),
                        action: "SWIPE".to_string(),
                        dir: Some("DOWN".to_string()),
                    },
                    MouseMapping {
                        button: "WHEEL_DOWN".to_string(),
                        action: "SWIPE".to_string(),
                        dir: Some("UP".to_string()),
                    },
                ],
            },
        }
    }
}

impl Default for MouseConfig {
    fn default() -> Self {
        Self {
            initial_state: true,
            mappings: Vec::new(),
        }
    }
}
