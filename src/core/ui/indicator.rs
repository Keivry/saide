// SPDX-License-Identifier: MIT OR Apache-2.0

//! Video statistics indicator overlay for the video player.
//!
//! Displays FPS, latency, and other relevant information in a floating panel.
//! The panel can be toggled by hovering over the indicator while holding a modifier key.

use {
    super::{VideoStats, theme::AppColors},
    crate::{config::IndicatorPosition, t},
    std::time::{Duration, Instant},
};

const INDICATOR_REFRESH_FPS: u64 = 5;
const INDICATOR_REFRESH_INTERVAL_MS: Duration = Duration::from_millis(1000 / INDICATOR_REFRESH_FPS);

const INDICATOR_PADDING: f32 = 8.0;
const PANEL_SPACING: f32 = 4.0;

/// Floating panel trigger key
const TRIGGER_MODIFIER: egui::Modifiers = egui::Modifiers::CTRL;

pub struct Indicator {
    /// Currently selected custom keyboard mapping profile
    active_profile_name: Option<String>,

    /// Device orientation (0-3), clockwise
    device_orientation: u32,

    // Scrcpy-server capture orientation (0-3), counter-clockwise
    capture_orientation: u32,

    // Video render rotation state (0-3), clockwise
    video_rotation: u32,

    video_original_width: u32,
    video_original_height: u32,

    max_fps: f32,

    /// Current video statistics
    video_stats: VideoStats,

    /// Timestamp of the last update, used to limit update frequency for video stats
    last_update: Instant,

    /// Position for indicator
    position: IndicatorPosition,

    /// Whether floating panel is currently visible
    floating_panel_visible: bool,
}

impl Indicator {
    pub fn new(position: IndicatorPosition, max_fps: f32) -> Self {
        Self {
            active_profile_name: None,
            device_orientation: 0,
            capture_orientation: 0,
            video_rotation: 0,
            video_original_width: 0,
            video_original_height: 0,
            max_fps,

            video_stats: VideoStats::default(),

            last_update: Instant::now(),

            position,
            floating_panel_visible: false,
        }
    }

    pub fn reset_profiles(&mut self) -> &mut Self {
        self.active_profile_name = None;
        self
    }

    pub fn update_active_profile(&mut self, profile_name: Option<String>) -> &mut Self {
        self.active_profile_name = profile_name;
        self
    }

    /// Update video statistics, limited to INDICATOR_REFRESH_FPS updates per second
    pub fn update_video_stats(&mut self, stats: VideoStats) -> &mut Self {
        if self.last_update.elapsed() < INDICATOR_REFRESH_INTERVAL_MS {
            return self;
        }

        self.video_stats = stats;
        self.last_update = Instant::now();
        self
    }

    pub fn update_video_resolution(&mut self, dimensions: (u32, u32)) -> &mut Self {
        self.video_original_width = dimensions.0;
        self.video_original_height = dimensions.1;
        self
    }

    pub fn update_device_orientation(&mut self, orientation: u32) -> &mut Self {
        self.device_orientation = orientation % 4;
        self
    }

    pub fn update_video_rotation(&mut self, rotation: u32) -> &mut Self {
        self.video_rotation = rotation;
        self
    }

    /// Get color based on latency value
    fn get_color_from_latency(&self, colors: &AppColors) -> egui::Color32 {
        if self.video_stats.latency_ms >= 50.0 {
            colors.indicator_fps_high
        } else if self.video_stats.latency_ms >= 20.0 {
            colors.indicator_fps_medium
        } else {
            colors.indicator_fps_low
        }
    }

    /// Get pivot point for indicator area based on position
    fn get_pivot(&self) -> egui::Align2 {
        match self.position {
            IndicatorPosition::TopLeft => egui::Align2::LEFT_TOP,
            IndicatorPosition::TopRight => egui::Align2::RIGHT_TOP,
            IndicatorPosition::BottomLeft => egui::Align2::LEFT_BOTTOM,
            IndicatorPosition::BottomRight => egui::Align2::RIGHT_BOTTOM,
        }
    }

    /// Draw indicator in video corner, returns the rectangle of the drawn indicator
    pub fn draw_indicator(&mut self, ui: &mut egui::Ui, video_rect: egui::Rect) -> egui::Rect {
        let colors = AppColors::from_context(ui.ctx());

        let indicator_pos = match self.position {
            IndicatorPosition::TopLeft => {
                video_rect.left_top() + egui::vec2(INDICATOR_PADDING, INDICATOR_PADDING)
            }
            IndicatorPosition::TopRight => {
                video_rect.right_top() + egui::vec2(-INDICATOR_PADDING, INDICATOR_PADDING)
            }
            IndicatorPosition::BottomLeft => {
                video_rect.left_bottom() + egui::vec2(INDICATOR_PADDING, -INDICATOR_PADDING)
            }
            IndicatorPosition::BottomRight => {
                video_rect.right_bottom() + egui::vec2(-INDICATOR_PADDING, -INDICATOR_PADDING)
            }
        };

        let indicator_id = ui.id().with("indicator");
        let area_response = egui::Area::new(indicator_id)
            .fixed_pos(indicator_pos)
            .pivot(self.get_pivot())
            .constrain(true)
            .interactable(false)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::NONE
                    .fill(colors.indicator_bg)
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.style_mut().interaction.selectable_labels = false;
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "FPS: {}",
                                    self.video_stats.fps.min(self.max_fps) as u32
                                ))
                                .color(self.get_color_from_latency(&colors))
                                .size(14.0),
                            );
                        });
                    })
                    .response
            });

        let indicator_rect = area_response.response.rect;

        let should_show_floating_panel = ui.input(|i| {
            i.modifiers.matches_exact(TRIGGER_MODIFIER)
                && i.pointer
                    .hover_pos()
                    .is_some_and(|pos| indicator_rect.contains(pos))
        });

        self.floating_panel_visible = should_show_floating_panel;

        if self.floating_panel_visible {
            self.draw_floating_panel(ui, indicator_rect, &colors);
        }

        indicator_rect
    }

    /// Draw detailed floating panel near the indicator
    fn draw_floating_panel(
        &self,
        ui: &mut egui::Ui,
        indicator_rect: egui::Rect,
        colors: &AppColors,
    ) {
        let panel_id = ui.id().with("stats_floating_panel");

        let panel_pos = match self.position {
            IndicatorPosition::TopLeft => egui::pos2(
                indicator_rect.left(),
                indicator_rect.bottom() + PANEL_SPACING,
            ),
            IndicatorPosition::TopRight => egui::pos2(
                indicator_rect.right(),
                indicator_rect.bottom() + PANEL_SPACING,
            ),
            IndicatorPosition::BottomLeft => {
                egui::pos2(indicator_rect.left(), indicator_rect.top() - PANEL_SPACING)
            }
            IndicatorPosition::BottomRight => {
                egui::pos2(indicator_rect.right(), indicator_rect.top() - PANEL_SPACING)
            }
        };

        egui::Area::new(panel_id)
            .fixed_pos(panel_pos)
            .pivot(self.get_pivot())
            .constrain(true)
            .order(egui::Order::Tooltip)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style())
                    .fill(colors.indicator_popup_bg)
                    .stroke(egui::Stroke::new(1.0, colors.indicator_popup_stroke))
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        ui.set_min_width(280.0);

                        egui::Grid::new("stats_grid")
                            .num_columns(2)
                            .spacing([16.0, 8.0])
                            .striped(true)
                            .show(ui, |ui| {
                                ui.label(t!("indicator-panel-resolution"));
                                ui.label(format!(
                                    "{} x {}",
                                    self.video_original_width, self.video_original_height
                                ));
                                ui.end_row();

                                ui.label(t!("indicator-panel-capture-orientation"));
                                ui.label(format!("{}°", self.capture_orientation * 90));
                                ui.end_row();

                                ui.label(t!("indicator-panel-video-rotation"));
                                ui.label(format!("{}°", self.video_rotation * 90));
                                ui.end_row();

                                ui.label(t!("indicator-panel-device-rotation"));
                                ui.label(format!("{}°", self.device_orientation * 90));
                                ui.end_row();

                                ui.label(t!("indicator-panel-fps"));
                                ui.label(format!(
                                    "{}",
                                    self.video_stats.fps.min(self.max_fps) as u32
                                ));
                                ui.end_row();

                                ui.label(t!("indicator-panel-frames"));
                                ui.label(format!(
                                    "{}/{}",
                                    self.video_stats.dropped_frames, self.video_stats.total_frames
                                ));
                                ui.end_row();

                                ui.label(t!("indicator-panel-latency-avg"));
                                ui.label(format!("{:.1}ms", self.video_stats.latency_ms));
                                ui.end_row();

                                ui.label(t!("indicator-panel-latency-p95"));
                                ui.label(format!("{:.1}ms", self.video_stats.latency_p95_ms));
                                ui.end_row();

                                ui.label(t!("indicator-panel-decode"));
                                ui.label(format!("{:.1}ms", self.video_stats.latency_decode_ms));
                                ui.end_row();

                                ui.label(t!("indicator-panel-gpu-upload"));
                                ui.label(format!("{:.1}ms", self.video_stats.latency_upload_ms));
                                ui.end_row();

                                ui.label(t!("indicator-panel-profile"));
                                ui.label(
                                    self.active_profile_name
                                        .as_deref()
                                        .unwrap_or(&t!("indicator-panel-profile-none")),
                                );
                                ui.end_row();
                            });
                    });
            });
    }
}
