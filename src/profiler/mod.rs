// SPDX-License-Identifier: MIT OR Apache-2.0

//! Performance profiling and latency measurement

pub mod latency;

pub use latency::{LatencyBreakdown, LatencyProfiler, LatencyStats};
