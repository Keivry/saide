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
    tracing::warn,
};

#[impl_action]
impl SAideApp {
    pub fn show_help_dialog(&mut self, ctx: &Context) {
        if !self.is_dialog_open() {
            let mut dialog = ModalDialog::new("help_dialog", &t!("mapping-config-help-title"));
            dialog.add_message(&t!("mapping-config-help-message"));
            self.open_dialog(dialog);
        }

        self.draw_dialog(ctx);
    }

    pub fn show_profile_selection_dialog(&mut self, ctx: &Context) -> Option<usize> {
        if !self.is_dialog_open() {
            let app_state = self.state();
            let Some(keyboard_mapper) = app_state.keyboard_mapper.as_ref() else {
                warn!("No keyboard mapper available");
                return None;
            };

            let profiles = keyboard_mapper.get_avail_profiles();
            if profiles.is_empty() {
                warn!("No profiles available to switch to");
                return None;
            }

            let idx = keyboard_mapper.get_active_profile_idx().unwrap_or(0);

            let mut dialog = ModalDialog::new(
                "switch_profile_dialog",
                &t!("mapping-config-dialog-switch-title"),
            );
            dialog.add_list_selection("profile", profiles.iter().map(|s| s.as_str()), idx);

            self.open_dialog(dialog);
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
        let current_name = self.get_active_profile_name();

        let mut dialog = ModalDialog::new("rename_dialog", &t!("mapping-config-rename-title"));
        dialog.add_text_input(
            "name",
            Some(&t!("mapping-config-rename-placeholder")),
            current_name.as_deref(),
            true,
        );

        match dialog.draw(ctx) {
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
        let mut dialog =
            ModalDialog::new("new_profile_dialog", &t!("mapping-config-dialog-new-title"));
        dialog.add_text_input("name", Some(""), None, true);

        match dialog.draw(ctx) {
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
        let mut dialog = ModalDialog::new(
            "delete_profile_dialog",
            &t!("mapping-config-dialog-delete-title"),
        );
        dialog.add_message(&t!("mapping-config-dialog-delete-message"));

        match dialog.draw(ctx) {
            DialogState::Confirmed => true,
            _ => false,
        }
    }

    pub fn show_save_profile_as_dialog(&mut self, ctx: &Context) -> Option<String> {
        let Some(current_name) = self.get_active_profile_name() else {
            warn!("No active profile to save as");
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

        match dialog.draw(ctx) {
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
        show_error_dialog_inner(
            ctx,
            &t!("mapping-config-error-profile-exists-title"),
            &tf!("mapping-config-error-profile-exists-message", "profile_name" => profile_name),
        );
    }

    pub fn show_error_dialog(&mut self, ctx: &Context, title: &str, error_msg: &str) {
        show_error_dialog_inner(ctx, &title, &error_msg);
    }
}

fn show_error_dialog_inner(ctx: &Context, title: &str, error_msg: &str) {
    let mut dialog = ModalDialog::new("saide_error_dialog", title).with_cancel::<String>(None);
    dialog.add_message(error_msg);

    dialog.draw(ctx);
}
