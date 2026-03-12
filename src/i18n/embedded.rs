// SPDX-License-Identifier: MIT OR Apache-2.0

//! This module provides an implementation of FtlSource that uses
//! embedded Fluent translation files.
//!
//! The translations are embedded at compile time using the
//! `i18n-embedded` build script, which generates the necessary code
//! to include the FTL files into the binary.

include!(concat!(env!("OUT_DIR"), "/i18n_embedded.rs"));

pub struct EmbeddedFtlSource;

impl EmbeddedFtlSource {
    pub fn new() -> Self { Self }
}

impl super::FtlSource for EmbeddedFtlSource {
    fn available_locales(&self) -> Vec<String> {
        available_locales().iter().map(|s| s.to_string()).collect()
    }

    fn load_locale(&self, locale: &str) -> Result<Vec<fluent_bundle::FluentResource>, String> {
        load_locale(locale).ok_or_else(|| format!("Unknown locale: {}", locale))
    }
}
