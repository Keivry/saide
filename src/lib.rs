pub mod avsync;
pub mod config;
pub mod constant;
pub mod controller;
pub mod core;
pub mod decoder;
pub mod error;
pub mod gpu;
pub mod i18n;
pub mod modal;
pub mod profiler;
pub mod scrcpy;

#[macro_use]
pub mod shortcut;

pub use core::SAideApp;
