use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    pub level: String,
}

impl LogConfig {
    pub fn from_toml_value(value: toml::Value) -> Result<Self, String> {
        value.try_into()
            .map_err(|e| format!("Failed to parse LogConfig: {}", e))
    }
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: "INFO".to_string(),
        }
    }
}
