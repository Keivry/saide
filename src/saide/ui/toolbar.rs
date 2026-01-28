//! Toolbar UI component
//!
//! Provides a vertical toolbar with buttons for common actions
//! such as rotating video, configuring mappings, and toggling screen power.

use {
    crate::t,
    egui::{Button, RichText},
    lazy_static::lazy_static,
};

const TOOLBAR_WIDTH: f32 = 42.0;
const TOOLBAR_BTN_SIZE: [f32; 2] = [36.0, 36.0];
const TOOLBAR_FONT_SIZE: f32 = 16.0;
const TOOLBAR_BTN_SPACING: f32 = 2.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToolbarEvent {
    None,
    ToggleKeyboardMapping,
    ToggleMappingVisualization,
    RotateVideo,
    ConfigureMappings,
    ToggleScreenPower,
}

enum ButtonType {
    Normal,
    SelectableConditional {
        is_selected: fn(&Toolbar) -> bool,
        is_enabled: fn(&Toolbar) -> bool,
    },
}

struct ToolbarButton {
    btn_type: ButtonType,
    lable: &'static str,
    tooltip_key: &'static str,
    event: ToolbarEvent,
}

lazy_static! {
    static ref TOOLBAR_BUTTONS_BASE: [ToolbarButton; 5] = [
        ToolbarButton {
            btn_type: ButtonType::SelectableConditional {
                is_selected: Toolbar::is_keyboard_mapping_enabled,
                is_enabled: Toolbar::has_active_mappings,
            },
            lable: "⌨",
            tooltip_key: "toolbar-keyboard-mapping",
            event: ToolbarEvent::ToggleKeyboardMapping,
        },
        ToolbarButton {
            btn_type: ButtonType::SelectableConditional {
                is_selected: Toolbar::is_mapping_visualization_enabled,
                is_enabled: Toolbar::has_active_mappings,
            },
            lable: "👁",
            tooltip_key: "toolbar-mapping-visualization",
            event: ToolbarEvent::ToggleMappingVisualization,
        },
        ToolbarButton {
            btn_type: ButtonType::Normal,
            lable: "⟳",
            tooltip_key: "toolbar-rotate",
            event: ToolbarEvent::RotateVideo,
        },
        ToolbarButton {
            btn_type: ButtonType::Normal,
            lable: "⚙",
            tooltip_key: "toolbar-configure",
            event: ToolbarEvent::ConfigureMappings,
        },
        ToolbarButton {
            btn_type: ButtonType::Normal,
            lable: "💤",
            tooltip_key: "toolbar-screen-off",
            event: ToolbarEvent::ToggleScreenPower,
        },
    ];
}

pub struct Toolbar {
    keyboard_mapping_enabled: bool,
    mapping_visualization_enabled: bool,
    has_active_mappings: bool,
}

impl Default for Toolbar {
    fn default() -> Self { Self::new() }
}

impl Toolbar {
    pub fn new() -> Self {
        Self {
            keyboard_mapping_enabled: false,
            mapping_visualization_enabled: false,
            has_active_mappings: false,
        }
    }

    pub fn width() -> f32 { TOOLBAR_WIDTH }

    fn is_keyboard_mapping_enabled(&self) -> bool { self.keyboard_mapping_enabled }

    fn is_mapping_visualization_enabled(&self) -> bool { self.mapping_visualization_enabled }

    fn has_active_mappings(&self) -> bool { self.has_active_mappings }

    /// Set keyboard mapping enabled state
    pub fn set_keyboard_mapping_enabled(&mut self, enabled: bool) {
        self.keyboard_mapping_enabled = enabled;
    }

    /// Set mapping visualization enabled state
    pub fn set_mapping_visualization_enabled(&mut self, enabled: bool) {
        self.mapping_visualization_enabled = enabled;
    }

    /// Set whether there are active mappings
    pub fn set_has_active_mappings(&mut self, has_mappings: bool) {
        self.has_active_mappings = has_mappings;
    }

    /// Draw the toolbar, return the event if any button is clicked
    pub fn draw(&mut self, ui: &mut egui::Ui) -> ToolbarEvent {
        let count = TOOLBAR_BUTTONS_BASE.len();
        if count == 0 {
            return ToolbarEvent::None;
        }

        let mut result = ToolbarEvent::None;

        ui.vertical_centered(|ui| {
            ui.spacing_mut().item_spacing.y = TOOLBAR_BTN_SPACING;

            let rect = ui.available_rect_before_wrap();
            let desired_height =
                (TOOLBAR_BTN_SIZE[1] + TOOLBAR_BTN_SPACING) * count as f32 + TOOLBAR_BTN_SPACING;
            let top_padding = (rect.height() - desired_height) / 2.0;
            ui.add_space(top_padding);

            ui.add_space(TOOLBAR_BTN_SPACING);

            for btn in TOOLBAR_BUTTONS_BASE.iter() {
                if self.draw_button(btn, ui) {
                    result = btn.event;
                }
            }
        });

        result
    }

    fn draw_button(&self, btn: &ToolbarButton, ui: &mut egui::Ui) -> bool {
        let mut button = Button::new(RichText::new(btn.lable).size(TOOLBAR_FONT_SIZE));

        let (enabled, selected) = match btn.btn_type {
            ButtonType::Normal => (true, false),
            ButtonType::SelectableConditional {
                is_selected,
                is_enabled,
            } => (is_enabled(self), is_selected(self)),
        };

        if selected {
            button = button.selected(true);
        }

        let response = ui.add_enabled_ui(enabled, |ui| {
            ui.add_sized(TOOLBAR_BTN_SIZE, button)
                .on_hover_text(t!(btn.tooltip_key))
        });

        response.inner.clicked()
    }
}
