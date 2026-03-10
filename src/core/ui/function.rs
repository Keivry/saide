use crate::{SAideApp, config::mapping::Profile, core::profile_manager::ProfileError, t};

impl SAideApp {
    pub fn create_profile(&mut self, profile_name: &str) {
        let profile = Profile::new(
            profile_name,
            &self.app_state.device_serial,
            self.app_state.device_orientation,
        );

        if self.profile_manager.add_profile(profile).is_ok() {
            self.profile_manager.update(
                &self.app_state.device_serial,
                self.app_state.device_orientation,
            );
            self.save_config();
            if self
                .profile_manager
                .switch_to_profile_by_name(profile_name)
                .is_err()
            {
                self.notify(&t!("notification-switch-profile-failed"));
            }
        } else {
            self.notify(&t!("notification-create-profile-failed"));
        }
    }

    pub fn delete_profile(&mut self) {
        if self.profile_manager.get_active_profile().is_none() {
            self.notify(&t!("notification-no-active-profile"));
            return;
        }

        if self.profile_manager.remove_active_profile().is_ok() {
            self.profile_manager.update(
                &self.app_state.device_serial,
                self.app_state.device_orientation,
            );
            self.save_config();
        } else {
            self.notify(&t!("notification-delete-profile-failed"));
        }
    }

    pub fn save_profile_as(&mut self, new_name: &str) {
        if self.profile_manager.get_active_profile().is_none() {
            self.notify(&t!("notification-no-active-profile"));
            return;
        }

        if self
            .profile_manager
            .save_active_profile_as(new_name)
            .is_ok()
        {
            self.profile_manager.update(
                &self.app_state.device_serial,
                self.app_state.device_orientation,
            );
            self.save_config();
        } else {
            self.notify(&t!("notification-save-profile-as-failed"));
        };
    }

    pub fn rename_profile(&mut self, new_name: &str) {
        if self.profile_manager.get_active_profile().is_none() {
            self.notify(&t!("notification-no-active-profile"));
            return;
        }

        if self.profile_manager.rename_active_profile(new_name).is_ok() {
            self.save_config()
        } else {
            {
                self.notify(&t!("notification-rename-profile-failed"));
            }
        };
    }

    pub fn switch_profile(&mut self, idx: usize) {
        self.profile_manager
            .switch_to_profile(idx)
            .unwrap_or_else(|_| {
                self.notify(&t!("notification-switch-profile-failed"));
            });
    }

    pub fn next_profile(&mut self) {
        match self.profile_manager.switch_profile_next() {
            Ok(()) | Err(ProfileError::NoProfileToSwitch) => {}
            Err(_) => self.notify(&t!("notification-switch-profile-failed")),
        }
    }

    pub fn prev_profile(&mut self) {
        match self.profile_manager.switch_profile_prev() {
            Ok(()) | Err(ProfileError::NoProfileToSwitch) => {}
            Err(_) => self.notify(&t!("notification-switch-profile-failed")),
        }
    }

    pub fn close_mapping_editor(&mut self) { self.mapping_editor.take(); }

    pub fn close_dialog(&mut self) { self.dialog.take(); }
}
