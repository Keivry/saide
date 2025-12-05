#[derive(Clone, PartialEq, Eq)]
pub enum StatusBarEvent {
    None,
    /// Selected keyboard mapping profile changed, contains new profile name
    ProfileChanged(String),
}

const STATUSBAR_HEIGHT: f32 = 32.0;

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

    // Current FPS
    fps: f32,

    max_fps: f32,
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
            fps: 0.0,
            max_fps,
        }
    }

    pub fn height() -> f32 { STATUSBAR_HEIGHT }

    pub fn fps(&self) -> f32 { self.fps }

    pub fn reset_profiles(&mut self) -> &mut Self {
        self.avail_profile_names.clear();
        self.active_profile_name = None;
        self
    }

    pub fn set_active_profile(&mut self, profile_name: Option<String>) -> &mut Self {
        self.active_profile_name = profile_name;
        self
    }

    pub fn set_available_profiles(&mut self, profile_names: Vec<String>) -> &mut Self {
        self.avail_profile_names = profile_names;
        self
    }

    pub fn set_fps(&mut self, fps: f32) -> &mut Self {
        self.fps = fps;
        self
    }

    pub fn set_video_resolution(&mut self, width: u32, height: u32) -> &mut Self {
        self.video_original_width = width;
        self.video_original_height = height;
        self
    }

    pub fn set_device_orientation(&mut self, orientation: u32) -> &mut Self {
        self.device_orientation = orientation;
        self
    }

    pub fn set_capture_orientation(&mut self, orientation: u32) -> &mut Self {
        self.capture_orientation = orientation;
        self
    }

    pub fn set_video_rotation(&mut self, rotation: u32) -> &mut Self {
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
            ui.label(format!("FPS: {:>3}", self.fps.min(self.max_fps) as u32));
            ui.separator();
            ui.label(format!(
                "Device Rotation: {:>3}°",
                self.device_orientation * 90
            ));
            ui.separator();
            ui.label(format!(
                "Capture Orientation: {:>3}°",
                self.capture_orientation * 90
            ));
            ui.separator();
            ui.label(format!("Video Rotation: {:>3}°", self.video_rotation * 90));
            ui.separator();

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
