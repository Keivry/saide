// SPDX-License-Identifier: MIT OR Apache-2.0

//! Toolbar UI component
//!
//! Provides a vertical toolbar with buttons for common actions
//! such as rotating video, configuring mappings, and toggling screen power.

use {
    crate::{
        core::{state::ToolbarMode, ui::AppCommand},
        t,
    },
    egui::{Button, RichText},
    lazy_static::lazy_static,
};

const TOOLBAR_WIDTH: f32 = 42.0;
const TOOLBAR_BTN_SIZE: [f32; 2] = [36.0, 36.0];
const TOOLBAR_FONT_SIZE: f32 = 16.0;
const TOOLBAR_BTN_SPACING: f32 = 2.0;
const TOOLBAR_SEPARATOR_SPACING: f32 = 10.0;

enum ButtonType {
    Separator,
    Normal,
    SelectableConditional {
        is_selected: fn(&ToolbarViewState) -> bool,
        is_enabled: fn(&ToolbarViewState) -> bool,
    },
}

struct ToolbarButton {
    btn_type: ButtonType,
    label: &'static str,
    tooltip_key: &'static str,
    event: AppCommand,
}

lazy_static! {
    static ref TOOLBAR_BUTTONS_BASE: [ToolbarButton; 9] = [
        ToolbarButton {
            btn_type: ButtonType::SelectableConditional {
                is_selected: ToolbarViewState::is_keyboard_mapping_enabled,
                is_enabled: ToolbarViewState::has_active_mappings,
            },
            label: "⌨",
            tooltip_key: "toolbar-toggle-keyboard-mapping",
            event: AppCommand::ToggleKeyboardMapping,
        },
        ToolbarButton {
            btn_type: ButtonType::SelectableConditional {
                is_selected: ToolbarViewState::is_mapping_visualization_enabled,
                is_enabled: ToolbarViewState::has_active_mappings,
            },
            label: "👁",
            tooltip_key: "toolbar-mapping-visualization",
            event: AppCommand::ToggleMappingVisualization,
        },
        ToolbarButton {
            btn_type: ButtonType::Normal,
            label: "⚙",
            tooltip_key: "toolbar-editor",
            event: AppCommand::ToggleMappingEditor,
        },
        ToolbarButton {
            btn_type: ButtonType::Separator,
            label: "",
            tooltip_key: "",
            event: AppCommand::ToggleFloat,
        },
        ToolbarButton {
            btn_type: ButtonType::Normal,
            label: "⟳",
            tooltip_key: "toolbar-rotate",
            event: AppCommand::RotateVideo,
        },
        ToolbarButton {
            btn_type: ButtonType::Normal,
            label: "📷",
            tooltip_key: "toolbar-screenshot",
            event: AppCommand::TakeScreenshot,
        },
        ToolbarButton {
            btn_type: ButtonType::SelectableConditional {
                is_selected: ToolbarViewState::is_recording,
                is_enabled: ToolbarViewState::always_enabled,
            },
            label: "⏺",
            tooltip_key: "toolbar-recording",
            event: AppCommand::ToggleRecording,
        },
        ToolbarButton {
            btn_type: ButtonType::Separator,
            label: "",
            tooltip_key: "",
            event: AppCommand::ToggleFloat,
        },
        ToolbarButton {
            btn_type: ButtonType::Normal,
            label: "💤",
            tooltip_key: "toolbar-screen-off",
            event: AppCommand::ToggleScreenPower,
        },
    ];
}

#[derive(Clone, Copy, Debug)]
pub struct ToolbarViewState {
    pub keyboard_mapping_enabled: bool,
    pub mapping_visualization_enabled: bool,
    pub has_active_mappings: bool,
    pub mode: ToolbarMode,
    pub is_recording: bool,
}

impl ToolbarViewState {
    fn is_keyboard_mapping_enabled(&self) -> bool { self.keyboard_mapping_enabled }

    fn is_mapping_visualization_enabled(&self) -> bool { self.mapping_visualization_enabled }

    fn has_active_mappings(&self) -> bool { self.has_active_mappings }

    fn is_recording(&self) -> bool { self.is_recording }

    fn always_enabled(_: &Self) -> bool { true }
}

pub struct Toolbar;

impl Default for Toolbar {
    fn default() -> Self { Self::new() }
}

impl Toolbar {
    pub fn new() -> Self { Self }

    pub fn width() -> f32 { TOOLBAR_WIDTH }

    pub fn draw(&self, ui: &mut egui::Ui, state: ToolbarViewState) -> Option<AppCommand> {
        let count = TOOLBAR_BUTTONS_BASE.len();
        if count == 0 {
            return None;
        }

        let mut result = None;
        let available = ui.available_rect_before_wrap();
        let screen_bottom = ui.ctx().content_rect().max.y;
        let full_rect = egui::Rect::from_min_max(
            available.min,
            egui::pos2(available.max.x, available.max.y.min(screen_bottom)),
        );

        let toggle_btn_height = TOOLBAR_BTN_SIZE[1] + TOOLBAR_BTN_SPACING * 2.0;
        let main_rect = egui::Rect::from_min_max(
            full_rect.min,
            egui::pos2(full_rect.max.x, full_rect.max.y - toggle_btn_height),
        );
        let toggle_rect = egui::Rect::from_min_max(
            egui::pos2(full_rect.min.x, full_rect.max.y - toggle_btn_height),
            full_rect.max,
        );

        ui.scope_builder(egui::UiBuilder::new().max_rect(main_rect), |ui| {
            ui.spacing_mut().item_spacing.y = TOOLBAR_BTN_SPACING;
            ui.vertical_centered(|ui| {
                let desired_height =
                    TOOLBAR_BTN_SIZE[1] * count as f32 + TOOLBAR_BTN_SPACING * (count - 1) as f32;
                let top_padding = ((main_rect.height() - desired_height) / 2.0).max(0.0);
                ui.add_space(top_padding);
                for btn in TOOLBAR_BUTTONS_BASE.iter() {
                    if matches!(btn.btn_type, ButtonType::Separator) {
                        self.draw_separator(ui);
                        continue;
                    }

                    if self.draw_button(btn, ui, state) {
                        result = Some(btn.event);
                    }
                }
            });
        });

        ui.scope_builder(egui::UiBuilder::new().max_rect(toggle_rect), |ui| {
            ui.spacing_mut().item_spacing.y = TOOLBAR_BTN_SPACING;
            ui.vertical_centered(|ui| {
                ui.add_space(TOOLBAR_BTN_SPACING);
                if self.draw_toggle_float_button(ui, state.mode) {
                    result = Some(AppCommand::ToggleFloat);
                }
            });
        });

        result
    }

    fn draw_button(&self, btn: &ToolbarButton, ui: &mut egui::Ui, state: ToolbarViewState) -> bool {
        let mut button = Button::new(RichText::new(btn.label).size(TOOLBAR_FONT_SIZE));

        let (enabled, selected) = match btn.btn_type {
            ButtonType::Normal => (true, false),
            ButtonType::SelectableConditional {
                is_selected,
                is_enabled,
            } => (is_enabled(&state), is_selected(&state)),
            ButtonType::Separator => (false, false), // Should not happen here
        };

        if selected {
            button = button.selected(true);
        }

        let response = ui.add_enabled_ui(enabled, |ui| ui.add_sized(TOOLBAR_BTN_SIZE, button));

        let response = response
            .inner
            .on_hover_text(t!(btn.tooltip_key))
            .on_disabled_hover_text(t!(btn.tooltip_key));

        response.clicked()
    }

    fn draw_separator(&self, ui: &mut egui::Ui) {
        let separator = egui::Separator::default().spacing(TOOLBAR_SEPARATOR_SPACING);
        ui.add(separator);
    }

    fn draw_toggle_float_button(&self, ui: &mut egui::Ui, mode: ToolbarMode) -> bool {
        let (label, tooltip_key) = if mode.is_floating() {
            ("🔒", "toolbar-pin-toolbar")
        } else {
            ("🔓", "toolbar-float-toolbar")
        };

        let button = Button::new(RichText::new(label).size(TOOLBAR_FONT_SIZE));
        let response = ui
            .add_sized(TOOLBAR_BTN_SIZE, button)
            .on_hover_text(t!(tooltip_key));
        response.clicked()
    }
}
