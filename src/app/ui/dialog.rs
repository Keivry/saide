//! Modal dialog implementation using egui
//!
//! Provides a reusable modal dialog component with customizable title, message,
//! confirm and cancel buttons, and optional key capture functionality.

use egui::{Key, Modal, Modifiers};

const BUTTON_SIZE: (f32, f32) = (80.0, 30.0);

pub enum ModalDialogResult {
    None,
    Confirmed,
    Canceled,
    CapturedKey(Key),
}

pub struct ModalDialog {
    pub visible: bool,

    pub id: String,
    pub title: String,
    pub message: String,
    pub confirm: Option<String>,
    pub cancel: Option<String>,

    /// Whether to capture key input
    pub capture: bool,
}

impl ModalDialog {
    /// Create a new modal dialog, with default confirm and cancel labels
    pub fn new(id: &str) -> Self {
        Self {
            visible: false,

            id: id.into(),
            title: String::new(),
            message: String::new(),
            confirm: Some("Confirm".into()),
            cancel: Some("Cancel".into()),

            capture: false,
        }
    }

    /// Set custom confirm button label, or disable confirm button if None
    #[allow(dead_code)]
    pub fn with_confirm(mut self, label: Option<&str>) -> Self {
        self.confirm = label.map(|s| s.into());
        self
    }

    /// Set custom confirm button label, or disable confirm button if None
    #[allow(dead_code)]
    pub fn set_confirm(&mut self, label: Option<&str>) -> &mut Self {
        self.confirm = label.map(|s| s.into());
        self
    }

    /// Set custom cancel button label, or disable cancel button if None
    #[allow(dead_code)]
    pub fn with_cancel(mut self, label: Option<&str>) -> Self {
        self.cancel = label.map(|s| s.into());
        self
    }

    /// Set custom cancel button label, or disable cancel button if None
    #[allow(dead_code)]
    pub fn set_cancel(&mut self, label: Option<&str>) -> &mut Self {
        self.cancel = label.map(|s| s.into());
        self
    }

    /// Enable or disable key capture mode, which captures key input and disables confirm/cancel
    /// buttons
    pub fn with_key_capture(mut self, enable: bool) -> Self {
        self.confirm = None;
        self.cancel = None;
        self.capture = enable;
        self
    }

    /// Enable or disable key capture mode, which captures key input and disables confirm/cancel
    #[allow(dead_code)]
    pub fn set_key_capture(&mut self, enable: bool) -> &mut Self {
        if enable {
            self.confirm = None;
            self.cancel = None;
        }
        self.capture = enable;
        self
    }

    /// Set the dialog title
    pub fn with_title(mut self, title: &str) -> Self {
        self.title = title.into();
        self
    }

    /// Set the dialog title
    #[allow(dead_code)]
    pub fn set_title(&mut self, title: &str) -> &mut Self {
        self.title = title.into();
        self
    }

    /// Set the dialog message
    #[allow(dead_code)]
    pub fn with_message(mut self, message: &str) -> Self {
        self.message = message.into();
        self
    }

    /// Set the dialog message
    pub fn set_message(&mut self, message: &str) -> &mut Self {
        self.message = message.into();
        self
    }

    /// Set the visibility of the dialog
    pub fn set_visible(&mut self, visible: bool) -> &mut Self {
        self.visible = visible;
        self
    }

    /// Reset the dialog to hidden state
    pub fn reset(&mut self) { self.visible = false; }

    pub fn is_visible(&self) -> bool { self.visible }

    pub fn button_count(&self) -> usize {
        self.confirm.is_some() as usize + self.cancel.is_some() as usize
    }

    /// Draw the modal dialog, returning the result
    pub fn show(&mut self, ctx: &egui::Context) -> ModalDialogResult {
        if !self.visible {
            return ModalDialogResult::None;
        }

        let mut result = ModalDialogResult::None;

        Modal::new(self.id.clone().into()).show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = 10.0;

                // Draw dialog content
                ui.label(self.title.as_str());
                ui.separator();
                ui.label(self.message.as_str());

                // Draw buttons if any
                let button_count = self.button_count();
                if button_count > 0 {
                    ui.separator();

                    // Confirm and Cancel buttons
                    ui.horizontal(|ui| {
                        let spacing = 20.0;
                        let total_width = BUTTON_SIZE.0 * button_count as f32
                            + spacing * (button_count.saturating_sub(1)) as f32;
                        let offset = (ui.available_width() - total_width) / 2.0;
                        ui.add_space(offset.max(0.0));

                        if let Some(confirm_label) = &self.confirm {
                            if ui
                                .add_sized(BUTTON_SIZE, egui::Button::new(confirm_label.as_str()))
                                .clicked()
                            {
                                result = ModalDialogResult::Confirmed;
                            }
                            if self.cancel.is_some() {
                                ui.add_space(spacing);
                            }
                        }

                        if let Some(cancel_label) = &self.cancel
                            && ui
                                .add_sized(BUTTON_SIZE, egui::Button::new(cancel_label.as_str()))
                                .clicked()
                        {
                            result = ModalDialogResult::Canceled;
                        }
                    });
                }

                // Capture key input if in capture mode
                // or capture ESC key to cancel
                ui.input_mut(|input| {
                    for event in &input.events {
                        if let egui::Event::Key {
                            key,
                            pressed: true,
                            repeat: false,
                            modifiers,
                            ..
                        } = event
                            // Only capture non-modifier keys
                            && modifiers.is_none()
                        {
                            if *key == egui::Key::Escape {
                                // Cancel input
                                result = ModalDialogResult::Canceled;
                            } else if self.capture {
                                // Capture the key
                                result = ModalDialogResult::CapturedKey(*key);
                            }

                            // Prevent further processing
                            input.consume_key(Modifiers::NONE, *key);
                            break;
                        }
                    }
                });
            });
        });

        result
    }
}
