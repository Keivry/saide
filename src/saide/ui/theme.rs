use egui::Color32;

#[derive(Debug, Clone, Copy)]
pub struct AppColors {
    pub toolbar_bg: Color32,
    pub toolbar_fg: Color32,
    pub indicator_bg: Color32,
    pub indicator_popup_bg: Color32,
    pub indicator_popup_stroke: Color32,
    pub mapping_overlay_fill: Color32,
    pub mapping_overlay_text: Color32,
    pub mapping_circle_fill: Color32,
    pub mapping_circle_stroke: Color32,
    pub audio_warning_bg: Color32,
}

impl AppColors {
    pub fn dark() -> Self {
        Self {
            toolbar_bg: Color32::from_rgb(32, 32, 32),
            toolbar_fg: Color32::from_rgb(200, 200, 200),
            indicator_bg: Color32::from_black_alpha(150),
            indicator_popup_bg: Color32::from_gray(40),
            indicator_popup_stroke: Color32::from_gray(80),
            mapping_overlay_fill: Color32::from_rgba_unmultiplied(0, 255, 0, 60),
            mapping_overlay_text: Color32::from_rgba_unmultiplied(0, 0, 0, 180),
            mapping_circle_fill: Color32::from_rgb(100, 200, 255),
            mapping_circle_stroke: Color32::WHITE,
            audio_warning_bg: Color32::from_rgba_unmultiplied(40, 40, 40, 220),
        }
    }

    pub fn light() -> Self {
        Self {
            toolbar_bg: Color32::from_rgb(240, 240, 240),
            toolbar_fg: Color32::from_rgb(60, 60, 60),
            indicator_bg: Color32::from_rgba_unmultiplied(255, 255, 255, 200),
            indicator_popup_bg: Color32::from_gray(245),
            indicator_popup_stroke: Color32::from_gray(200),
            mapping_overlay_fill: Color32::from_rgba_unmultiplied(0, 120, 215, 80),
            mapping_overlay_text: Color32::from_rgba_unmultiplied(255, 255, 255, 230),
            mapping_circle_fill: Color32::from_rgb(0, 120, 215),
            mapping_circle_stroke: Color32::from_rgb(0, 80, 150),
            audio_warning_bg: Color32::from_rgba_unmultiplied(255, 255, 255, 240),
        }
    }

    pub fn from_context(ctx: &egui::Context) -> Self {
        if ctx.style().visuals.dark_mode {
            Self::dark()
        } else {
            Self::light()
        }
    }
}
