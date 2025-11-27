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

pub type Key = egui::Key;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub model: String,
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
#[derive(Debug)]
pub struct KeyMapping {
    inner: HashMap<Key, AdbAction>,
}

impl<'de> Deserialize<'de> for KeyMapping {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawMapping {
            key: Key,
            #[serde(flatten)]
            action: AdbAction,
        }

        let raw_mappings: Vec<RawMapping> = Deserialize::deserialize(deserializer)?;
        let mut inner = HashMap::new();
        raw_mappings.into_iter().for_each(|rm| {
            inner.insert(rm.key, rm.action);
        });
        Ok(KeyMapping { inner })
    }
}

impl Serialize for KeyMapping {
    fn serialize<Ser>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error>
    where
        Ser: serde::Serializer,
    {
        #[derive(Serialize)]
        struct RawMapping<'a> {
            key: &'a Key,
            #[serde(flatten)]
            action: &'a AdbAction,
        }

        let raw_mappings: Vec<RawMapping> = self
            .inner
            .iter()
            .map(|(key, action)| RawMapping { key, action })
            .collect();
        raw_mappings.serialize(serializer)
    }
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

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct MappingConfig {
    #[serde(
        serialize_with = "serialize_option_arc",
        deserialize_with = "deserialize_option_arc"
    )]
    #[serde(default)]
    pub keyboard: Option<Arc<KeyboardConfig>>,
    #[serde(
        serialize_with = "serialize_option_arc",
        deserialize_with = "deserialize_option_arc"
    )]
    #[serde(default = "default_mouse_config")]
    pub mouse: Option<Arc<MouseConfig>>,
}

fn default_mouse_config() -> Option<Arc<MouseConfig>> { Some(Arc::new(MouseConfig::default())) }
