use {
    crate::controller::utils::*,
    eframe::{egui, egui::PointerButton},
    serde::{Deserialize, Serialize},
    std::{collections::HashMap, ops::Deref, sync::Arc},
};

pub type Key = egui::Key;
pub type Modifiers = egui::Modifiers;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// ADB input command types
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Touch down event (start of drag)
    TouchDown {
        x: u32,
        y: u32,
    },
    /// Touch move event (during drag)
    TouchMove {
        x: u32,
        y: u32,
    },
    /// Touch up event (end of drag)
    TouchUp {
        x: u32,
        y: u32,
    },
    Scroll {
        x: u32,
        y: u32,
        direction: WheelDirection,
    },
    Key {
        keycode: u8,
    },
    KeyCombo {
        modifiers: Modifiers,
        keycode: u8,
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
#[derive(Debug, Default)]
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

impl KeyMapping {
    pub fn from_hashmap(inner: HashMap<Key, AdbAction>) -> Self {
        Self { inner }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,

    /// Device ID this profile is associated with
    pub device_id: String,

    /// Screen rotation (0-3), each step represents 90 degrees clockwise
    pub rotation: u32,

    /// Key mappings
    #[serde(serialize_with = "serialize_arc", deserialize_with = "deserialize_arc")]
    pub mappings: Arc<KeyMapping>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MappingsConfig {
    #[serde(default = "default_toggle_key")]
    pub toggle: String,
    #[serde(default)]
    pub initial_state: bool,
    #[serde(default = "default_true")]
    pub show_notification: bool,
    pub profiles: Vec<Arc<Profile>>,
}

fn default_toggle_key() -> String { "F10".to_string() }
fn default_true() -> bool { true }
