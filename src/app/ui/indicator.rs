use {
    super::VideoStats,
    std::time::{Duration, Instant},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum IndicatorPosition {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

const INDICATOR_REFRESH_FPS: u64 = 5;
const INDICATOR_REFRESH_INTERVAL_MS: Duration = Duration::from_millis(1000 / INDICATOR_REFRESH_FPS);

/// Minimal indicator size
const INDICATOR_PADDING: f32 = 8.0;
const INDICATOR_SPACING: f32 = 4.0;

/// Floating panel trigger key
const TRIGGER_MODIFIER: egui::Modifiers = egui::Modifiers::CTRL;

pub struct Indicator {
    /// Currently selected custom keyboard mapping profile
    active_profile_name: Option<String>,

    /// Device orientation (0-3), clockwise
    device_orientation: u32,

    // V4l2 capture orientation (0-3), counter-clockwise
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
    pub fn new(max_fps: f32) -> Self {
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

            position: IndicatorPosition::BottomLeft,
            floating_panel_visible: false,
        }
    }

    pub fn fps(&self) -> f32 { self.video_stats.fps }

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
        self.device_orientation = orientation;
        self
    }

    pub fn update_capture_orientation(&mut self, orientation: u32) -> &mut Self {
        self.capture_orientation = orientation;
        self
    }

    pub fn update_video_rotation(&mut self, rotation: u32) -> &mut Self {
        self.video_rotation = rotation;
        self
    }

    /// Draw indicator in video corner, returns the rectangle of the drawn indicator
    pub fn draw_indicator(&mut self, ui: &mut egui::Ui, video_rect: egui::Rect) -> egui::Rect {
        // Calculate indicator position based on corner_position
        let indicator_pos = match self.position {
            IndicatorPosition::TopLeft => egui::pos2(
                video_rect.left() + INDICATOR_PADDING,
                video_rect.top() + INDICATOR_PADDING,
            ),
            IndicatorPosition::TopRight => egui::pos2(
                video_rect.right() - INDICATOR_PADDING,
                video_rect.top() + INDICATOR_PADDING,
            ),
            IndicatorPosition::BottomLeft => egui::pos2(
                video_rect.left() + INDICATOR_PADDING,
                video_rect.bottom() - INDICATOR_PADDING,
            ),
            IndicatorPosition::BottomRight => egui::pos2(
                video_rect.right() - INDICATOR_PADDING,
                video_rect.bottom() - INDICATOR_PADDING,
            ),
        };

        let indicator_id = ui.id().with("indicator");
        let area_response = egui::Area::new(indicator_id)
            .fixed_pos(indicator_pos)
            .constrain(true)
            .interactable(false)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::NONE
                    .fill(egui::Color32::from_black_alpha(150))
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
                                .color(egui::Color32::from_rgb(100, 255, 100))
                                .size(14.0),
                            );

                            // Draw Latency info (disabled for now)
                            // ui.add_space(INDICATOR_SPACING);
                            // ui.label(
                            //     egui::RichText::new(format!(
                            //         "Latency: {}ms",
                            //         self.video_stats.latency_ms
                            //     ))
                            //     .color(egui::Color32::from_rgb(255, 200, 100))
                            //     .size(14.0),
                            // );
                        });
                    })
                    .response
            });

        let indicator_rect = area_response.response.rect;

        // Check if mouse is hovering and modifier key is pressed
        let should_show_floating_panel = ui.input(|i| {
            i.modifiers.matches_exact(TRIGGER_MODIFIER)
                && i.pointer
                    .hover_pos()
                    .is_some_and(|pos| indicator_rect.contains(pos))
        });

        self.floating_panel_visible = should_show_floating_panel;

        // Draw floating panel if needed
        if self.floating_panel_visible {
            self.draw_floating_panel(ui, indicator_rect);
        }

        indicator_rect
    }

    /// Draw detailed floating panel near the indicator
    fn draw_floating_panel(&self, ui: &mut egui::Ui, indicator_rect: egui::Rect) {
        let panel_id = ui.id().with("stats_floating_panel");

        // Position panel below or above indicator based on corner position
        let (panel_pos, anchor) = match self.position {
            IndicatorPosition::TopLeft | IndicatorPosition::TopRight => (
                egui::pos2(indicator_rect.left(), indicator_rect.bottom() + 4.0),
                egui::Align2::LEFT_TOP,
            ),
            IndicatorPosition::BottomLeft | IndicatorPosition::BottomRight => (
                egui::pos2(indicator_rect.left(), indicator_rect.top() - 4.0),
                egui::Align2::LEFT_BOTTOM,
            ),
        };

        egui::Area::new(panel_id)
            .fixed_pos(panel_pos)
            .anchor(anchor, egui::Vec2::ZERO)
            .constrain(true)
            .order(egui::Order::Tooltip)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style())
                    .fill(egui::Color32::from_gray(40))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(80)))
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        ui.set_min_width(280.0);

                        egui::Grid::new("stats_grid")
                            .num_columns(2)
                            .spacing([16.0, 8.0])
                            .striped(true)
                            .show(ui, |ui| {
                                ui.label("Resolution:");
                                ui.label(format!(
                                    "{} x {}",
                                    self.video_original_width, self.video_original_height
                                ));
                                ui.end_row();

                                ui.label("Capture Orientation:");
                                ui.label(format!("{}°", self.capture_orientation * 90));
                                ui.end_row();

                                ui.label("Video Rotation:");
                                ui.label(format!("{}°", self.video_rotation * 90));
                                ui.end_row();

                                ui.label("Device Rotation:");
                                ui.label(format!("{}°", self.device_orientation * 90));
                                ui.end_row();

                                ui.label("FPS:");
                                ui.label(format!(
                                    "{}",
                                    self.video_stats.fps.min(self.max_fps) as u32
                                ));
                                ui.end_row();

                                ui.label("Frames (Dropped/Total):");
                                ui.label(format!(
                                    "{}/{}",
                                    self.video_stats.dropped_frames, self.video_stats.total_frames
                                ));
                                ui.end_row();

                                ui.label("Latency:");
                                ui.label(format!("{}ms", self.video_stats.latency_ms));
                                ui.end_row();

                                ui.label("Profile:");
                                ui.label(
                                    self.active_profile_name
                                        .as_deref()
                                        .unwrap_or("Not Available"),
                                );
                                ui.end_row();
                            });
                    });
            });
    }
}
