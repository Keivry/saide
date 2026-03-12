// SPDX-License-Identifier: MIT OR Apache-2.0

//! Configuration structures for scrcpy settings.
//!
//! This module defines the configuration structures used for scrcpy settings,
//! including video, audio, and other options.

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ScrcpyConfig {
    pub video: VideoConfig,
    pub audio: AudioConfig,
    pub options: OptionsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoConfig {
    #[serde(default = "default_bitrate")]
    pub bit_rate: String,
    #[serde(default = "default_min_fps")]
    pub min_fps: u32,
    #[serde(default = "default_max_fps")]
    pub max_fps: u32,
    #[serde(default = "default_max_size")]
    pub max_size: u32,
    #[serde(default = "default_codec")]
    pub codec: String,
    #[serde(default = "default_encoder")]
    pub encoder: Option<String>,
    /// Lock screen orientation during capture (0-3: portrait/landscape/portrait180/landscape180)
    /// None = auto-rotate with device, Some(0) = lock to portrait
    #[serde(default)]
    pub capture_orientation: Option<u32>,
}

impl Default for VideoConfig {
    fn default() -> Self {
        Self {
            bit_rate: default_bitrate(),
            min_fps: default_min_fps(),
            max_fps: default_max_fps(),
            max_size: default_max_size(),
            codec: default_codec(),
            encoder: default_encoder(),
            capture_orientation: None,
        }
    }
}

fn default_bitrate() -> String { "8M".to_string() }
fn default_min_fps() -> u32 { 5 }
fn default_max_fps() -> u32 { 60 }
fn default_max_size() -> u32 { 1920 }
fn default_codec() -> String { "h264".to_string() }
fn default_encoder() -> Option<String> { None }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    #[serde(default = "default_audio_enabled")]
    pub enabled: bool,
    #[serde(default = "default_audio_codec")]
    pub codec: String,
    #[serde(default = "default_audio_source")]
    pub source: String,
    /// Audio buffer size in frames (lower = less latency, higher = more stable)
    /// Typical values: 64 (1.33ms @ 48kHz), 128 (2.67ms), 256 (5.33ms)
    /// Default: 64 frames for minimal latency
    #[serde(default = "default_buffer_frames")]
    pub buffer_frames: u32,
    /// Ring buffer capacity in samples (affects internal audio buffering)
    /// Higher values = more buffering = higher latency but fewer glitches
    /// Default: 5760 samples
    #[serde(default = "default_ring_capacity")]
    pub ring_capacity: usize,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            enabled: default_audio_enabled(),
            codec: default_audio_codec(),
            source: default_audio_source(),
            buffer_frames: default_buffer_frames(),
            ring_capacity: default_ring_capacity(),
        }
    }
}

fn default_audio_enabled() -> bool { true }
fn default_audio_codec() -> String { "opus".to_string() }
fn default_audio_source() -> String { "playback".to_string() }
fn default_buffer_frames() -> u32 { 64 }
fn default_ring_capacity() -> usize { 5760 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionsConfig {
    #[serde(default = "default_true")]
    pub turn_screen_off: bool,
    #[serde(default = "default_true")]
    pub stay_awake: bool,
}

impl Default for OptionsConfig {
    fn default() -> Self {
        Self {
            turn_screen_off: default_true(),
            stay_awake: default_true(),
        }
    }
}

fn default_true() -> bool { true }
