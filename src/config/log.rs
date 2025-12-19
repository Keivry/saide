//! Configuration for logging.

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct LogConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
}

fn default_log_level() -> String { "info".to_string() }
