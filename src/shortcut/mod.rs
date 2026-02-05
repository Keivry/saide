#[macro_use]
mod manager;

pub(crate) use manager::{ShortcutMap, shortcut};
use {crate::SAideApp, lazy_static::lazy_static};

lazy_static! {
    pub static ref DEFAULT_SHORTCUTS: ShortcutMap<SAideApp> = shortcuts! {
        sc!("F1") => action!(ui SAideApp::show_help_dialog);
        sc!("F6") => action!(serial [
            action!(ui usize SAideApp::show_profile_selection),
            action!(func usize SAideApp::switch_profile)
        ]);
        sc!("F7") => action!(func SAideApp::prev_profile);
        sc!("F8") => action!(func SAideApp::next_profile);
    };
}
