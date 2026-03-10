use {
    super::SAideApp,
    crate::{core::coords::MappingPos, modal::ModalDialog, t, tf},
};

impl SAideApp {
    pub fn show_help_dialog(&mut self) {
        if self.dialog.is_none() {
            let mut dialog = ModalDialog::new("help_dialog", &t!("mapping-config-help-title"));
            dialog.add_message(&t!("mapping-config-help-message"));
            self.dialog.replace(dialog);
        }
    }

    pub fn show_profile_selection_dialog(&mut self) {
        if self.dialog.is_none() {
            let profiles = self.profile_manager.get_avail_profile_names();
            if profiles.is_empty() {
                self.notify(&t!("notification-no-profiles"));
                return;
            }

            let idx = self.profile_manager.get_active_profile_idx().unwrap_or(0);

            let mut dialog =
                ModalDialog::new("switch_profile_dialog", &t!("editor-dialog-switch-title"));
            dialog.add_list_selection("profile", profiles.iter().map(|s| s.as_str()), idx);

            self.dialog.replace(dialog);
        }
    }

    pub fn show_rename_profile_dialog(&mut self) {
        if self.dialog.is_none() {
            let Some(current_name) = self.profile_manager.get_active_profile_name() else {
                self.notify(&t!("notification-no-active-profile"));
                return;
            };

            let mut dialog = ModalDialog::new("rename_dialog", &t!("editor-dialog-rename-title"));
            dialog.add_text_input(
                "name",
                Some(&t!("editor-dialog-rename-placeholder")),
                Some(&current_name),
                true,
            );

            self.dialog.replace(dialog);
        }
    }

    pub fn show_create_profile_dialog(&mut self) {
        if self.dialog.is_none() {
            let mut dialog = ModalDialog::new("new_profile_dialog", &t!("editor-dialog-new-title"));
            let placeholder = t!("editor-dialog-new-placeholder");
            dialog.add_text_input("name", Some(placeholder.as_str()), None, true);

            self.dialog.replace(dialog);
        }
    }

    pub fn show_delete_profile_dialog(&mut self) {
        if self.dialog.is_none() {
            let Some(current_name) = self.profile_manager.get_active_profile_name() else {
                self.notify(&t!("notification-no-active-profile"));
                return;
            };

            let mut dialog = ModalDialog::new(
                "delete_profile_dialog",
                &t!("editor-dialog-delete-profile-title"),
            );
            let name = current_name.clone();
            dialog
                .add_message(&tf!("editor-dialog-delete-profile-message", "name" => name.as_str()));

            self.dialog.replace(dialog);
        }
    }

    pub fn show_save_profile_as_dialog(&mut self) {
        if self.dialog.is_none() {
            let Some(current_name) = self.profile_manager.get_active_profile_name() else {
                self.notify(&t!("notification-no-active-profile"));
                return;
            };

            let mut dialog =
                ModalDialog::new("save_as_profile_dialog", &t!("editor-dialog-saveas-title"));
            dialog.add_text_input(
                "name",
                Some(&t!("editor-dialog-saveas-placeholder")),
                Some(&current_name),
                true,
            );

            self.dialog.replace(dialog);
        }
    }

    pub fn show_profile_exists_dialog(&mut self, profile_name: &str) {
        self.show_error_dialog_inner(
            &t!("editor-dialog-error-profile-exists-title"),
            &tf!("editor-dialog-error-profile-exists-message", "profile_name" => profile_name),
        );
    }

    pub fn show_error_dialog(&mut self, title: &str, error_msg: &str) {
        self.show_error_dialog_inner(title, error_msg);
    }

    fn show_error_dialog_inner(&mut self, title: &str, error_msg: &str) {
        if self.dialog.is_none() {
            let mut dialog =
                ModalDialog::new("saide_error_dialog", title).with_cancel::<String>(None);
            dialog.add_message(error_msg);

            self.dialog.replace(dialog);
        }
    }

    pub fn show_add_mapping_dialog(&mut self, mapping_pos: &MappingPos) {
        if self.dialog.is_none() {
            let x = format!("{:.1}%", mapping_pos.x * 100.0);
            let y = format!("{:.1}%", mapping_pos.y * 100.0);
            let dialog = ModalDialog::new("add_mapping_dialog", &t!("editor-dialog-create-title"))
                .with_key_capture(&tf!(
                    "editor-dialog-create-message",
                    "x" => x.as_str(),
                    "y" => y.as_str()
                ));
            self.dialog.replace(dialog);
        }
    }

    pub fn show_delete_mapping_dialog(&mut self, mapping_pos: &MappingPos, key: &str) {
        if self.dialog.is_none() {
            let x = format!("{:.1}%", mapping_pos.x * 100.0);
            let y = format!("{:.1}%", mapping_pos.y * 100.0);
            let mut dialog =
                ModalDialog::new("delete_mapping_dialog", &t!("editor-dialog-delete-title"));
            dialog.add_message(&tf!(
                "editor-dialog-delete-message",
                "key" => key,
                "x" => x.as_str(),
                "y" => y.as_str()
            ));
            self.dialog.replace(dialog);
        }
    }
}
