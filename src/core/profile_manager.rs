use {
    crate::config::mapping::{Key, MappingAction, Profile, ProfileRef, Profiles},
    arc_swap::ArcSwap,
    parking_lot::RwLock,
    std::sync::Arc,
    thiserror::Error,
};

#[derive(Debug, Error)]
pub enum ProfileError {
    #[error("No active profile")]
    NoActiveProfile,

    #[error("Profile not exists")]
    ProfileNotExist,

    #[error("Profile name already exists")]
    ProfileConflict,

    #[error("Invalid profile format")]
    ProfileFormatError,
}

pub type Result<T> = std::result::Result<T, ProfileError>;

/// Manages profiles for different devices and rotations, providing CRUD operations and active
/// profile management.
pub struct ProfileManager {
    /// Reference to the main config for profile CRUD operations
    profiles: Profiles,

    /// Available profiles for current device and rotation, rebuilt by `update()`
    avail_profiles: Vec<ProfileRef>,

    /// Active profile for current device and rotation (if any)
    active_profile: ArcSwap<Option<ProfileRef>>,
}

impl ProfileManager {
    pub fn new(profiles: &Profiles) -> Self {
        ProfileManager {
            profiles: Arc::clone(profiles),
            avail_profiles: Vec::new(),
            active_profile: ArcSwap::new(Arc::new(None)),
        }
    }

    /// Check if a profile with the given name exists in the config
    pub fn profile_exists(&self, profile_name: &str) -> bool {
        ensure_profile_name(profile_name).is_ok()
            && self
                .profiles
                .read()
                .iter()
                .any(|p| p.read().name() == profile_name)
    }

    /// Helper methods to ensure profile existence or non-existence for operations that require it
    fn ensure_profile_exists(&self, profile_name: &str) -> Result<()> {
        if self.profile_exists(profile_name) {
            Ok(())
        } else {
            Err(ProfileError::ProfileNotExist)
        }
    }

    /// Helper method to ensure a profile with the given name does not exist for operations that
    /// require unique names
    fn ensure_profile_not_exists(&self, profile_name: &str) -> Result<()> {
        if self.profile_exists(profile_name) {
            Err(ProfileError::ProfileConflict)
        } else {
            Ok(())
        }
    }

    #[allow(dead_code)]
    fn ensure_active_profile_exists(&self) -> Result<()> {
        if self.get_active_profile().is_some() {
            Ok(())
        } else {
            Err(ProfileError::NoActiveProfile)
        }
    }

    fn filter_profiles_for_device(&self, device_serial: &str, rotation: u32) -> Vec<ProfileRef> {
        self.profiles
            .read()
            .iter()
            .filter(|p| p.read().matches(device_serial, rotation))
            .cloned()
            .collect()
    }

    /// Filter profiles based on device serial and rotation.
    /// Always queries live data from `self.profiles`; does not use the `avail_profiles` cache.
    #[allow(dead_code)]
    pub fn get_avail_profiles(&self, device_serial: &str, rotation: u32) -> Vec<ProfileRef> {
        self.filter_profiles_for_device(device_serial, rotation)
    }

    /// Get names of available profiles for current device and rotation
    pub fn get_avail_profile_names(&self) -> Vec<String> {
        self.avail_profiles
            .iter()
            .map(|p| p.read().name().into())
            .collect()
    }

    /// Get the active profile reference, if any
    pub fn get_active_profile(&self) -> Option<ProfileRef> {
        self.active_profile.load().as_ref().clone()
    }

    /// Get the name of the active profile, if any
    pub fn get_active_profile_name(&self) -> Option<String> {
        self.get_active_profile()
            .as_ref()
            .map(|p| p.read().name().into())
    }

    /// Get the index of the active profile in the available profiles list, if any
    pub fn get_active_profile_idx(&self) -> Option<usize> {
        let active_name = self.get_active_profile_name()?;
        self.avail_profiles
            .iter()
            .position(|p| p.read().name() == active_name)
    }

    /// Get a profile reference by its index in the available profiles list
    #[allow(dead_code)]
    pub fn get_profile_by_idx(&self, idx: usize) -> Option<ProfileRef> {
        if idx < self.avail_profiles.len() {
            Some(self.avail_profiles[idx].clone())
        } else {
            None
        }
    }

    /// Get a profile reference by its name in the available profiles list
    #[allow(dead_code)]
    pub fn get_profile_by_name(&self, profile_name: &str) -> Option<ProfileRef> {
        self.avail_profiles
            .iter()
            .find(|p| p.read().name() == profile_name)
            .cloned()
    }

    /// Get the name of a profile by its index in the available profiles list
    #[allow(dead_code)]
    pub fn get_profile_name_by_idx(&self, idx: usize) -> Option<String> {
        self.get_profile_by_idx(idx)
            .as_ref()
            .map(|p| p.read().name().into())
    }

    /// Get the index of a profile by its name in the available profiles list
    pub fn get_profile_idx_by_name(&self, profile_name: &str) -> Option<usize> {
        self.avail_profiles
            .iter()
            .position(|p| p.read().name() == profile_name)
    }

    /// Add a new profile to the config, ensuring the name is unique
    pub fn add_profile(&self, profile: Profile) -> Result<()> {
        self.ensure_profile_not_exists(profile.name())?;

        let mut profiles = self.profiles.write();
        profiles.push(Arc::new(RwLock::new(profile)));

        Ok(())
    }

    /// Rename the active profile, ensuring the new name is unique and there is an active profile
    pub fn rename_active_profile(&self, new_name: &str) -> Result<()> {
        self.ensure_profile_not_exists(new_name)?;

        self.get_active_profile()
            .ok_or(ProfileError::NoActiveProfile)?
            .write()
            .rename(new_name);

        Ok(())
    }

    /// Rename a profile by its current name, ensuring the new name is unique and the old profile
    /// exists
    #[allow(dead_code)]
    pub fn rename_profile_by_name(&self, old_name: &str, new_name: &str) -> Result<()> {
        self.ensure_profile_exists(old_name)?;
        self.ensure_profile_not_exists(new_name)?;

        let profiles = self.profiles.write();
        if let Some(profile) = profiles.iter().find(|p| p.read().name() == old_name) {
            profile.write().rename(new_name);
            Ok(())
        } else {
            Err(ProfileError::ProfileNotExist)
        }
    }

    /// Rename a profile by its index in the full profiles list, ensuring the new name is
    /// unique and the index is valid
    #[allow(dead_code)]
    pub fn rename_profile_by_idx(&self, idx: usize, new_name: &str) -> Result<()> {
        self.ensure_profile_not_exists(new_name)?;

        let profiles = self.profiles.read();
        if idx < profiles.len() {
            profiles[idx].write().rename(new_name);
            Ok(())
        } else {
            Err(ProfileError::ProfileNotExist)
        }
    }

    /// Remove the active profile from the config, ensuring there is an active profile
    pub fn remove_active_profile(&self) -> Result<()> {
        let active_profile = self
            .get_active_profile()
            .ok_or(ProfileError::NoActiveProfile)?;

        self.remove_profile_by_name(active_profile.read().name())?;
        self.active_profile.store(Arc::new(None));
        Ok(())
    }

    /// Remove a profile by its index in the full profiles list, ensuring the index is valid
    #[allow(dead_code)]
    pub fn remove_profile_by_idx(&self, idx: usize) -> Result<()> {
        let mut profiles = self.profiles.write();
        if idx < profiles.len() {
            // If the removed profile is currently active, clear the active profile reference
            if let Some(active) = self.get_active_profile()
                && Arc::ptr_eq(&profiles[idx], &active)
            {
                self.active_profile.store(Arc::new(None));
            }
            profiles.remove(idx);
            Ok(())
        } else {
            Err(ProfileError::ProfileNotExist)
        }
    }

    /// Remove a profile by its name, ensuring the profile exists
    pub fn remove_profile_by_name(&self, profile_name: &str) -> Result<()> {
        self.ensure_profile_exists(profile_name)?;

        // If the removed profile is currently active, clear the active profile reference
        if let Some(active) = self.get_active_profile()
            && active.read().name() == profile_name
        {
            self.active_profile.store(Arc::new(None));
        }

        self.profiles
            .write()
            .retain(|profile| profile.read().name() != profile_name);

        Ok(())
    }

    /// Save the active profile as a new profile with the given name,
    /// ensuring there is an active profile and the new name is unique
    pub fn save_active_profile_as(&self, new_name: &str) -> Result<()> {
        self.ensure_profile_not_exists(new_name)?;

        let active_profile = self
            .get_active_profile()
            .ok_or(ProfileError::NoActiveProfile)?;

        let mut new_profile = active_profile.read().clone();
        new_profile.rename(new_name);

        self.add_profile(new_profile)
    }

    /// Add or update a key mapping in the active profile
    pub fn add_mapping(&self, key: Key, action: MappingAction) -> Result<()> {
        let active_profile = self
            .get_active_profile()
            .ok_or(ProfileError::NoActiveProfile)?;

        active_profile.write().add_mapping(key, action);
        Ok(())
    }

    /// Remove a key mapping from the active profile
    pub fn remove_mapping(&self, key: &Key) -> Result<()> {
        let active_profile = self
            .get_active_profile()
            .ok_or(ProfileError::NoActiveProfile)?;

        active_profile.write().remove_mapping(key);
        Ok(())
    }

    /// Clear all key mappings from the active profile
    #[allow(dead_code)]
    pub fn clear_mappings(&self) -> Result<()> {
        let active_profile = self
            .get_active_profile()
            .ok_or(ProfileError::NoActiveProfile)?;

        active_profile.write().clear_mappings();
        Ok(())
    }

    /// Switch to the next profile in the available profiles list, wrapping around to the first
    pub fn switch_profile_next(&self) -> Result<()> {
        let avail_count = self.avail_profiles.len();
        if avail_count <= 1 {
            return Err(ProfileError::ProfileNotExist);
        }

        match self.get_active_profile_idx() {
            Some(idx) => {
                let next_idx = (idx + 1) % avail_count;
                self.active_profile
                    .store(Arc::new(Some(self.avail_profiles[next_idx].clone())));

                Ok(())
            }
            None => Err(ProfileError::NoActiveProfile),
        }
    }

    /// Switch to the previous profile in the available profiles list, wrapping around to the last
    pub fn switch_profile_prev(&self) -> Result<()> {
        let avail_count = self.avail_profiles.len();
        if avail_count <= 1 {
            return Err(ProfileError::ProfileNotExist);
        }

        match self.get_active_profile_idx() {
            Some(idx) => {
                let prev_idx = (idx + avail_count - 1) % avail_count;
                self.active_profile
                    .store(Arc::new(Some(self.avail_profiles[prev_idx].clone())));

                Ok(())
            }
            None => Err(ProfileError::NoActiveProfile),
        }
    }

    /// Switch to a profile by its index in the available profiles list, ensuring the index is valid
    pub fn switch_to_profile(&self, idx: usize) -> Result<()> {
        let avail_count = self.avail_profiles.len();
        if idx >= avail_count {
            return Err(ProfileError::ProfileNotExist);
        }

        self.active_profile
            .store(Arc::new(Some(self.avail_profiles[idx].clone())));

        Ok(())
    }

    /// Switch to a profile by its name in the available profiles list, ensuring the profile exists
    pub fn switch_to_profile_by_name(&self, profile_name: &str) -> Result<()> {
        if let Some(idx) = self.get_profile_idx_by_name(profile_name) {
            self.switch_to_profile(idx)
        } else {
            Err(ProfileError::ProfileNotExist)
        }
    }

    /// Update profiles state based on current device serial and rotation
    ///
    /// # Parameters
    /// - `device_serial`: current device serial
    /// - `device_rotation`: current device rotation (0, 1, 2, 3)
    pub fn update(&mut self, device_serial: &str, device_rotation: u32) {
        let avail_profiles: Vec<ProfileRef> =
            self.filter_profiles_for_device(device_serial, device_rotation);

        if avail_profiles.is_empty() {
            self.active_profile.store(Arc::new(None));
            self.avail_profiles.clear();
        } else {
            self.avail_profiles = avail_profiles;
            let idx = self
                .get_active_profile_name()
                .and_then(|name| self.get_profile_idx_by_name(&name))
                .unwrap_or(0);

            self.active_profile
                .store(Arc::new(Some(self.avail_profiles[idx].clone())));
        }
    }
}

fn ensure_profile_name(profile_name: &str) -> Result<()> {
    if profile_name.trim().is_empty() {
        Err(ProfileError::ProfileFormatError)
    } else {
        Ok(())
    }
}
