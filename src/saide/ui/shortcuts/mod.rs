#[macro_use]
mod manager;

pub(crate) use manager::{ShortcutMap, shortcut};
use {crate::saide::SAideApp, lazy_static::lazy_static};

lazy_static! {
    static ref DEFAULT_SHORTCUTS: ShortcutMap<SAideApp> = shortcuts! {
        sc!("F1") => action!(ui SAideApp::show_help_dialog);
        sc!("F2") => action!(serial [
            action!(ui string SAideApp::show_rename_profile_dialog),
            action!(func string SAideApp::rename_profile),
        ]);
        sc!("F3") => action!(func key pos SAideApp::test);
    };
}
