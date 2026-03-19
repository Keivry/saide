// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::{SAideApp, config::mapping::Profile, core::profile_manager::ProfileError, t, tf};

impl SAideApp {
    fn notify_profile_switched(&self) {
        if let Some(profile_name) = self.profile_manager.get_active_profile_name() {
            self.notify(
                &tf!("notification-switch-profile-success", "profile_name" => &profile_name),
            );
        }
    }

    fn profile_error_reason(&self, err: &ProfileError) -> String {
        match err {
            ProfileError::NoActiveProfile => t!("notification-no-active-profile"),
            ProfileError::ProfileNotFound => t!("notification-profile-error-not-found"),
            ProfileError::ProfileConflict => t!("notification-profile-error-name-conflict"),
            ProfileError::ProfileFormatError => t!("notification-profile-error-invalid-format"),
            ProfileError::NoProfileToSwitch => t!("notification-no-profile-to-switch"),
        }
    }

    pub fn create_profile(&mut self, profile_name: &str) {
        let profile = Profile::new(
            profile_name,
            &self.app_state.device_serial,
            self.app_state.display_rotation,
        );

        match self.profile_manager.add_profile(profile) {
            Ok(()) => {
                self.profile_manager.update(
                    &self.app_state.device_serial,
                    self.app_state.display_rotation,
                );
                self.save_config();
                if self
                    .profile_manager
                    .switch_to_profile_by_name(profile_name)
                    .is_err()
                {
                    self.notify(&t!("notification-switch-profile-failed"));
                } else {
                    self.refresh_mapping_profiles();
                }
            }
            Err(err) => {
                let reason = self.profile_error_reason(&err);
                self.notify(&tf!(
                    "notification-create-profile-failed-with-reason",
                    "reason" => &reason
                ));
            }
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
                self.app_state.display_rotation,
            );
            self.refresh_mapping_profiles();
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

        match self.profile_manager.save_active_profile_as(new_name) {
            Ok(()) => {
                self.profile_manager.update(
                    &self.app_state.device_serial,
                    self.app_state.display_rotation,
                );
                self.save_config();
            }
            Err(err) => {
                let reason = self.profile_error_reason(&err);
                self.notify(&tf!(
                    "notification-save-profile-as-failed-with-reason",
                    "reason" => &reason
                ));
            }
        };
    }

    pub fn rename_profile(&mut self, new_name: &str) {
        if self.profile_manager.get_active_profile().is_none() {
            self.notify(&t!("notification-no-active-profile"));
            return;
        }

        if let Err(err) = self.profile_manager.rename_active_profile(new_name) {
            let reason = self.profile_error_reason(&err);
            self.notify(&tf!(
                "notification-rename-profile-failed-with-reason",
                "reason" => &reason
            ));
        } else {
            self.save_config()
        }
    }

    pub fn switch_profile(&mut self, idx: usize) {
        self.profile_manager
            .switch_to_profile(idx)
            .map(|()| self.refresh_mapping_profiles())
            .unwrap_or_else(|_| {
                self.notify(&t!("notification-switch-profile-failed"));
            });
    }

    pub fn next_profile(&mut self) {
        match self.profile_manager.switch_profile_next() {
            Ok(()) => {
                self.refresh_mapping_profiles();
                self.notify_profile_switched()
            }
            Err(ProfileError::NoProfileToSwitch) => {
                self.notify(&t!("notification-no-profile-to-switch"))
            }
            Err(_) => self.notify(&t!("notification-switch-profile-failed")),
        }
    }

    pub fn prev_profile(&mut self) {
        match self.profile_manager.switch_profile_prev() {
            Ok(()) => {
                self.refresh_mapping_profiles();
                self.notify_profile_switched()
            }
            Err(ProfileError::NoProfileToSwitch) => {
                self.notify(&t!("notification-no-profile-to-switch"))
            }
            Err(_) => self.notify(&t!("notification-switch-profile-failed")),
        }
    }

    pub fn close_mapping_editor(&mut self) { self.mapping_editor.take(); }

    pub fn close_dialog(&mut self) { self.dialog.take(); }
}
