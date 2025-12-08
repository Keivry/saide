use {
    super::VideoStats,
    std::time::{Duration, Instant},
};

#[derive(Clone, PartialEq, Eq)]
pub enum StatusBarEvent {
    None,
    /// Selected keyboard mapping profile changed, contains new profile name
    ProfileChanged(String),
}

const STATUSBAR_HEIGHT: f32 = 32.0;
const STATUSBAR_REFRESH_FPS: u64 = 5;
const STATUSBAR_REFRESH_INTERVAL_MS: Duration = Duration::from_millis(1000 / STATUSBAR_REFRESH_FPS);

pub struct StatusBar {
    /// Available custom keyboard mapping profiles names
    avail_profile_names: Vec<String>,
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
}

impl StatusBar {
    pub fn new(max_fps: f32) -> Self {
        Self {
            avail_profile_names: Vec::new(),
            active_profile_name: None,
            device_orientation: 0,
            capture_orientation: 0,
            video_rotation: 0,
            video_original_width: 0,
            video_original_height: 0,
            max_fps,

            video_stats: VideoStats::default(),

            last_update: Instant::now(),
        }
    }

    pub fn height() -> f32 { STATUSBAR_HEIGHT }

    #[allow(dead_code)]
    pub fn fps(&self) -> f32 { self.video_stats.fps }

    pub fn reset_profiles(&mut self) -> &mut Self {
        self.avail_profile_names.clear();
        self.active_profile_name = None;
        self
    }

    pub fn update_active_profile(&mut self, profile_name: Option<String>) -> &mut Self {
        self.active_profile_name = profile_name;
        self
    }

    pub fn update_available_profiles(&mut self, profile_names: Vec<String>) -> &mut Self {
        self.avail_profile_names = profile_names;
        self
    }

    /// Update video statistics, limited to STATUSBAR_REFRESH_FPS updates per second
    pub fn update_video_stats(&mut self, stats: VideoStats) -> &mut Self {
        if self.last_update.elapsed() < STATUSBAR_REFRESH_INTERVAL_MS {
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

    pub fn draw(&self, ui: &mut egui::Ui) -> StatusBarEvent {
        let mut result = StatusBarEvent::None;

        ui.horizontal_centered(|ui| {
            ui.label(format!(
                "Resolution: {:>5} x {:<5}",
                self.video_original_width, self.video_original_height
            ));
            ui.separator();

            ui.label(format!(
                "Capture Orientation: {:>3}°",
                self.capture_orientation * 90
            ));
            ui.separator();

            ui.label(format!("Video Rotation: {:>3}°", self.video_rotation * 90));
            ui.separator();

            ui.label(format!(
                "Device Rotation: {:>3}°",
                self.device_orientation * 90
            ));
            ui.separator();

            ui.label(format!(
                "FPS: {:>3}",
                self.video_stats.fps.min(self.max_fps) as u32
            ));
            ui.separator();

            ui.label(format!(
                "Frames: {:>4}/{:<4}",
                self.video_stats.dropped_frames, self.video_stats.total_frames
            ));
            ui.separator();

            ui.label(format!("Latency: {:>3}ms", self.video_stats.latency_ms));

            ui.label("Profile:");
            egui::ComboBox::from_id_salt("mapping_profile_combobox")
                .selected_text(
                    self.active_profile_name
                        .as_deref()
                        .unwrap_or("Not Available"),
                )
                .show_ui(ui, |ui| {
                    self.avail_profile_names.iter().for_each(|profile_name| {
                        if ui
                            .selectable_label(
                                Some(profile_name.as_str()) == self.active_profile_name.as_deref(),
                                profile_name.as_str(),
                            )
                            .clicked()
                        {
                            result = StatusBarEvent::ProfileChanged(profile_name.clone());
                        }
                    });
                });
        });

        result
    }
}
