//! Keyboard mapping configuration
//!
//! This module defines the structures and enums related to keyboard input mapping, including the
//! mapping actions that can be performed when a key is pressed.

use {
    super::action::{MappingAction, ScrcpyAction},
    crate::core::coords::{MappingCoordSys, ScrcpyCoordSys},
    egui::Key,
    serde::{Deserialize, Serialize},
    std::collections::HashMap,
};

/// Key to MappingAction map with serialization support
#[derive(Debug, Clone)]
pub struct KeyMapping {
    inner: HashMap<Key, MappingAction>,
}

impl KeyMapping {
    pub fn new() -> Self { KeyMapping::default() }

    /// Get the mapping action for a given key, if it exists
    pub fn get(&self, key: &Key) -> Option<MappingAction> { self.inner.get(key).cloned() }

    /// Insert or update a mapping for a given key
    pub fn insert(&mut self, key: Key, action: MappingAction) { self.inner.insert(key, action); }

    /// Remove a mapping for a given key
    pub fn remove(&mut self, key: &Key) { self.inner.remove(key); }

    pub fn contains_key(&self, key: &Key) -> bool { self.inner.contains_key(key) }

    pub fn clear(&mut self) { self.inner.clear(); }

    /// Convert this KeyMapping to a Scrcpy mapping using the provided coordinate systems
    pub fn to_scrcpy_mapping(
        &self,
        scrcpy_coords: &ScrcpyCoordSys,
        mapping_coords: &MappingCoordSys,
    ) -> HashMap<Key, ScrcpyAction> {
        self.inner
            .iter()
            .map(|(key, action)| {
                (
                    key.clone(),
                    ScrcpyAction::from_mapping_action(action, scrcpy_coords, mapping_coords),
                )
            })
            .collect()
    }
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
            action: MappingAction,
        }

        let raw_mappings: Vec<RawMapping> = Deserialize::deserialize(deserializer)?;

        let mut m = HashMap::new();
        raw_mappings.into_iter().for_each(|rm| {
            m.insert(rm.key, rm.action);
        });

        Ok(KeyMapping { inner: m })
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
            action: &'a MappingAction,
        }

        let raw_mappings = self
            .inner
            .iter()
            .map(|(key, action)| RawMapping { key, action })
            .collect::<Vec<RawMapping>>();
        raw_mappings.serialize(serializer)
    }
}

impl Default for KeyMapping {
    fn default() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }
}
