use egui::Color32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThemeMode {
    Auto,
    Dark,
    Light,
}

impl ThemeMode {
    fn from_env() -> Self {
        match std::env::var("SAIDE_THEME").as_deref() {
            Ok("dark") => Self::Dark,
            Ok("light") => Self::Light,
            Ok("auto") | Err(_) => Self::Auto,
            Ok(other) => {
                tracing::warn!(
                    "Invalid SAIDE_THEME value '{}', using 'auto' (valid: dark, light, auto)",
                    other
                );
                Self::Auto
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AppColors {
    pub toolbar_bg: Color32,
    pub toolbar_fg: Color32,
    pub toolbar_button_bg: Color32,
    pub toolbar_button_bg_hovered: Color32,
    pub toolbar_button_bg_active: Color32,
    pub indicator_bg: Color32,
    pub indicator_popup_bg: Color32,
    pub indicator_popup_stroke: Color32,
    pub indicator_fps_low: Color32,
    pub indicator_fps_medium: Color32,
    pub indicator_fps_high: Color32,
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
            toolbar_button_bg: Color32::from_rgb(48, 48, 48),
            toolbar_button_bg_hovered: Color32::from_rgb(64, 64, 64),
            toolbar_button_bg_active: Color32::from_rgb(80, 80, 80),
            indicator_bg: Color32::from_black_alpha(150),
            indicator_popup_bg: Color32::from_gray(40),
            indicator_popup_stroke: Color32::from_gray(80),
            indicator_fps_low: Color32::from_rgb(100, 255, 100),
            indicator_fps_medium: Color32::from_rgb(255, 255, 100),
            indicator_fps_high: Color32::from_rgb(255, 100, 100),
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
            toolbar_button_bg: Color32::from_rgb(220, 220, 220),
            toolbar_button_bg_hovered: Color32::from_rgb(200, 200, 200),
            toolbar_button_bg_active: Color32::from_rgb(180, 180, 180),
            indicator_bg: Color32::from_rgba_unmultiplied(255, 255, 255, 200),
            indicator_popup_bg: Color32::from_gray(245),
            indicator_popup_stroke: Color32::from_gray(200),
            indicator_fps_low: Color32::from_rgb(0, 150, 0),
            indicator_fps_medium: Color32::from_rgb(200, 140, 0),
            indicator_fps_high: Color32::from_rgb(200, 0, 0),
            mapping_overlay_fill: Color32::from_rgba_unmultiplied(0, 120, 215, 80),
            mapping_overlay_text: Color32::from_rgba_unmultiplied(255, 255, 255, 230),
            mapping_circle_fill: Color32::from_rgb(0, 120, 215),
            mapping_circle_stroke: Color32::from_rgb(0, 80, 150),
            audio_warning_bg: Color32::from_rgba_unmultiplied(255, 255, 255, 240),
        }
    }

    /// Get theme colors based on context and environment variable override
    ///
    /// Priority:
    /// 1. SAIDE_THEME env var (dark/light/auto)
    /// 2. System theme from egui Context (auto mode)
    ///
    /// # Environment Variable
    /// - `SAIDE_THEME=dark` - Force dark mode
    /// - `SAIDE_THEME=light` - Force light mode
    /// - `SAIDE_THEME=auto` - Use system theme (default)
    pub fn from_context(ctx: &egui::Context) -> Self {
        let theme_mode = ThemeMode::from_env();

        let use_dark = match theme_mode {
            ThemeMode::Dark => true,
            ThemeMode::Light => false,
            ThemeMode::Auto => ctx.style().visuals.dark_mode,
        };

        if use_dark {
            Self::dark()
        } else {
            Self::light()
        }
    }
}
