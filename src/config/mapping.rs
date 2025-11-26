use {
    crate::controller::utils::*,
    eframe::{egui, egui::PointerButton},
    serde::{Deserialize, Serialize},
    std::{
        collections::HashMap,
        fmt::{self, Display},
        ops::Deref,
        sync::Arc,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    // TODO: Add extra buttons
}

#[derive(Debug, Serialize, Deserialize)]
pub enum WheelDirection {
    Up,
    Down,
}

impl From<PointerButton> for MouseButton {
    fn from(button: PointerButton) -> Self {
        match button {
            PointerButton::Primary => MouseButton::Left,
            PointerButton::Secondary => MouseButton::Right,
            PointerButton::Middle => MouseButton::Middle,
            _ => MouseButton::Left, // Default case
        }
    }
}

impl Display for MouseButton {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            MouseButton::Left => "BTN_LEFT",
            MouseButton::Right => "BTN_RIGHT",
            MouseButton::Middle => "BTN_MIDDLE",
        };
        write!(f, "{}", s)
    }
}

pub type Key = egui::Key;

#[derive(Debug, Serialize, Deserialize)]
pub enum AdbActionType {
    Tap,
    Swipe,
    Key,
    Back,
    Home,
    Menu,
    Power,
}

/// ADB input command types
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "action")]
pub enum AdbAction {
    Tap {
        x: u32,
        y: u32,
    },
    Swipe {
        x1: u32,
        y1: u32,
        x2: u32,
        y2: u32,
        duration: u32,
    },
    Key {
        keycode: String,
    },
    Text {
        text: String,
    },
    Back,
    Home,
    Menu,
    Power,

    Ignore,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct KeyMapping {
    #[serde(flatten)]
    inner: HashMap<Key, AdbAction>,
}

impl Deref for KeyMapping {
    type Target = HashMap<Key, AdbAction>;

    fn deref(&self) -> &Self::Target { &self.inner }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    #[serde(serialize_with = "serialize_arc", deserialize_with = "deserialize_arc")]
    pub mappings: Arc<KeyMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyboardConfig {
    pub toggle: String,
    pub initial_state: bool,
    pub show_notification: bool,
    pub profiles: Vec<Profile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MouseConfig {
    pub initial_state: bool,
}

impl Default for MouseConfig {
    fn default() -> Self {
        Self {
            initial_state: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappingConfig {
    #[serde(
        serialize_with = "serialize_option_arc",
        deserialize_with = "deserialize_option_arc"
    )]
    pub keyboard: Option<Arc<KeyboardConfig>>,
    #[serde(
        serialize_with = "serialize_option_arc",
        deserialize_with = "deserialize_option_arc"
    )]
    pub mouse: Option<Arc<MouseConfig>>,
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
            keyboard: None,
            mouse: Some(Arc::new(MouseConfig::default())),
        }
    }
}
