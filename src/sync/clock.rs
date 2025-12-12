//! AV synchronization clock (scrcpy-style)
//!
//! Reference: scrcpy/app/src/clock.c
//!
//! Uses PTS-to-system-time mapping for minimal latency synchronization.
//! Both audio and video use the same PTS time base from the device.

use std::time::{Duration, Instant};

/// Audio-video clock for PTS synchronization
///
/// Maps device PTS (microseconds) to system monotonic time.
/// This is the core of scrcpy's low-latency AV sync.
#[derive(Debug, Clone)]
pub struct AVClock {
    /// First PTS received (microseconds)
    pts_base: i64,
    /// System time when first PTS was received
    system_base: Instant,
}

impl AVClock {
    /// Create new AV clock with first frame PTS
    pub fn new(first_pts: i64) -> Self {
        Self {
            pts_base: first_pts,
            system_base: Instant::now(),
        }
    }

    /// Convert PTS to absolute system time
    ///
    /// Formula: system_time = system_base + (pts - pts_base)
    pub fn pts_to_system_time(&self, pts: i64) -> Instant {
        let offset_us = pts.saturating_sub(self.pts_base);
        if offset_us >= 0 {
            self.system_base + Duration::from_micros(offset_us as u64)
        } else {
            // Negative offset (shouldn't happen with monotonic PTS)
            self.system_base
                .checked_sub(Duration::from_micros((-offset_us) as u64))
                .unwrap_or(self.system_base)
        }
    }

    /// Get elapsed time since clock start
    pub fn elapsed(&self) -> Duration { self.system_base.elapsed() }

    /// Get current PTS estimate based on wall clock
    pub fn current_pts(&self) -> i64 {
        let elapsed_us = self.system_base.elapsed().as_micros() as i64;
        self.pts_base + elapsed_us
    }
}

/// AV sync state for coordinating audio and video
///
/// Implements scrcpy's synchronization strategy:
/// - Video: render at PTS deadline (minimal buffering)
/// - Audio: adaptive buffering with compensation
#[derive(Debug)]
pub struct AVSync {
    clock: Option<AVClock>,
    /// Sync threshold (microseconds)
    /// Frames within ±threshold are considered in sync
    threshold_us: i64,
}

impl AVSync {
    /// Create new AV sync controller
    ///
    /// # Arguments
    /// * `threshold_ms` - Sync threshold in milliseconds (default: 20ms)
    pub fn new(threshold_ms: u32) -> Self {
        Self {
            clock: None,
            threshold_us: threshold_ms as i64 * 1000,
        }
    }

    /// Initialize clock with first PTS (video or audio)
    pub fn init_clock(&mut self, first_pts: i64) {
        if self.clock.is_none() {
            self.clock = Some(AVClock::new(first_pts));
        }
    }

    /// Get current AV clock
    pub fn clock(&self) -> Option<&AVClock> { self.clock.as_ref() }

    /// Calculate sleep duration until PTS deadline
    ///
    /// Returns:
    /// - `Some(duration)` if frame is early (should wait)
    /// - `None` if frame is late or on-time (render immediately)
    pub fn time_until_pts(&self, pts: i64) -> Option<Duration> {
        let clock = self.clock.as_ref()?;
        let deadline = clock.pts_to_system_time(pts);
        let now = Instant::now();

        if deadline > now {
            Some(deadline.duration_since(now))
        } else {
            None
        }
    }

    /// Check if video frame should be dropped (too late)
    ///
    /// Returns true if frame is more than threshold behind current time
    pub fn should_drop_video(&self, pts: i64) -> bool {
        if let Some(clock) = &self.clock {
            let current_pts = clock.current_pts();
            current_pts - pts > self.threshold_us
        } else {
            false
        }
    }

    /// Get sync status for debugging
    pub fn sync_status(&self, video_pts: i64, audio_pts: i64) -> SyncStatus {
        let diff_us = video_pts - audio_pts;
        let diff_ms = diff_us / 1000;

        SyncStatus {
            video_pts,
            audio_pts,
            diff_us,
            diff_ms,
            in_sync: diff_us.abs() < self.threshold_us,
        }
    }
}

impl Default for AVSync {
    fn default() -> Self { Self::new(20) }
}

/// Sync status for monitoring
#[derive(Debug, Clone, Copy)]
pub struct SyncStatus {
    pub video_pts: i64,
    pub audio_pts: i64,
    pub diff_us: i64,
    pub diff_ms: i64,
    pub in_sync: bool,
}

#[cfg(test)]
mod tests {
    use {super::*, std::thread};

    #[test]
    fn test_avclock_basic() {
        let clock = AVClock::new(1000000); // 1 second
        assert_eq!(clock.pts_base, 1000000);
    }

    #[test]
    fn test_pts_to_system_time() {
        let clock = AVClock::new(0);
        let future_pts = 100_000; // 100ms

        let target = clock.pts_to_system_time(future_pts);
        let now = Instant::now();

        // Target should be ~100ms in the future
        assert!(target > now);
        let diff = target.duration_since(now);
        assert!(diff.as_millis() >= 95 && diff.as_millis() <= 105);
    }

    #[test]
    fn test_current_pts_estimation() {
        let clock = AVClock::new(0);
        thread::sleep(Duration::from_millis(50));

        let estimated_pts = clock.current_pts();
        // Should be approximately 50ms = 50000µs
        assert!(estimated_pts >= 45_000 && estimated_pts <= 55_000);
    }

    #[test]
    fn test_av_sync_init() {
        let mut sync = AVSync::new(20);
        assert!(sync.clock().is_none());

        sync.init_clock(1000000);
        assert!(sync.clock().is_some());
        assert_eq!(sync.clock().unwrap().pts_base, 1000000);
    }

    #[test]
    fn test_time_until_pts() {
        let mut sync = AVSync::new(20);
        sync.init_clock(0);

        // Frame 100ms in the future
        let future_pts = 100_000;
        let wait_time = sync.time_until_pts(future_pts);

        assert!(wait_time.is_some());
        let duration = wait_time.unwrap();
        assert!(duration.as_millis() >= 95 && duration.as_millis() <= 105);
    }

    #[test]
    fn test_should_drop_video() {
        let mut sync = AVSync::new(20);
        sync.init_clock(0);

        thread::sleep(Duration::from_millis(50));

        // Old frame (100ms behind)
        assert!(sync.should_drop_video(-100_000));

        // Current frame
        assert!(!sync.should_drop_video(sync.clock().unwrap().current_pts()));
    }

    #[test]
    fn test_sync_status() {
        let sync = AVSync::new(20);
        let status = sync.sync_status(1000000, 999000);

        assert_eq!(status.diff_us, 1000);
        assert_eq!(status.diff_ms, 1);
        assert!(status.in_sync); // Within 20ms threshold
    }
}
