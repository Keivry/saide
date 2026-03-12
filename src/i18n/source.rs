// SPDX-License-Identifier: MIT OR Apache-2.0

//! Traits and types for Fluent translation sources.
//!
//! This module defines the `FtlSource` trait, which abstracts the loading of
//! Fluent translation resources from various sources, such as the filesystem or
//! embedded resources.

use fluent_bundle::FluentResource;

pub trait FtlSource {
    fn available_locales(&self) -> Vec<String>;
    fn load_locale(&self, locale: &str) -> Result<Vec<FluentResource>, String>;
    #[cfg(debug_assertions)]
    fn as_watcher_source(&mut self) -> Option<&mut dyn super::fs_source::FtlWatcherSource> { None }
}
