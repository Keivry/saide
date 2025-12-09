use {
    crate::controller::utils::*,
    eframe::egui::{self, PointerButton},
    parking_lot::RwLock,
    serde::{Deserialize, Serialize},
    std::{collections::HashMap, ops::Deref, sync::Arc},
};

pub type Key = egui::Key;
pub type Modifiers = egui::Modifiers;

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
    inner: Arc<RwLock<HashMap<Key, AdbAction>>>,
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
        let mut m = HashMap::new();
        raw_mappings.into_iter().for_each(|rm| {
            m.insert(rm.key, rm.action);
        });
        Ok(KeyMapping {
            inner: Arc::new(RwLock::new(m)),
        })
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

        let mappings = self.inner.read();
        let raw_mappings = mappings
            .iter()
            .map(|(key, action)| RawMapping { key, action })
            .collect::<Vec<RawMapping>>();
        raw_mappings.serialize(serializer)
    }
}

impl Deref for KeyMapping {
    type Target = Arc<RwLock<HashMap<Key, AdbAction>>>;

    fn deref(&self) -> &Self::Target { &self.inner }
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

impl Profile {
    /// Add a mapping to the profile, returning a new profile
    pub fn add_mapping(&self, key: Key, action: AdbAction) -> &Self {
        self.mappings.inner.write().insert(key, action);
        self
    }

    /// Remove a mapping from the profile, returning a new profile
    pub fn remove_mapping(&self, key: &Key) -> &Self {
        self.mappings.inner.write().remove(key);
        self
    }

    /// Check if this profile matches the given device and rotation
    pub fn matches(&self, device_id: &str, rotation: u32) -> bool {
        self.device_id == device_id && self.rotation == rotation
    }

    /// Get the ADB action for a given key, if it exists
    pub fn get_mapping(&self, key: &Key) -> Option<AdbAction> {
        self.mappings.inner.read().get(key).cloned()
    }

    #[allow(dead_code)]
    pub fn contains_key(&self, key: &Key) -> bool { self.mappings.inner.read().contains_key(key) }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Mappings {
    #[serde(default = "default_toggle_key")]
    pub toggle: String,
    #[serde(default)]
    pub initial_state: bool,
    #[serde(default = "default_true")]
    pub show_notification: bool,
    #[serde(
        deserialize_with = "deserialize_profiles",
        serialize_with = "serialize_mutex_arc_vec"
    )]
    pub profiles: RwLock<Vec<Arc<Profile>>>,
}

impl Mappings {
    /// Filter profiles based on device ID and rotation
    pub fn filter_profiles(&self, device_id: &str, rotation: u32) -> Vec<Arc<Profile>> {
        self.profiles
            .read()
            .iter()
            .filter(|profile| profile.matches(device_id, rotation))
            .cloned()
            .collect()
    }

    #[allow(dead_code)]
    pub fn add_profile(&self, profile: Arc<Profile>) { self.profiles.write().push(profile); }

    #[allow(dead_code)]
    pub fn remove_profile(&self, profile_name: &str) {
        self.profiles
            .write()
            .retain(|profile| profile.name != profile_name);
    }
}

fn default_toggle_key() -> String { "F10".to_string() }
fn default_true() -> bool { true }

fn deserialize_profiles<'de, D>(deserializer: D) -> Result<RwLock<Vec<Arc<Profile>>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let profiles: Vec<Profile> = Deserialize::deserialize(deserializer)?;
    let arc_profiles = profiles.into_iter().map(Arc::new).collect();
    Ok(RwLock::new(arc_profiles))
}

fn serialize_mutex_arc_vec<S>(
    profiles: &RwLock<Vec<Arc<Profile>>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let profiles = profiles.read();
    let vec_profiles: Vec<&Profile> = profiles.iter().map(|p| p.as_ref()).collect();
    vec_profiles.serialize(serializer)
}
