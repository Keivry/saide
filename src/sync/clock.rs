//! AV synchronization clock (scrcpy-style)
//!
//! Reference: scrcpy/app/src/clock.c
//!
//! Uses PTS-to-system-time mapping for minimal latency synchronization.
//! Both audio and video use the same PTS time base from the device.
//!
//! Lock-free architecture:
//! - Audio thread = master clock (唯一写者)
//! - Video thread = reads atomic snapshot (无锁)

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
    /// Sync threshold (microseconds)
    threshold_us: i64,
    /// Drift correction threshold (microseconds)
    drift_correction_threshold_us: i64,
    /// Drift history for averaging
    drift_history: Vec<i64>,
    max_drift_history: usize,
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
            threshold_us,
            drift_correction_threshold_us: 8000, // 8ms
            drift_history: Vec::with_capacity(10),
            max_drift_history: 10,
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

    /// Initialize clock with first video PTS
    ///
    /// CRITICAL: Only video should initialize the clock, never audio.
    /// Audio arrival time != video capture time, initializing with audio
    /// creates systematic drift in ≤20ms scenarios.
    pub fn init_clock_from_video(&mut self, first_video_pts: i64) {
        if self.clock.is_none() {
            self.clock = Some(AVClock::new(first_video_pts));
            // Mark clock as ready
            self.snapshot.clock_ready.store(true, Ordering::Release);
        }
    }

    /// Update audio PTS and drift (audio thread only)
    ///
    /// This is the ONLY method that writes to snapshot.
    /// Called by audio thread on every frame.
    pub fn update_audio_pts(&mut self, audio_pts: i64) {
        // Update snapshot atomically
        self.snapshot.audio_pts.store(audio_pts, Ordering::Release);

        // Update drift tracking
        if let Some(drift) = self.audio_drift_us(audio_pts) {
            self.drift_history.push(drift);
            if self.drift_history.len() > self.max_drift_history {
                self.drift_history.remove(0);
            }

            let avg_drift = if !self.drift_history.is_empty() {
                self.drift_history.iter().sum::<i64>() / self.drift_history.len() as i64
            } else {
                0
            };

            self.snapshot
                .avg_drift_us
                .store(avg_drift, Ordering::Release);
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
    /// Returns true if frame is more than threshold behind system time.
    /// Uses pure system-time comparison (scrcpy approach), avoiding
    /// current_pts() drift issues.
    pub fn should_drop_video(&self, pts: i64) -> bool {
        let clock = match &self.clock {
            Some(c) => c,
            None => return false,
        };

        let deadline = clock.pts_to_system_time(pts);
        let now = Instant::now();

        // Frame is late if deadline has passed by more than threshold
        now.saturating_duration_since(deadline).as_micros() as i64 > self.threshold_us
    }

    /// Check if audio should be played based on PTS
    ///
    /// Returns:
    /// - `AudioAction::Play` if audio is on-time (within threshold)
    /// - `AudioAction::Drop` if audio is too late (> threshold behind)
    /// - `AudioAction::Wait(duration)` if audio is too early
    pub fn check_audio_pts(&self, audio_pts: i64) -> AudioAction {
        let clock = match &self.clock {
            Some(c) => c,
            None => return AudioAction::Play, // No clock yet, play immediately
        };

        let deadline = clock.pts_to_system_time(audio_pts);
        let now = Instant::now();

        // Calculate how early/late the audio is
        if deadline > now {
            // Audio is early - should wait
            let wait_time = deadline.duration_since(now);
            if wait_time.as_micros() as i64 > self.threshold_us {
                // Too early, wait
                AudioAction::Wait(wait_time)
            } else {
                // Close enough, play now
                AudioAction::Play
            }
        } else {
            // Audio is late
            let lateness = now.saturating_duration_since(deadline);
            if lateness.as_micros() as i64 > self.threshold_us {
                // Too late, drop
                AudioAction::Drop
            } else {
                // Close enough, play now
                AudioAction::Play
            }
        }
    }

    /// Get sync drift in microseconds (positive = audio ahead, negative = audio behind)
    pub fn audio_drift_us(&self, audio_pts: i64) -> Option<i64> {
        let clock = self.clock.as_ref()?;
        let deadline = clock.pts_to_system_time(audio_pts);
        let now = Instant::now();

        if deadline > now {
            Some(deadline.duration_since(now).as_micros() as i64)
        } else {
            Some(-(now.saturating_duration_since(deadline).as_micros() as i64))
        }
    }

    /// Update drift tracking and get correction action
    ///
    /// Call this periodically (e.g., every audio frame) to track drift.
    /// Returns `DriftCorrection` action to take.
    pub fn update_drift(&mut self, audio_pts: i64) -> DriftCorrection {
        let drift = match self.audio_drift_us(audio_pts) {
            Some(d) => d,
            None => return DriftCorrection::None,
        };

        // Add to history
        self.drift_history.push(drift);
        if self.drift_history.len() > self.max_drift_history {
            self.drift_history.remove(0);
        }

        // Calculate average drift
        let avg_drift = self.drift_history.iter().sum::<i64>() / self.drift_history.len() as i64;

        // Check if correction is needed
        if avg_drift.abs() < self.drift_correction_threshold_us {
            DriftCorrection::None
        } else if avg_drift > 0 {
            // Audio ahead - slow down (drop occasional frame)
            DriftCorrection::DropFrame
        } else {
            // Audio behind - speed up (duplicate frame or reduce buffering)
            DriftCorrection::InsertSilence
        }
    }

    /// Get average drift from recent history
    pub fn average_drift_us(&self) -> i64 {
        if self.drift_history.is_empty() {
            0
        } else {
            self.drift_history.iter().sum::<i64>() / self.drift_history.len() as i64
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

/// Action to take for audio playback based on PTS
#[derive(Debug, Clone, Copy)]
pub enum AudioAction {
    /// Play audio immediately
    Play,
    /// Drop audio (too late)
    Drop,
    /// Wait before playing
    Wait(Duration),
}

/// Drift correction action
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriftCorrection {
    /// No correction needed
    None,
    /// Drop one audio frame (audio ahead)
    DropFrame,
    /// Insert silence (audio behind)
    InsertSilence,
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
        assert!((45_000..=55_000).contains(&estimated_pts));
    }

    #[test]
    fn test_av_sync_init() {
        let mut sync = AVSync::new(20);
        assert!(sync.clock().is_none());

        sync.init_clock_from_video(1000000);
        assert!(sync.clock().is_some());
        assert_eq!(sync.clock().unwrap().pts_base, 1000000);
    }

    #[test]
    fn test_time_until_pts() {
        let mut sync = AVSync::new(20);
        sync.init_clock_from_video(0);

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
        sync.init_clock_from_video(0);

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
