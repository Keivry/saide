// SPDX-License-Identifier: MIT OR Apache-2.0

//! Public library entry points for the SAide workspace application crate.
//!
//! The reusable scrcpy protocol/runtime implementation now lives in the
//! workspace member `3rd-party/scrcpy-rs`. This crate keeps the desktop-specific UI,
//! configuration, controller, decoder, capture, and runtime orchestration
//! layers that build on top of that reusable crate.

pub mod avsync;
pub mod capture;
pub mod config;
pub mod constant;
pub mod controller;
pub mod core;
pub mod decoder;
pub mod decoder_probe;
pub mod error;
pub mod gpu;
pub mod i18n;
pub mod modal;
pub mod profiler;
pub mod runtime;
pub mod scrcpy;

#[macro_use]
pub mod shortcut;

pub use {
    core::{AppShell, SAideApp},
    egui_command_binding::shortcut_map,
};
