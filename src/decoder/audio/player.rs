//! Audio playback using cpal
//!
//! Ultra-low latency design:
//! - Lock-free ring buffer (rtrb, sample-level buffering)
//! - Small fixed buffer size (128 frames = 2.67ms @ 48kHz)
//! - No prebuffering (first sample plays immediately)
//! - Underrun = silence (minimal latency over glitch-free)

use {
    super::{
        DecodedAudio,
        error::{AudioError, Result},
    },
    cpal::{
        BufferSize,
        Stream,
        StreamConfig,
        traits::{DeviceTrait, HostTrait, StreamTrait},
    },
    rtrb::{Consumer, Producer, RingBuffer},
    std::{
        sync::{
            Arc,
            Mutex,
            atomic::{AtomicBool, AtomicU64, Ordering},
        },
        thread,
        time::Duration,
    },
    tracing::{debug, info, warn},
};

/// Audio player using cpal with lock-free ring buffer
pub struct AudioPlayer {
    /// Audio output stream
    _stream: Stream,

    /// Ring buffer producer (thread-safe access via Mutex)
    producer: Arc<Mutex<Producer<f32>>>,

    /// Player running flag
    running: Arc<AtomicBool>,

    /// Underrun counter (for diagnostics)
    underruns: Arc<AtomicU64>,

    /// Sample rate
    sample_rate: u32,

    /// Number of channels
    channels: u16,
}

impl AudioPlayer {
    /// Create new audio player
    ///
    /// # Arguments
    /// * `sample_rate` - Audio sample rate (Hz)
    /// * `channels` - Number of channels (1=mono, 2=stereo)
    /// * `buffer_frames` - Buffer size in frames (lower = less latency, higher = more stable)
    /// * `ring_capacity` - Ring buffer capacity in samples
    pub fn new(
        sample_rate: u32,
        channels: u16,
        buffer_frames: u32,
        ring_capacity: usize,
    ) -> Result<Self> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or_else(|| {
            AudioError::InitializationError("No default audio output device found".to_string())
        })?;

        let description = device.description().map_err(|e| {
            AudioError::InitializationError(format!("Failed to get device description: {}", e))
        })?;
        info!("Using audio device: {}", description.name());

        let config = StreamConfig {
            channels,
            sample_rate,
            buffer_size: BufferSize::Fixed(buffer_frames),
        };

        debug!("Audio config: {:?}", config);

        // Create lock-free ring buffer (sample-level)
        let (producer, mut consumer) = RingBuffer::<f32>::new(ring_capacity);

        let running = Arc::new(AtomicBool::new(true));
        let underruns = Arc::new(AtomicU64::new(0));

        let running_clone = running.clone();
        let underruns_clone = underruns.clone();

        // Create audio output stream
        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    audio_callback(data, &mut consumer, &running_clone, &underruns_clone);
                },
                move |err| {
                    warn!("Audio stream error: {}", err);
                },
                None,
            )
            .map_err(|e| {
                AudioError::InitializationError(format!("Failed to create audio stream: {}", e))
            })?;

        stream.play().map_err(|e| {
            AudioError::InitializationError(format!("Failed to start audio stream: {}", e))
        })?;

        info!(
            "Audio player started: {}Hz, {} channels, buffer={}frames ({:.2}ms)",
            sample_rate,
            channels,
            buffer_frames,
            (buffer_frames as f64 / sample_rate as f64 * 1000.0)
        );

        Ok(Self {
            _stream: stream,
            producer: Arc::new(Mutex::new(producer)),
            running,
            underruns,
            sample_rate,
            channels,
        })
    }

    /// Play decoded audio frame
    pub fn play(&self, audio: &DecodedAudio) -> Result<()> {
        // Validate sample rate and channels match
        if audio.sample_rate != self.sample_rate {
            return Err(AudioError::PlaybackError(format!(
                "Sample rate mismatch: expected {}, got {}",
                self.sample_rate, audio.sample_rate
            )));
        }

        if audio.channels != self.channels {
            return Err(AudioError::PlaybackError(format!(
                "Channel count mismatch: expected {}, got {}",
                self.channels, audio.channels
            )));
        }

        // Write samples to ring buffer (sample-by-sample, lock-free ring)
        let written = match self
            .producer
            .lock()
            .expect("Mutex poisoned")
            .write_chunk_uninit(audio.samples.len())
        {
            Ok(chunk) => {
                // fill_from_iter() consumes chunk and commits automatically
                let len = chunk.len();
                chunk.fill_from_iter(audio.samples.iter().copied());
                len
            }
            Err(_) => 0,
        };

        if written < audio.samples.len() {
            debug!(
                "Buffer overflow: dropped {} samples (buffer full)",
                audio.samples.len() - written
            );
        }

        Ok(())
    }

    /// Get underrun count
    pub fn underrun_count(&self) -> u64 { self.underruns.load(Ordering::Relaxed) }

    /// Stop playback
    pub fn stop(&self) { self.running.store(false, Ordering::Relaxed); }
}

/// Audio output callback (NO MUTEX, lock-free)
fn audio_callback(
    output: &mut [f32],
    consumer: &mut Consumer<f32>,
    running: &AtomicBool,
    underruns: &AtomicU64,
) {
    if !running.load(Ordering::Relaxed) {
        output.fill(0.0);
        return;
    }

    // Read samples from lock-free ring buffer
    let read = match consumer.read_chunk(output.len()) {
        Ok(chunk) => {
            let (first, second) = chunk.as_slices();

            // Defensive check: ensure we don't overflow output buffer
            let first_len = first.len().min(output.len());
            let second_start = first_len;
            let second_len = second.len().min(output.len().saturating_sub(second_start));

            output[..first_len].copy_from_slice(&first[..first_len]);
            if second_len > 0 {
                output[second_start..second_start + second_len]
                    .copy_from_slice(&second[..second_len]);
            }

            chunk.commit_all();
            first_len + second_len
        }
        Err(_) => 0,
    };

    // If underrun (not enough data), fill rest with silence
    if read < output.len() {
        output[read..].fill(0.0);
        if read == 0 {
            underruns.fetch_add(1, Ordering::Relaxed);
        }
    }
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        self.stop();

        // Give audio stream time to finish current callback
        thread::sleep(Duration::from_millis(50));

        let underruns = self.underrun_count();
        if underruns > 0 {
            debug!("Audio player stopped ({} underruns)", underruns);
        } else {
            debug!("Audio player stopped");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_player_creation() {
        let player = AudioPlayer::new(48000, 2, 64, 5760);
        assert!(player.is_ok());
    }
}
