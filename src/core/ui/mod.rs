mod app;
mod dialog;
mod editor;
mod function;
mod indicator;
mod player;
mod theme;
mod toolbar;

pub use app::SAideApp;
use {
    crate::{action::action, sc, shortcut::ShortcutMap, shortcuts},
    lazy_static::lazy_static,
};

lazy_static! {
    pub static ref DEFAULT_SHORTCUTS: ShortcutMap<SAideApp> = shortcuts! {
        sc!("F1") => action!(null SAideApp::show_help_dialog);
        sc!("F6") => action!(serial [
            action!(usize show_profile_selection),
            action!(SAideApp::switch_profile)
        ]);
        sc!("F7") => action!(SAideApp::prev_profile);
        sc!("F8") => action!(SAideApp::next_profile);
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
