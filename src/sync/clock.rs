//! AV synchronization clock (scrcpy-style)
//!
//! Reference: scrcpy/app/src/clock.c
//!
//! Uses PTS-to-system-time mapping for minimal latency synchronization.
//! Both audio and video use the same PTS time base from the device.
//!
//! Lock-free architecture:
//! - Audio thread = master clock (unique writer)
//! - Video thread = reads atomic snapshot (non-locking)

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicI64, Ordering},
    },
    time::{Duration, Instant},
};

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

/// Atomic snapshot for video thread (read-only)
///
/// Lock-free access to current sync state.
/// Updated only by audio thread via Release ordering.
#[derive(Debug)]
pub struct AVSyncSnapshot {
    /// Current audio PTS (microseconds)
    audio_pts: AtomicI64,
    /// Average drift (microseconds, positive = audio ahead)
    avg_drift_us: AtomicI64,
    /// Sync threshold (microseconds)
    threshold_us: i64,
    /// Whether clock is initialized
    clock_ready: AtomicBool,
}

impl AVSyncSnapshot {
    /// Check if video frame should be dropped (too late)
    ///
    /// Uses audio PTS as reference. Frame is late if it's behind
    /// audio by more than threshold.
    pub fn should_drop_video(&self, video_pts: i64) -> bool {
        if !self.clock_ready.load(Ordering::Acquire) {
            return false; // No clock yet, don't drop
        }

        let audio_pts = self.audio_pts.load(Ordering::Acquire);
        let drift = video_pts - audio_pts;

        // Video too far behind audio → drop
        drift < -(self.threshold_us)
    }

    /// Get average drift in microseconds
    pub fn avg_drift(&self) -> i64 { self.avg_drift_us.load(Ordering::Acquire) }

    /// Get current audio PTS
    pub fn audio_pts(&self) -> i64 { self.audio_pts.load(Ordering::Acquire) }

    /// Get sync threshold
    pub fn threshold_us(&self) -> i64 { self.threshold_us }
}

/// AV sync state for coordinating audio and video
///
/// Lock-free architecture:
/// - Audio thread holds `&mut AVSync` (唯一写者)
/// - Video thread holds `Arc<AVSyncSnapshot>` (只读)
///
/// Implements scrcpy's synchronization strategy:
/// - Audio = master clock
/// - Video = reads snapshot, drops late frames
#[derive(Debug)]
pub struct AVSync {
    clock: Option<AVClock>,
    /// Atomic snapshot for video thread
    snapshot: Arc<AVSyncSnapshot>,
}

impl AVSync {
    /// Create new AV sync controller
    ///
    /// # Arguments
    /// * `threshold_ms` - Sync threshold in milliseconds (default: 20ms)
    pub fn new(threshold_ms: u32) -> Self {
        let threshold_us = threshold_ms as i64 * 1000;
        Self {
            clock: None,
            snapshot: Arc::new(AVSyncSnapshot {
                audio_pts: AtomicI64::new(0),
                avg_drift_us: AtomicI64::new(0),
                threshold_us,
                clock_ready: AtomicBool::new(false),
            }),
        }
    }

    /// Get snapshot for video thread (lock-free read)
    pub fn snapshot(&self) -> Arc<AVSyncSnapshot> { Arc::clone(&self.snapshot) }

    /// Update audio PTS (audio thread only)
    ///
    /// This is the ONLY method that writes to snapshot.
    /// Called by audio thread on every frame.
    /// Automatically initializes clock on first call.
    pub fn update_audio_pts(&mut self, audio_pts: i64) {
        // Auto-initialize clock on first audio frame
        if self.clock.is_none() {
            self.clock = Some(AVClock::new(audio_pts));
            self.snapshot.clock_ready.store(true, Ordering::Release);
        }

        // Update snapshot atomically
        self.snapshot.audio_pts.store(audio_pts, Ordering::Release);
    }
}

impl Default for AVSync {
    fn default() -> Self { Self::new(20) }
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
        assert!((45_000..=55_000).contains(&estimated_pts));
    }

    #[test]
    fn test_av_sync_init() {
        let mut sync = AVSync::new(20);

        // Clock auto-initialized on first audio frame
        sync.update_audio_pts(1000000);

        // Verify snapshot is updated
        let snapshot = sync.snapshot();
        assert_eq!(snapshot.audio_pts(), 1000000);
        assert!(snapshot.clock_ready.load(Ordering::Acquire));
    }

    #[test]
    fn test_snapshot_should_drop_video() {
        let mut sync = AVSync::new(20);

        // Initialize with audio
        sync.update_audio_pts(1000000);
        let snapshot = sync.snapshot();

        // Video far behind audio → should drop
        assert!(snapshot.should_drop_video(950_000)); // 50ms behind

        // Video close to audio → should not drop
        assert!(!snapshot.should_drop_video(990_000)); // 10ms behind

        // Video ahead of audio → should not drop
        assert!(!snapshot.should_drop_video(1_010_000));
    }
}
