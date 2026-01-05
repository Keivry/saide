//! Toolbar UI component
//!
//! Provides a vertical toolbar with buttons for common actions
//! such as rotating video, configuring mappings, and toggling screen power.

use {
    crate::t,
    egui::{Button, Color32, RichText},
    lazy_static::lazy_static,
};

const TOOLBAR_WIDTH: f32 = 42.0;
const TOOLBAR_BTN_SIZE: [f32; 2] = [36.0, 36.0];
const TOOLBAR_FG_COLOR: Color32 = Color32::from_rgb(200, 200, 200);
const TOOLBAR_FONT_SIZE: f32 = 16.0;
const TOOLBAR_BTN_SPACING: f32 = 2.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToolbarEvent {
    None,
    RotateVideo,
    ConfigureMappings,
    ToggleScreenPower,
}

struct ToolbarButton {
    lable: &'static str,
    tooltip_key: &'static str,
    event: ToolbarEvent,
}

lazy_static! {
    static ref TOOLBAR_BUTTONS_BASE: [ToolbarButton; 2] = [
        ToolbarButton {
            lable: "⟳",
            tooltip_key: "toolbar-rotate",
            event: ToolbarEvent::RotateVideo,
        },
        ToolbarButton {
            lable: "⚙",
            tooltip_key: "toolbar-configure",
            event: ToolbarEvent::ConfigureMappings,
        },
    ];
}

pub struct Toolbar {}

impl Default for Toolbar {
    fn default() -> Self { Self::new() }
}

impl Toolbar {
    pub fn new() -> Self { Self {} }

    pub fn width() -> f32 { TOOLBAR_WIDTH }

    /// Draw the toolbar, return the event if any button is clicked
    pub fn draw(&self, ui: &mut egui::Ui) -> ToolbarEvent {
        let count = TOOLBAR_BUTTONS_BASE.len() + 1; // +1 for dynamic screen power button
        if count == 0 {
            return ToolbarEvent::None;
        }

        let mut result = ToolbarEvent::None;

        ui.vertical_centered(|ui| {
            ui.spacing_mut().item_spacing.y = TOOLBAR_BTN_SPACING;

            // Center buttons vertically
            let rect = ui.available_rect_before_wrap();
            let desired_height =
                (TOOLBAR_BTN_SIZE[1] + TOOLBAR_BTN_SPACING) * count as f32 + TOOLBAR_BTN_SPACING;
            let top_padding = (rect.height() - desired_height) / 2.0;
            ui.add_space(top_padding);

            ui.add_space(TOOLBAR_BTN_SPACING);

            // Draw base buttons
            for btn in TOOLBAR_BUTTONS_BASE.iter() {
                if self.draw_button(btn, ui) {
                    result = btn.event;
                }
            }

            // Draw screen off button (wake up with physical power button)
            let screen_off_tooltip = format!(
                "{}\n{}",
                t!("toolbar-screen-off"),
                t!("toolbar-screen-off-hint")
            );
            if ui
                .add_sized(
                    TOOLBAR_BTN_SIZE,
                    Button::new(
                        RichText::new("💤")
                            .color(TOOLBAR_FG_COLOR)
                            .size(TOOLBAR_FONT_SIZE),
                    ),
                )
                .on_hover_text(screen_off_tooltip)
                .clicked()
            {
                result = ToolbarEvent::ToggleScreenPower;
            }
        });

        result
    }

    /// Draw a single button, return true if clicked
    fn draw_button(&self, btn: &ToolbarButton, ui: &mut egui::Ui) -> bool {
        ui.add_sized(
            TOOLBAR_BTN_SIZE,
            Button::new(
                RichText::new(btn.lable)
                    .color(TOOLBAR_FG_COLOR)
                    .size(TOOLBAR_FONT_SIZE),
            ),
        )
        .on_hover_text(t!(btn.tooltip_key))
        .clicked()
    }
}
