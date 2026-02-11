use {
    super::SAideApp,
    crate::{
        modal::{DialogState, ModalDialog, WidgetState},
        t,
        tf,
    },
    egui::Context,
    egui_action::ActionArgs,
    egui_action_macro::impl_action,
};

#[impl_action]
impl SAideApp {
    pub fn show_help_dialog(&mut self, ctx: &Context) {
        if self.dialog.is_none() {
            let mut dialog = ModalDialog::new("help_dialog", &t!("mapping-config-help-title"));
            dialog.add_message(&t!("mapping-config-help-message"));
            self.dialog.replace(dialog);
        }

        self.draw_dialog(ctx);
    }

    pub fn show_profile_selection_dialog(&mut self, ctx: &Context) -> Option<usize> {
        if self.dialog.is_none() {
            let profiles = self.profile_manager.get_avail_profile_names();
            if profiles.is_empty() {
                self.notify(&t!("notification-no-profiles"));
                return None;
            }

            let idx = self.profile_manager.get_active_profile_idx().unwrap_or(0);

            let mut dialog = ModalDialog::new(
                "switch_profile_dialog",
                &t!("mapping-config-dialog-switch-title"),
            );
            dialog.add_list_selection("profile", profiles.iter().map(|s| s.as_str()), idx);

            self.dialog.replace(dialog);
        }

        match self.draw_dialog(ctx) {
            DialogState::WidgetsState(states) => {
                if let Some(WidgetState::ListSelection(idx)) = states.get("profile") {
                    Some(*idx)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub fn show_rename_profile_dialog(&mut self, ctx: &Context) -> Option<String> {
        if self.dialog.is_none() {
            let Some(current_name) = self.profile_manager.get_active_profile_name() else {
                self.notify(&t!("notification-no-active-profile"));
                return None;
            };

            let mut dialog = ModalDialog::new("rename_dialog", &t!("mapping-config-rename-title"));
            dialog.add_text_input(
                "name",
                Some(&t!("mapping-config-rename-placeholder")),
                Some(&current_name),
                true,
            );

            self.dialog.replace(dialog);
        }

        match self.draw_dialog(ctx) {
            DialogState::WidgetsState(states) => {
                if let Some(WidgetState::TextInput(name)) = states.get("name") {
                    Some(name.clone())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub fn show_create_profile_dialog(&mut self, ctx: &Context) -> Option<String> {
        if self.dialog.is_none() {
            let mut dialog =
                ModalDialog::new("new_profile_dialog", &t!("mapping-config-dialog-new-title"));
            dialog.add_text_input("name", Some(""), None, true);

            self.dialog.replace(dialog);
        }

        match self.draw_dialog(ctx) {
            DialogState::WidgetsState(states) => {
                if let Some(WidgetState::TextInput(name)) = states.get("name") {
                    Some(name.clone())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub fn show_delete_profile_dialog(&mut self, ctx: &Context) -> bool {
        if self.dialog.is_none() {
            if self.profile_manager.get_active_profile().is_none() {
                self.notify(&t!("notification-no-active-profile"));
                return false;
            }

            let mut dialog = ModalDialog::new(
                "delete_profile_dialog",
                &t!("mapping-config-dialog-delete-title"),
            );
            dialog.add_message(&t!("mapping-config-dialog-delete-message"));

            self.dialog.replace(dialog);
        }

        matches!(self.draw_dialog(ctx), DialogState::Comfirmed)
    }

    pub fn show_save_profile_as_dialog(&mut self, ctx: &Context) -> Option<String> {
        if self.dialog.is_none() {
            let Some(current_name) = self.profile_manager.get_active_profile_name() else {
                self.notify(&t!("notification-no-active-profile"));
                return None;
            };

            let mut dialog = ModalDialog::new(
                "save_as_profile_dialog",
                &t!("mapping-config-dialog-saveas-title"),
            );
            dialog.add_text_input(
                "name",
                Some(&t!("mapping-config-dialog-saveas-placeholder")),
                Some(&current_name),
                true,
            );

            self.dialog.replace(dialog);
        }

        match self.draw_dialog(ctx) {
            DialogState::WidgetsState(states) => {
                if let Some(WidgetState::TextInput(name)) = states.get("name") {
                    Some(name.clone())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub fn show_profile_exists_dialog(&mut self, ctx: &Context, profile_name: &str) {
        self.show_error_dialog_inner(
            ctx,
            &t!("mapping-config-error-profile-exists-title"),
            &tf!("mapping-config-error-profile-exists-message", "profile_name" => profile_name),
        );
    }

    pub fn show_error_dialog(&mut self, ctx: &Context, title: &str, error_msg: &str) {
        self.show_error_dialog_inner(ctx, title, error_msg);
    }

    fn show_error_dialog_inner(&mut self, ctx: &Context, title: &str, error_msg: &str) {
        if self.dialog.is_none() {
            let mut dialog =
                ModalDialog::new("saide_error_dialog", title).with_cancel::<String>(None);
            dialog.add_message(error_msg);

            self.dialog.replace(dialog);
        }

        self.draw_dialog(ctx);
    }
}
