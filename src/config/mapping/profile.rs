//! Profile management for key mappings
//!
//! Each profile is associated with a specific device (identified by serial) and rotation, and
//! contains a set of key mappings. The manager module provides functionality to load, save, and
//! manage these profiles.

use {
    super::{Key, action::MappingAction, keymapping::KeyMapping},
    chrono::{DateTime, Utc},
    parking_lot::RwLock,
    serde::{Deserialize, Serialize},
    std::sync::Arc,
};

/// Profile for a specific device and rotation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    name: String,

    /// Device serial this profile is associated with
    device_serial: String,

    /// Rotation this profile is associated with
    rotation: u32,

    /// Last modified timestamp (UTC)
    #[serde(default = "default_last_modified")]
    last_modified: DateTime<Utc>,

    /// Key mappings
    mappings: KeyMapping,
}

fn default_last_modified() -> DateTime<Utc> { Utc::now() }

impl Profile {
    pub fn new(name: &str, device_serial: &str, rotation: u32) -> Self {
        Self {
            name: name.to_owned(),
            device_serial: device_serial.to_owned(),
            rotation,
            last_modified: Utc::now(),
            mappings: KeyMapping::new(),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Key, &MappingAction)> { self.mappings.iter() }

    pub fn name(&self) -> &str { &self.name }

    pub fn device_serial(&self) -> &str { &self.device_serial }

    pub fn rotation(&self) -> u32 { self.rotation }

    pub fn mappings(&self) -> &KeyMapping { &self.mappings }

    pub fn is_empty(&self) -> bool { self.mappings.is_empty() }

    /// Check if this profile matches the given device and rotation
    pub fn matches(&self, device_serial: &str, rotation: u32) -> bool {
        self.device_serial == device_serial && self.rotation == rotation
    }

    /// Get the ADB action for a given key, if it exists
    pub fn get_mapping(&self, key: &Key) -> Option<MappingAction> { self.mappings.get(key) }

    /// Check if the profile contains a mapping for the given key
    pub fn contains_key(&self, key: &Key) -> bool { self.mappings.contains_key(key) }

    /// Rename the profile
    pub fn rename(&mut self, new_name: &str) {
        self.name = new_name.to_owned();
        self.update_timestamp();
    }

    /// Update the last modified timestamp to now
    pub fn update_timestamp(&mut self) { self.last_modified = Utc::now(); }

    /// Add a mapping to the profile
    pub fn add_mapping(&mut self, key: Key, action: MappingAction) -> &Self {
        self.mappings.insert(key, action);
        self.update_timestamp();
        self
    }

    /// Remove a mapping from the profile
    pub fn remove_mapping(&mut self, key: &Key) -> &Self {
        self.mappings.remove(key);
        self.update_timestamp();
        self
    }

    /// Clear all mappings from the profile
    pub fn clear_mappings(&mut self) -> &Self {
        self.mappings.clear();
        self.update_timestamp();
        self
    }
}

pub type ProfileRef = Arc<RwLock<Profile>>;
pub type Profiles = Arc<RwLock<Vec<ProfileRef>>>;
