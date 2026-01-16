//! Latency measurement and profiling
//!
//! Tracks end-to-end latency from device capture to screen display.
//! Reference: docs/LATENCY_OPTIMIZATION.md

use std::time::{Duration, Instant};

/// Latency profiler - tracks timestamps at each pipeline stage
///
/// # Pipeline Stages
///
/// 1. **Capture**: Device captures frame (estimated from PTS)
/// 2. **Receive**: First byte arrives via TCP
/// 3. **Decode**: Frame fully decoded
/// 4. **Upload**: Frame uploaded to GPU
/// 5. **Display**: Frame rendered to screen (vsync)
///
/// # Usage
///
/// ```rust,ignore
/// let mut profiler = LatencyProfiler::new();
///
/// // Mark capture time (from PTS)
/// profiler.mark_capture(clock.pts_to_system_time(pts));
///
/// // Mark receive time
/// profiler.mark_receive();
///
/// // Mark decode completion
/// profiler.mark_decode();
///
/// // Mark GPU upload
/// profiler.mark_upload();
///
/// // Mark display (vsync)
/// profiler.mark_display();
///
/// // Get breakdown
/// if let Some(breakdown) = profiler.breakdown() {
///     println!("Network: {:.1}ms", breakdown.network.as_secs_f64() * 1000.0);
///     println!("Total: {:.1}ms", profiler.end_to_end_latency().unwrap().as_secs_f64() * 1000.0);
/// }
/// ```
#[derive(Debug, Clone, Default)]
pub struct LatencyProfiler {
    /// Device capture time (from PTS → system time mapping)
    pub capture_time: Option<Instant>,

    /// TCP receive time (first byte arrival)
    pub receive_time: Option<Instant>,

    /// Decode completion time
    pub decode_time: Option<Instant>,

    /// GPU upload completion time
    pub upload_time: Option<Instant>,

    /// Display time (frame rendered to screen, vsync)
    pub display_time: Option<Instant>,
}

/// Latency breakdown by stage
#[derive(Debug, Clone, Copy)]
pub struct LatencyBreakdown {
    /// Network transmission latency (capture → receive)
    pub network: Duration,

    /// Decode latency (receive → decode)
    pub decode: Duration,

    /// GPU upload latency (decode → upload)
    pub upload: Duration,

    /// Render latency (upload → display)
    pub render: Duration,

    /// Total end-to-end latency
    pub total: Duration,
}

impl LatencyProfiler {
    /// Create a new latency profiler
    pub fn new() -> Self { Self::default() }

    /// Mark device capture time (estimated from PTS)
    pub fn mark_capture(&mut self, time: Instant) { self.capture_time = Some(time); }

    /// Mark TCP receive time (now)
    pub fn mark_receive(&mut self) { self.receive_time = Some(Instant::now()); }

    /// Mark decode completion time (now)
    pub fn mark_decode(&mut self) { self.decode_time = Some(Instant::now()); }

    /// Mark GPU upload completion time (now)
    pub fn mark_upload(&mut self) { self.upload_time = Some(Instant::now()); }

    /// Mark display time (now)
    pub fn mark_display(&mut self) { self.display_time = Some(Instant::now()); }

    /// Reset all timestamps
    pub fn reset(&mut self) { *self = Self::default(); }

    /// Calculate end-to-end latency (capture → display)
    pub fn end_to_end_latency(&self) -> Option<Duration> {
        Some(self.display_time?.duration_since(self.capture_time?))
    }

    /// Calculate latency breakdown by stage
    ///
    /// Returns `None` if any required timestamp is missing.
    pub fn breakdown(&self) -> Option<LatencyBreakdown> {
        let capture = self.capture_time?;
        let receive = self.receive_time?;
        let decode = self.decode_time?;
        let upload = self.upload_time?;
        let display = self.display_time?;

        Some(LatencyBreakdown {
            network: receive.saturating_duration_since(capture),
            decode: decode.saturating_duration_since(receive),
            upload: upload.saturating_duration_since(decode),
            render: display.saturating_duration_since(upload),
            total: display.saturating_duration_since(capture),
        })
    }

    /// Check if profiler has complete data
    pub fn is_complete(&self) -> bool {
        self.capture_time.is_some()
            && self.receive_time.is_some()
            && self.decode_time.is_some()
            && self.upload_time.is_some()
            && self.display_time.is_some()
    }
}

impl LatencyBreakdown {
    /// Format breakdown as human-readable string
    pub fn format(&self) -> String {
        format!(
            "Network: {:4.1}ms | Decode: {:4.1}ms | Upload: {:4.1}ms | Render: {:4.1}ms | Total: {:4.1}ms",
            self.network.as_secs_f64() * 1000.0,
            self.decode.as_secs_f64() * 1000.0,
            self.upload.as_secs_f64() * 1000.0,
            self.render.as_secs_f64() * 1000.0,
            self.total.as_secs_f64() * 1000.0,
        )
    }

    /// Get total latency in milliseconds
    pub fn total_ms(&self) -> f64 { self.total.as_secs_f64() * 1000.0 }

    /// Get network latency in milliseconds
    pub fn network_ms(&self) -> f64 { self.network.as_secs_f64() * 1000.0 }

    /// Get decode latency in milliseconds
    pub fn decode_ms(&self) -> f64 { self.decode.as_secs_f64() * 1000.0 }

    /// Get upload latency in milliseconds
    pub fn upload_ms(&self) -> f64 { self.upload.as_secs_f64() * 1000.0 }

    /// Get render latency in milliseconds
    pub fn render_ms(&self) -> f64 { self.render.as_secs_f64() * 1000.0 }
}

/// Latency statistics aggregator
///
/// Collects latency samples and computes running statistics.
#[derive(Debug, Clone)]
pub struct LatencyStats {
    /// Sample window (number of frames to average)
    window_size: usize,

    /// Recent samples (ring buffer)
    samples: Vec<f64>,

    /// Current write position
    write_pos: usize,

    /// Number of samples collected
    count: usize,
}

impl LatencyStats {
    /// Create new statistics aggregator
    ///
    /// # Arguments
    /// * `window_size` - Number of samples to average (e.g., 60 for 1 second @ 60fps)
    pub fn new(window_size: usize) -> Self {
        Self {
            window_size,
            samples: vec![0.0; window_size],
            write_pos: 0,
            count: 0,
        }
    }

    /// Add a latency sample (in milliseconds)
    pub fn add_sample(&mut self, latency_ms: f64) {
        self.samples[self.write_pos] = latency_ms;
        self.write_pos = (self.write_pos + 1) % self.window_size;
        self.count = (self.count + 1).min(self.window_size);
    }

    /// Get average latency (in milliseconds)
    pub fn average(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }

        let sum: f64 = self.samples.iter().take(self.count).sum();
        sum / self.count as f64
    }

    /// Get minimum latency (in milliseconds)
    pub fn min(&self) -> f64 {
        self.samples
            .iter()
            .take(self.count)
            .copied()
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0)
    }

    /// Get maximum latency (in milliseconds)
    pub fn max(&self) -> f64 {
        self.samples
            .iter()
            .take(self.count)
            .copied()
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0)
    }

    /// Get 95th percentile latency (in milliseconds)
    pub fn p95(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }

        let mut sorted: Vec<f64> = self.samples.iter().take(self.count).copied().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let idx = (self.count as f64 * 0.95) as usize;
        sorted[idx.min(self.count - 1)]
    }

    /// Reset statistics
    pub fn reset(&mut self) {
        self.samples.fill(0.0);
        self.write_pos = 0;
        self.count = 0;
    }

    /// Check if statistics are ready (have enough samples)
    pub fn is_ready(&self) -> bool { self.count >= self.window_size / 2 }
}

#[cfg(test)]
mod tests {
    use {super::*, std::thread};

    #[test]
    fn test_profiler_basic() {
        let mut profiler = LatencyProfiler::new();

        let base_time = Instant::now();
        profiler.mark_capture(base_time);

        thread::sleep(Duration::from_millis(5));
        profiler.mark_receive();

        thread::sleep(Duration::from_millis(10));
        profiler.mark_decode();

        thread::sleep(Duration::from_millis(3));
        profiler.mark_upload();

        thread::sleep(Duration::from_millis(2));
        profiler.mark_display();

        assert!(profiler.is_complete());

        let breakdown = profiler.breakdown().unwrap();
        assert!(breakdown.network_ms() >= 4.0 && breakdown.network_ms() <= 6.0);
        assert!(breakdown.decode_ms() >= 9.0 && breakdown.decode_ms() <= 11.0);
        assert!(breakdown.upload_ms() >= 2.0 && breakdown.upload_ms() <= 4.0);
        assert!(breakdown.render_ms() >= 1.0 && breakdown.render_ms() <= 3.0);
        assert!(breakdown.total_ms() >= 19.0 && breakdown.total_ms() <= 21.0);
    }

    #[test]
    fn test_profiler_incomplete() {
        let mut profiler = LatencyProfiler::new();
        profiler.mark_capture(Instant::now());
        profiler.mark_receive();

        assert!(!profiler.is_complete());
        assert!(profiler.breakdown().is_none());
    }

    #[test]
    fn test_latency_stats() {
        let mut stats = LatencyStats::new(5);

        stats.add_sample(10.0);
        stats.add_sample(20.0);
        stats.add_sample(15.0);
        stats.add_sample(25.0);
        stats.add_sample(30.0);

        assert_eq!(stats.average(), 20.0);
        assert_eq!(stats.min(), 10.0);
        assert_eq!(stats.max(), 30.0);
        assert!(stats.is_ready());
    }

    #[test]
    fn test_latency_stats_p95() {
        let mut stats = LatencyStats::new(100);

        for i in 1..=100 {
            stats.add_sample(i as f64);
        }

        let p95 = stats.p95();
        assert!((94.0..=96.0).contains(&p95));
    }

    #[test]
    fn test_latency_stats_ring_buffer() {
        let mut stats = LatencyStats::new(3);

        stats.add_sample(10.0);
        stats.add_sample(20.0);
        stats.add_sample(30.0);
        stats.add_sample(40.0); // Overwrites 10.0

        assert_eq!(stats.average(), 30.0); // (20 + 30 + 40) / 3
        assert_eq!(stats.min(), 20.0);
        assert_eq!(stats.max(), 40.0);
    }
}
