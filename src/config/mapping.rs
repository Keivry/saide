use {
    crate::controller::utils::*,
    eframe::egui::{self, PointerButton},
    parking_lot::RwLock,
    serde::{Deserialize, Deserializer, Serialize},
    std::{collections::HashMap, ops::Deref, sync::Arc},
};

/// Raw action from config file (with percentage coordinates)
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action")]
enum RawAdbAction {
    Tap { x: f32, y: f32 },
    Swipe { x1: f32, y1: f32, x2: f32, y2: f32, duration: u32 },
    TouchDown { x: f32, y: f32 },
    TouchMove { x: f32, y: f32 },
    TouchUp { x: f32, y: f32 },
    Scroll { x: f32, y: f32, direction: WheelDirection },
    Key { keycode: u8 },
    KeyCombo { modifiers: Modifiers, keycode: u8 },
    Text { text: String },
    Back,
    Home,
    Menu,
    Power,
    Ignore,
}

impl RawAdbAction {
    /// Validate percentage values are in [0, 1] range
    fn validate(&self) -> Result<(), String> {
        let check_percent = |v: f32, name: &str| -> Result<(), String> {
            if v >= 2.0 {
                Err(format!(
                    "{} coordinate {} appears to be in legacy absolute format. \
                    Please run: python scripts/convert_coords_to_percent.py",
                    name, v
                ))
            } else if !(0.0..=1.0).contains(&v) {
                Err(format!("{} percentage {} must be in range [0.0, 1.0]", name, v))
            } else {
                Ok(())
            }
        };

        match self {
            Self::Tap { x, y } | Self::TouchDown { x, y } | Self::TouchMove { x, y } | Self::TouchUp { x, y } => {
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

    /// Convert to AdbAction with pixel coordinates
    fn to_pixels(&self, video_width: u32, video_height: u32) -> AdbAction {
        match self {
            Self::Tap { x, y } => AdbAction::Tap {
                x: (x * video_width as f32) as u32,
                y: (y * video_height as f32) as u32,
            },
            Self::Swipe { x1, y1, x2, y2, duration } => AdbAction::Swipe {
                x1: (x1 * video_width as f32) as u32,
                y1: (y1 * video_height as f32) as u32,
                x2: (x2 * video_width as f32) as u32,
                y2: (y2 * video_height as f32) as u32,
                duration: *duration,
            },
            Self::TouchDown { x, y } => AdbAction::TouchDown {
                x: (x * video_width as f32) as u32,
                y: (y * video_height as f32) as u32,
            },
            Self::TouchMove { x, y } => AdbAction::TouchMove {
                x: (x * video_width as f32) as u32,
                y: (y * video_height as f32) as u32,
            },
            Self::TouchUp { x, y } => AdbAction::TouchUp {
                x: (x * video_width as f32) as u32,
                y: (y * video_height as f32) as u32,
            },
            Self::Scroll { x, y, direction } => AdbAction::Scroll {
                x: (x * video_width as f32) as u32,
                y: (y * video_height as f32) as u32,
                direction: direction.clone(),
            },
            Self::Key { keycode } => AdbAction::Key { keycode: *keycode },
            Self::KeyCombo { modifiers, keycode } => AdbAction::KeyCombo { modifiers: *modifiers, keycode: *keycode },
            Self::Text { text } => AdbAction::Text { text: text.clone() },
            Self::Back => AdbAction::Back,
            Self::Home => AdbAction::Home,
            Self::Menu => AdbAction::Menu,
            Self::Power => AdbAction::Power,
            Self::Ignore => AdbAction::Ignore,
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
/// Coordinates are stored as actual pixels after conversion from config percentages
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
    /// Convert all percentage coordinates in this profile to pixel coordinates
    /// This should be called after loading from config when video resolution is known
    pub fn convert_to_pixels(&self, video_width: u32, video_height: u32) {
        let mut mappings = self.mappings.inner.write();
        for (_key, action) in mappings.iter_mut() {
            let new_action = match &*action {
                AdbAction::Tap { x, y } if *x <= 1 && *y <= 1 => AdbAction::Tap {
                    x: (*x as f32 * video_width as f32) as u32,
                    y: (*y as f32 * video_height as f32) as u32,
                },
                AdbAction::TouchDown { x, y } if *x <= 1 && *y <= 1 => AdbAction::TouchDown {
                    x: (*x as f32 * video_width as f32) as u32,
                    y: (*y as f32 * video_height as f32) as u32,
                },
                AdbAction::TouchMove { x, y } if *x <= 1 && *y <= 1 => AdbAction::TouchMove {
                    x: (*x as f32 * video_width as f32) as u32,
                    y: (*y as f32 * video_height as f32) as u32,
                },
                AdbAction::TouchUp { x, y } if *x <= 1 && *y <= 1 => AdbAction::TouchUp {
                    x: (*x as f32 * video_width as f32) as u32,
                    y: (*y as f32 * video_height as f32) as u32,
                },
                AdbAction::Scroll { x, y, direction } if *x <= 1 && *y <= 1 => AdbAction::Scroll {
                    x: (*x as f32 * video_width as f32) as u32,
                    y: (*y as f32 * video_height as f32) as u32,
                    direction: direction.clone(),
                },
                AdbAction::Swipe { x1, y1, x2, y2, duration } if *x1 <= 1 && *y1 <= 1 && *x2 <= 1 && *y2 <= 1 => {
                    AdbAction::Swipe {
                        x1: (*x1 as f32 * video_width as f32) as u32,
                        y1: (*y1 as f32 * video_height as f32) as u32,
                        x2: (*x2 as f32 * video_width as f32) as u32,
                        y2: (*y2 as f32 * video_height as f32) as u32,
                        duration: *duration,
                    }
                }
                _ => continue,  // Skip if already converted or no coordinates
            };
            *action = new_action;
        }
        tracing::debug!(
            "Converted profile '{}' coordinates to {}x{} pixels",
            self.name,
            video_width,
            video_height
        );
    }

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
