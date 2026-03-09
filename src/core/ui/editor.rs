//! Mapping Editor UI
//!
//! This module provides the UI overlay for edit key mappings.
//! It allows users to add, delete, and manage key mappings visually.

use {
    super::{
        super::{
            coords::{MappingCoordSys, ScrcpyCoordSys, VisualCoordSys},
            utils::extract_position,
        },
        AppCommand,
        theme::AppColors,
    },
    crate::{config::mapping::ProfileRef, shortcut::ShortcutMap, t},
    eframe::egui::{self, Color32, FontId, Pos2, Stroke},
    lazy_static::lazy_static,
};

lazy_static! {
    pub static ref MAPPING_EDITOR_SHORTCUTS: ShortcutMap<AppCommand> = crate::shortcut_map! {
        "F2"     => AppCommand::ShowRenameDialog,
        "F3"     => AppCommand::ShowCreateDialog,
        "F4"     => AppCommand::ShowDeleteDialog,
        "F5"     => AppCommand::ShowSaveAsDialog,
        "Escape" => AppCommand::CloseEditor,
    };
}

const EDITOR_WINDOW_ID: &str = "mapping_config_overlay";

/// Parameters for the mapping editor UI
pub struct EditorParams<'a> {
    pub profile: Option<ProfileRef>,
    pub video_rect: &'a egui::Rect,
    pub visual_coords: &'a VisualCoordSys,
    pub scrcpy_coords: &'a ScrcpyCoordSys,
    pub mapping_coords: &'a MappingCoordSys,
}

/// Mapping Editor UI
pub struct MappingEditor {}

impl MappingEditor {
    pub fn new() -> Self { Self {} }

    pub fn draw(&self, ctx: &egui::Context, params: EditorParams) {
        let colors = AppColors::from_context(ctx);

        egui::Area::new(egui::Id::new(EDITOR_WINDOW_ID))
            .fixed_pos(params.video_rect.min)
            .interactable(false)
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                let painter = ui.painter();

                painter.rect_filled(
                    *params.video_rect,
                    0.0,
                    Color32::from_rgba_premultiplied(0, 0, 0, 180),
                );

                painter.text(
                    Pos2::new(params.video_rect.center().x, params.video_rect.top() + 20.0),
                    egui::Align2::CENTER_TOP,
                    t!("mapping-config-title"),
                    FontId::proportional(18.0),
                    Color32::WHITE,
                );

                let instructions = [
                    t!("mapping-config-instruction-help"),
                    t!("mapping-config-instruction-exit"),
                ];
                instructions.iter().enumerate().for_each(|(i, text)| {
                    painter.text(
                        Pos2::new(
                            params.video_rect.center().x,
                            params.video_rect.top() + 50.0 + i as f32 * 20.0,
                        ),
                        egui::Align2::CENTER_TOP,
                        text,
                        FontId::proportional(12.0),
                        Color32::LIGHT_GRAY,
                    );
                });

                if let Some(profile) = &params.profile {
                    profile.read().iter().for_each(|(key, action)| {
                        if let Some(percent_pos) = extract_position(action) {
                            let screen_pos = params.visual_coords.from_mapping(
                                &percent_pos,
                                params.video_rect,
                                params.scrcpy_coords,
                                params.mapping_coords,
                            );

                            painter.circle_filled(screen_pos, 12.0, colors.mapping_circle_fill);
                            painter.circle_stroke(
                                screen_pos,
                                12.0,
                                Stroke::new(2.0, colors.mapping_circle_stroke),
                            );

                            let key_text = format!("{:?}", key);
                            painter.text(
                                Pos2::new(screen_pos.x, screen_pos.y - 15.0),
                                egui::Align2::CENTER_BOTTOM,
                                &key_text,
                                FontId::proportional(12.0),
                                Color32::WHITE,
                            );
                        }
                    });
                }

                let profile_text = format!(
                    "{} {}",
                    t!("mapping-config-profile-label"),
                    params
                        .profile
                        .as_ref()
                        .map(|p| p.read().name().to_string())
                        .unwrap_or_else(|| t!("mapping-config-profile-none"))
                );
                painter.text(
                    Pos2::new(
                        params.video_rect.center().x,
                        params.video_rect.top() + 100.0,
                    ),
                    egui::Align2::CENTER_TOP,
                    &profile_text,
                    FontId::proportional(14.0),
                    Color32::LIGHT_GRAY,
                );
            });
    }
}
