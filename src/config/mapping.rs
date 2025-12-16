use {
    crate::controller::utils::*,
    eframe::egui::{self, PointerButton},
    parking_lot::RwLock,
    serde::{Deserialize, Serialize},
    std::{collections::HashMap, ops::Deref, sync::Arc},
};

/// Percentage coordinate (0.0-1.0)
type Percent = f32;

/// Raw action from config file (with percentage coordinates 0.0-1.0)
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action")]
enum RawInputAction {
    Tap {
        x: Percent,
        y: Percent,
    },
    Swipe {
        x1: Percent,
        y1: Percent,
        x2: Percent,
        y2: Percent,
        duration: u32,
    },
    TouchDown {
        x: Percent,
        y: Percent,
    },
    TouchMove {
        x: Percent,
        y: Percent,
    },
    TouchUp {
        x: Percent,
        y: Percent,
    },
    Scroll {
        x: Percent,
        y: Percent,
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

impl RawInputAction {
    /// Validate percentage values are in [0, 1] range
    fn validate(&self) -> Result<(), String> {
        let check_percent = |v: Percent, name: &str| -> Result<(), String> {
            if v >= 2.0 {
                Err(format!(
                    "{} coordinate {} appears to be in legacy absolute format. \
                    Please run: python scripts/convert_coords_to_percent.py",
                    name, v
                ))
            } else if !(0.0..=1.0).contains(&v) {
                Err(format!(
                    "{} percentage {} must be in range [0.0, 1.0]",
                    name, v
                ))
            } else {
                Ok(())
            }
        };

        match self {
            Self::Tap { x, y }
            | Self::TouchDown { x, y }
            | Self::TouchMove { x, y }
            | Self::TouchUp { x, y } => {
                check_percent(*x, "x")?;
                check_percent(*y, "y")?;
            }
            Self::Scroll { x, y, .. } => {
                check_percent(*x, "x")?;
                check_percent(*y, "y")?;
            }
            Self::Swipe { x1, y1, x2, y2, .. } => {
                check_percent(*x1, "x1")?;
                check_percent(*y1, "y1")?;
                check_percent(*x2, "x2")?;
                check_percent(*y2, "y2")?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Convert to InputAction, keeping percentage coordinates
    fn to_input_action(&self) -> InputAction {
        match self {
            Self::Tap { x, y } => InputAction::Tap { x: *x, y: *y },
            Self::Swipe {
                x1,
                y1,
                x2,
                y2,
                duration,
            } => InputAction::Swipe {
                x1: *x1,
                y1: *y1,
                x2: *x2,
                y2: *y2,
                duration: *duration,
            },
            Self::TouchDown { x, y } => InputAction::TouchDown { x: *x, y: *y },
            Self::TouchMove { x, y } => InputAction::TouchMove { x: *x, y: *y },
            Self::TouchUp { x, y } => InputAction::TouchUp { x: *x, y: *y },
            Self::Scroll { x, y, direction } => InputAction::Scroll {
                x: *x,
                y: *y,
                direction: direction.clone(),
            },
            Self::Key { keycode } => InputAction::Key { keycode: *keycode },
            Self::KeyCombo { modifiers, keycode } => InputAction::KeyCombo {
                modifiers: *modifiers,
                keycode: *keycode,
            },
            Self::Text { text } => InputAction::Text { text: text.clone() },
            Self::Back => InputAction::Back,
            Self::Home => InputAction::Home,
            Self::Menu => InputAction::Menu,
            Self::Power => InputAction::Power,
            Self::Ignore => InputAction::Ignore,
        }
    }
}

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
///
/// Coordinates are stored as:
/// - Percentage (0.0-1.0 f32) when loaded from config
/// - Converted to pixels (u32) when profile is activated via convert_to_pixels()
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action")]
pub enum InputAction {
    Tap {
        x: Percent,
        y: Percent,
    },
    Swipe {
        x1: Percent,
        y1: Percent,
        x2: Percent,
        y2: Percent,
        duration: u32,
    },
    /// Touch down event (start of drag)
    TouchDown {
        x: Percent,
        y: Percent,
    },
    /// Touch move event (during drag)
    TouchMove {
        x: Percent,
        y: Percent,
    },
    /// Touch up event (end of drag)
    TouchUp {
        x: Percent,
        y: Percent,
    },
    Scroll {
        x: Percent,
        y: Percent,
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
    inner: Arc<RwLock<HashMap<Key, InputAction>>>,
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
            action: RawInputAction,
        }

        let raw_mappings: Vec<RawMapping> = Deserialize::deserialize(deserializer)?;

        // Validate all percentage coordinates
        for rm in &raw_mappings {
            rm.action.validate().map_err(serde::de::Error::custom)?;
        }

        // Keep percentage coordinates (0.0-1.0) as-is
        // Will be converted to actual pixels in Profile::convert_to_pixels()
        let mut m = HashMap::new();
        raw_mappings.into_iter().for_each(|rm| {
            m.insert(rm.key, rm.action.to_input_action());
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
            action: &'a InputAction,
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
    type Target = Arc<RwLock<HashMap<Key, InputAction>>>;

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
    pub fn add_mapping(&self, key: Key, action: InputAction) -> &Self {
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
    pub fn get_mapping(&self, key: &Key) -> Option<InputAction> {
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
