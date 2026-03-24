// SPDX-License-Identifier: MIT OR Apache-2.0

pub mod avsync;
pub mod capture;
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
pub mod runtime;
pub mod scrcpy;

#[macro_use]
pub mod shortcut;

pub use {core::SAideApp, egui_command_binding::shortcut_map};
