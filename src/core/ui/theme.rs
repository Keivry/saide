// SPDX-License-Identifier: MIT OR Apache-2.0

//! UI theme management, including color definitions and theme mode handling.
//!
//! Supports dark and light themes, with an option to automatically match the system theme. Colors
//! for various UI elements are defined based on the active theme.
use egui::Color32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
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

    pub fn apply_to_context(ctx: &egui::Context) {
        let theme_mode = Self::from_env();

        if let Some(use_dark) = match theme_mode {
            ThemeMode::Dark => Some(true),
            ThemeMode::Light => Some(false),
            ThemeMode::Auto => None,
        } {
            ctx.set_visuals(if use_dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            });
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AppColors {
    pub toolbar_bg: Color32,
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
    pub error_overlay_bg: Color32,
    pub error_overlay_title: Color32,
    pub error_overlay_text: Color32,
    pub error_overlay_hint: Color32,
    pub error_overlay_details: Color32,
}

impl AppColors {
    pub fn dark() -> Self {
        Self {
            toolbar_bg: Color32::from_rgb(32, 32, 32),
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
            error_overlay_bg: Color32::from_black_alpha(200),
            error_overlay_title: Color32::from_rgb(255, 100, 100),
            error_overlay_text: Color32::WHITE,
            error_overlay_hint: Color32::GRAY,
            error_overlay_details: Color32::DARK_GRAY,
        }
    }

    pub fn light() -> Self {
        Self {
            toolbar_bg: Color32::from_rgb(240, 240, 240),
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
            error_overlay_bg: Color32::from_rgba_unmultiplied(255, 255, 255, 240),
            error_overlay_title: Color32::from_rgb(200, 0, 0),
            error_overlay_text: Color32::from_rgb(40, 40, 40),
            error_overlay_hint: Color32::from_rgb(120, 120, 120),
            error_overlay_details: Color32::from_rgb(100, 100, 100),
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
