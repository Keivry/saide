//! Module for key mappings and profiles
//!
//! This module defines the structures and serialization logic for key mappings
//! and profiles used in the application. It includes definitions for mapping actions,
//! profiles, and the overall mappings configuration.

mod action;
mod keymapping;
mod mouse;
mod profile;

use serde::{Deserialize, Serialize};
pub use {
    action::{MappingAction, ScrcpyAction},
    keymapping::KeyMapping,
    mouse::{MouseButton, WheelDirection},
    parking_lot::RwLock,
    profile::Profile,
    std::sync::Arc,
};

pub type Key = egui::Key;
pub type Modifiers = egui::Modifiers;

pub type ProfileRef = Arc<RwLock<Profile>>;
pub type Profiles = Arc<RwLock<Vec<ProfileRef>>>;

/// Overall mappings configuration
#[derive(Debug, Serialize, Deserialize)]
pub struct MappingsConfig {
    /// Key to toggle mappings on/off
    #[serde(default = "default_toggle_key")]
    pub toggle: Key,

    /// Initial state of mappings (enabled/disabled)
    #[serde(default)]
    pub initial_state: bool,

    /// Whether to show notification on toggle
    #[serde(default = "default_true")]
    pub show_notification: bool,

    /// List of profiles
    #[serde(
        serialize_with = "serialize_mutex_arc_vec",
        deserialize_with = "deserialize_profiles"
    )]
    profiles: Profiles,
}

impl Default for MappingsConfig {
    fn default() -> Self {
        MappingsConfig {
            toggle: default_toggle_key(),
            initial_state: default_false(),
            show_notification: default_true(),
            profiles: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

fn default_toggle_key() -> Key { Key::F10 }
fn default_false() -> bool { false }
fn default_true() -> bool { true }

fn deserialize_profiles<'de, D>(deserializer: D) -> std::result::Result<Profiles, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let profiles: Vec<Profile> = Deserialize::deserialize(deserializer)?;
    let arc_profiles = profiles
        .into_iter()
        .map(|profile| Arc::new(RwLock::new(profile)))
        .collect();
    Ok(Arc::new(RwLock::new(arc_profiles)))
}

fn serialize_mutex_arc_vec<S>(
    profiles: &Profiles,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let profiles = profiles.read();
    let vec: Vec<Profile> = profiles
        .iter()
        .map(|profile| profile.read().clone())
        .collect();
    vec.serialize(serializer)
}
