//! Modal dialog implementation using egui
//!
//! Provides a reusable modal dialog component with customizable title, message,
//! confirm and cancel buttons, and optional key capture functionality.

use {
    crate::t,
    egui::{Key, Modal, Modifiers},
    std::{cell::RefCell, collections::HashMap},
};

const LIST_MAX_HEIGHT: f32 = 300.0;
const BUTTON_SIZE: (f32, f32) = (80.0, 30.0);

#[derive(PartialEq)]
pub enum ButtonState {
    None,
    Confirm,
    Cancelled,
}

pub enum WidgetKind {
    Message(String),
    TextInput {
        placeholder: Option<String>,
        text: RefCell<String>,
        required: bool,
    },
    ListSelection {
        items: Vec<String>,
        selected_idx: RefCell<usize>,
    },
}

impl WidgetKind {
    fn validate(&self) -> bool {
        match self {
            Self::TextInput { text, required, .. } => {
                !(*required && text.borrow().trim().is_empty())
            }
            _ => true,
        }
    }

    fn state(&self) -> WidgetState {
        match self {
            Self::TextInput { text, .. } => WidgetState::TextInput(text.borrow().clone()),
            Self::ListSelection { selected_idx, .. } => {
                WidgetState::ListSelection(*selected_idx.borrow())
            }
            _ => WidgetState::None,
        }
    }
}

pub struct Widget {
    pub id: Option<String>,
    pub kind: WidgetKind,
}

pub enum WidgetState {
    None,
    TextInput(String),
    ListSelection(usize),
}

pub enum DialogBody {
    None,
    KeyCapture(String),
    Widgets(Vec<Widget>),
}

impl DialogBody {
    fn validate(&self) -> bool {
        match self {
            DialogBody::None => true,
            DialogBody::KeyCapture(_) => true,
            DialogBody::Widgets(widgets) => widgets.iter().all(|widget| widget.kind.validate()),
        }
    }

    fn state(&self) -> HashMap<String, WidgetState> {
        let mut states = HashMap::new();
        if let DialogBody::Widgets(widgets) = self {
            for widget in widgets {
                if let Some(id) = &widget.id {
                    states.insert(id.clone(), widget.kind.state());
                }
            }
        }
        states
    }
}

pub enum DialogState {
    None,
    Cancelled,
    CapturedKey(Key),
    WidgetsState(HashMap<String, WidgetState>),
}

pub struct ModalDialog {
    pub id: String,
    pub title: String,

    pub body: DialogBody,

    pub confirm: Option<String>,
    pub cancel: Option<String>,
}

impl ModalDialog {
    /// Create a new modal dialog, with default confirm and cancel labels
    pub fn new<S>(id: S, title: S) -> Self
    where
        S: Into<String>,
    {
        Self {
            id: id.into(),
            title: title.into(),

            body: DialogBody::None,

            confirm: Some(t!("dialog-button-confirm")),
            cancel: Some(t!("dialog-button-cancel")),
        }
    }

    #[allow(dead_code)]
    /// Set custom confirm button label, or disable confirm button if None
    pub fn with_confirm<S>(mut self, label: Option<S>) -> Self
    where
        S: Into<String>,
    {
        self.set_confirm(label);
        self
    }

    #[allow(dead_code)]
    /// Set custom cancel button label, or disable cancel button if None
    pub fn with_cancel<S>(mut self, label: Option<S>) -> Self
    where
        S: Into<String>,
    {
        self.set_cancel(label);
        self
    }

    /// Enable or disable key capture mode
    pub fn with_key_capture<S>(mut self, message: S) -> Self
    where
        S: Into<String>,
    {
        self.set_key_capture(message);
        self
    }

    pub fn button_count(&self) -> usize {
        self.confirm.is_some() as usize + self.cancel.is_some() as usize
    }

    #[allow(dead_code)]
    /// Set the dialog title
    pub fn set_title<S>(&mut self, title: S) -> &mut Self
    where
        S: Into<String>,
    {
        self.title = title.into();
        self
    }

    #[allow(dead_code)]
    /// Set custom confirm button label, or disable confirm button if None
    pub fn set_confirm<S>(&mut self, label: Option<S>) -> &mut Self
    where
        S: Into<String>,
    {
        match self.body {
            DialogBody::KeyCapture(_) => {
                // Do nothing, cannot set confirm button in key capture mode
            }
            _ => {
                self.confirm = label.map(|s| s.into());
            }
        }
        self
    }

    #[allow(dead_code)]
    /// Set custom cancel button label, or disable cancel button if None
    pub fn set_cancel<S>(&mut self, label: Option<S>) -> &mut Self
    where
        S: Into<String>,
    {
        match self.body {
            DialogBody::KeyCapture(_) => {
                // Do nothing, cannot set cancel button in key capture mode
            }
            _ => {
                self.cancel = label.map(|s| s.into());
            }
        }
        self
    }

    #[allow(dead_code)]
    /// Enable or disable key capture mode, which captures key input
    /// and disables confirm/cancel buttons
    pub fn set_key_capture<S>(&mut self, message: S) -> &mut Self
    where
        S: Into<String>,
    {
        self.confirm = None;
        self.cancel = None;

        self.body = DialogBody::KeyCapture(message.into());
        self
    }

    /// Add text message to the dialog
    pub fn add_message<S>(&mut self, message: S) -> &mut Self
    where
        S: Into<String>,
    {
        match &mut self.body {
            DialogBody::None => {
                self.body = DialogBody::Widgets(vec![Widget {
                    id: None,
                    kind: WidgetKind::Message(message.into()),
                }]);
            }
            DialogBody::Widgets(widgets) => {
                widgets.push(Widget {
                    id: None,
                    kind: WidgetKind::Message(message.into()),
                });
            }
            DialogBody::KeyCapture(_) => {
                // Do nothing, cannot add text in key capture mode
            }
        }
        self
    }

    /// Add text input widget with optional placeholder and initial value
    pub fn add_text_input<S>(
        &mut self,
        id: S,
        placeholder: Option<S>,
        value: Option<S>,
        required: bool,
    ) -> &mut Self
    where
        S: Into<String>,
    {
        let widget = Widget {
            id: Some(id.into()),
            kind: WidgetKind::TextInput {
                placeholder: placeholder.map(|s| s.into()),
                text: RefCell::new(value.map(|s| s.into()).unwrap_or_default()),
                required,
            },
        };

        match &mut self.body {
            DialogBody::None => {
                self.body = DialogBody::Widgets(vec![widget]);
            }
            DialogBody::Widgets(widgets) => {
                widgets.push(widget);
            }
            DialogBody::KeyCapture(_) => {
                // Do nothing, cannot add text input in key capture mode
            }
        }

        self
    }

    /// Add list selection widget with items and optional selected index
    pub fn add_list_selection<S, I>(&mut self, id: S, items: I, selected_idx: usize) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let widget = Widget {
            id: Some(id.into()),
            kind: WidgetKind::ListSelection {
                items: items.into_iter().map(|s| s.into()).collect(),
                selected_idx: RefCell::new(selected_idx),
            },
        };

        match &mut self.body {
            DialogBody::None => {
                self.body = DialogBody::Widgets(vec![widget]);
            }
            DialogBody::Widgets(widgets) => {
                widgets.push(widget);
            }
            DialogBody::KeyCapture(_) => {
                // Do nothing, cannot add list selection in key capture mode
            }
        }

        self
    }

    /// Draw the modal dialog, returning the result
    pub fn draw(&mut self, ctx: &egui::Context) -> DialogState {
        let mut state = DialogState::None;

        Modal::new(self.id.clone().into()).show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = 10.0;

                ui.heading(self.title.as_str());

                // Draw dialog content
                match &self.body {
                    DialogBody::None => {}
                    DialogBody::KeyCapture(message) => {
                        ui.label(message.as_str());
                    }
                    DialogBody::Widgets(widgets) => {
                        self.draw_widgets(ui, widgets);
                    }
                }

                let confirm_enabled = self.body.validate();
                let button_state = self.draw_buttons(ui, confirm_enabled);
                let mut confirm = button_state == ButtonState::Confirm;

                ui.input_mut(|input| {
                    for event in &input.events {
                        if let egui::Event::Key {
                            key,
                            pressed: true,
                            repeat: false,
                            modifiers,
                            ..
                        } = event
                            && modifiers.is_none()
                        {
                            match *key {
                                egui::Key::Escape => {
                                    state = DialogState::Cancelled;
                                }
                                egui::Key::Enter => {
                                    if confirm_enabled {
                                        confirm = true;
                                    }
                                }
                                _ => {
                                    if let DialogBody::KeyCapture(_) = &self.body {
                                        state = DialogState::CapturedKey(*key);
                                    }
                                }
                            }

                            input.consume_key(Modifiers::NONE, *key);
                            break;
                        }
                    }
                });

                if button_state == ButtonState::Cancelled {
                    state = DialogState::Cancelled;
                } else if confirm {
                    state = DialogState::WidgetsState(self.body.state());
                }
            });
        });

        state
    }

    fn draw_widgets(&self, ui: &mut egui::Ui, widgets: &[Widget]) {
        for widget in widgets {
            match &widget.kind {
                WidgetKind::Message(message) => {
                    ui.label(message.as_str());
                }
                WidgetKind::TextInput {
                    placeholder, text, ..
                } => {
                    self.draw_text_input(ui, placeholder.clone(), &mut text.borrow_mut());
                }
                WidgetKind::ListSelection {
                    items,
                    selected_idx,
                } => {
                    self.draw_list_selection(ui, items, &mut selected_idx.borrow_mut());
                }
            }
        }
    }

    fn draw_input(&self, ui: &mut egui::Ui, value: &mut String) {
        let response = ui.text_edit_singleline(value);

        // Auto-focus the text input
        response.request_focus();
    }

    fn draw_text_input(&self, ui: &mut egui::Ui, placeholder: Option<String>, text: &mut String) {
        if let Some(placeholder) = placeholder {
            ui.horizontal_centered(|ui| {
                ui.label(placeholder.as_str());
            });
            self.draw_input(ui, text);
        } else {
            self.draw_input(ui, text);
        }
    }

    fn draw_list_selection(&self, ui: &mut egui::Ui, items: &[String], selected_idx: &mut usize) {
        egui::ScrollArea::vertical()
            .max_height(LIST_MAX_HEIGHT)
            .show(ui, |ui| {
                for (idx, item) in items.iter().enumerate() {
                    let is_selected = idx == *selected_idx;
                    if ui.selectable_label(is_selected, item.as_str()).clicked() {
                        *selected_idx = idx;
                    }
                }
            });
    }

    fn draw_buttons(&mut self, ui: &mut egui::Ui, confirm_enabled: bool) -> ButtonState {
        let button_count = self.button_count();
        if button_count == 0 {
            return ButtonState::None;
        }

        ui.separator();

        let mut state = ButtonState::None;

        // Confirm and Cancel buttons
        ui.horizontal(|ui| {
            let spacing = 20.0;
            let total_width = BUTTON_SIZE.0 * button_count as f32
                + spacing * (button_count.saturating_sub(1)) as f32;
            let offset = (ui.available_width() - total_width) / 2.0;
            ui.add_space(offset.max(0.0));

            if let Some(confirm_label) = &self.confirm {
                if ui
                    .add_enabled(
                        confirm_enabled,
                        egui::Button::new(confirm_label.as_str()).min_size(BUTTON_SIZE.into()),
                    )
                    .clicked()
                {
                    state = ButtonState::Confirm;
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
                state = ButtonState::Cancelled;
            }
        });

        state
    }
}
