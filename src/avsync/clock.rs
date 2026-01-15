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
    collections::VecDeque,
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

/// Network jitter estimator for dynamic threshold adjustment
///
/// Tracks recent frame arrival intervals to estimate network jitter.
/// Used to dynamically adjust sync threshold:
/// - Low jitter → tighter threshold (lower latency)
/// - High jitter → looser threshold (fewer drops)
#[derive(Debug)]
struct JitterEstimator {
    /// Recent inter-arrival deltas (microseconds)
    samples: VecDeque<i64>,
    /// Maximum samples to keep
    max_samples: usize,
    /// Last PTS seen
    last_pts: Option<i64>,
}

impl JitterEstimator {
    fn new(max_samples: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(max_samples),
            max_samples,
            last_pts: None,
        }
    }

    /// Update with new PTS, returns estimated jitter (microseconds)
    fn update(&mut self, pts: i64) -> i64 {
        if let Some(last) = self.last_pts {
            let delta = pts - last;
            if delta > 0 {
                self.samples.push_back(delta);
                if self.samples.len() > self.max_samples {
                    self.samples.pop_front();
                }
            }
        }
        self.last_pts = Some(pts);
        self.estimate_jitter()
    }

    /// Calculate jitter as standard deviation of deltas
    fn estimate_jitter(&self) -> i64 {
        if self.samples.len() < 2 {
            return 10_000; // Default 10ms
        }

        let mean: i64 = self.samples.iter().sum::<i64>() / self.samples.len() as i64;
        let variance: i64 = self
            .samples
            .iter()
            .map(|&x| {
                let diff = x - mean;
                diff * diff
            })
            .sum::<i64>()
            / self.samples.len() as i64;

        (variance as f64).sqrt() as i64
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
    /// Dynamic sync threshold (microseconds)
    threshold_us: AtomicI64,
    /// Base threshold (microseconds)
    base_threshold_us: i64,
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
            return false;
        }

        let audio_pts = self.audio_pts.load(Ordering::Acquire);
        let drift = video_pts - audio_pts;
        let threshold = self.threshold_us.load(Ordering::Acquire);

        drift < -threshold
    }

    /// Get average drift in microseconds
    pub fn avg_drift(&self) -> i64 { self.avg_drift_us.load(Ordering::Acquire) }

    /// Get current audio PTS
    pub fn audio_pts(&self) -> i64 { self.audio_pts.load(Ordering::Acquire) }

    /// Get current sync threshold (microseconds)
    pub fn threshold_us(&self) -> i64 { self.threshold_us.load(Ordering::Acquire) }
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
    /// Jitter estimator for dynamic threshold
    jitter: JitterEstimator,
}

impl AVSync {
    /// Create new AV sync controller
    ///
    /// # Arguments
    /// * `threshold_ms` - Base sync threshold in milliseconds (default: 20ms)
    pub fn new(threshold_ms: u32) -> Self {
        let threshold_us = threshold_ms as i64 * 1000;
        Self {
            clock: None,
            snapshot: Arc::new(AVSyncSnapshot {
                audio_pts: AtomicI64::new(0),
                avg_drift_us: AtomicI64::new(0),
                threshold_us: AtomicI64::new(threshold_us),
                base_threshold_us: threshold_us,
                clock_ready: AtomicBool::new(false),
            }),
            jitter: JitterEstimator::new(30),
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
        if self.clock.is_none() {
            self.clock = Some(AVClock::new(audio_pts));
            self.snapshot.clock_ready.store(true, Ordering::Release);
        }

        self.snapshot.audio_pts.store(audio_pts, Ordering::Release);

        let jitter_us = self.jitter.update(audio_pts);
        let dynamic_threshold = self.snapshot.base_threshold_us + (jitter_us * 2);
        self.snapshot
            .threshold_us
            .store(dynamic_threshold, Ordering::Release);
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
