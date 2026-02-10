mod app;
mod dialog;
mod editor;
mod function;
mod indicator;
mod player;
mod theme;
mod toolbar;

use {
    crate::{sc, shortcut::ShortcutMap, shortcuts},
    egui_action_macro::action,
    lazy_static::lazy_static,
};
pub use {app::SAideApp, theme::ThemeMode};

lazy_static! {
    pub static ref DEFAULT_SHORTCUTS: ShortcutMap<SAideApp> = shortcuts! {
        sc!("F1") => action!(null SAideApp::show_help_dialog);
        sc!("F6") => action!(serial [
            action!(usize SAideApp::show_profile_selection_dialog),
            action!(null SAideApp::switch_profile)
        ]);
        sc!("F7") => action!(null SAideApp::prev_profile);
        sc!("F8") => action!(null SAideApp::next_profile);
    };
}

#[derive(Default)]
pub struct VideoStats {
    pub fps: f32,
    pub total_frames: u32,
    pub dropped_frames: u32,
    pub latency_ms: f32,
    pub latency_decode_ms: f32,
    pub latency_upload_ms: f32,
    pub latency_p95_ms: f32,
}
