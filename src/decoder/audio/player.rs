//! Audio playback using cpal

use {
    super::DecodedAudio,
    anyhow::{Context, Result},
    cpal::{
        traits::{DeviceTrait, HostTrait, StreamTrait},
        SampleRate, Stream, StreamConfig,
    },
    std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    tracing::{debug, info, warn},
};

/// Audio playback buffer size (milliseconds)
/// Increased for better network streaming stability
const BUFFER_MS: usize = 200;

/// Target buffering before starting playback (milliseconds)
/// Wait for initial buffering to avoid immediate underrun
const PREBUFFER_MS: usize = 100;

/// Ring buffer for audio samples
struct RingBuffer {
    buffer: Vec<f32>,
    capacity: usize,
    read_pos: usize,
    write_pos: usize,
    size: usize,
}

impl RingBuffer {
    fn new(capacity: usize) -> Self {
        Self {
            buffer: vec![0.0; capacity],
            capacity,
            read_pos: 0,
            write_pos: 0,
            size: 0,
        }
    }

    fn write(&mut self, data: &[f32]) -> usize {
        let available = self.capacity - self.size;
        let to_write = data.len().min(available);

        for &sample in &data[..to_write] {
            self.buffer[self.write_pos] = sample;
            self.write_pos = (self.write_pos + 1) % self.capacity;
            self.size += 1;
        }

        to_write
    }

    fn read(&mut self, output: &mut [f32]) -> usize {
        let to_read = output.len().min(self.size);

        for i in 0..to_read {
            output[i] = self.buffer[self.read_pos];
            self.read_pos = (self.read_pos + 1) % self.capacity;
            self.size -= 1;
        }

        // Fill remaining with silence if buffer underrun
        for i in to_read..output.len() {
            output[i] = 0.0;
        }

        to_read
    }

    fn fill_level(&self) -> f32 { self.size as f32 / self.capacity as f32 }
}

/// Audio player using cpal
pub struct AudioPlayer {
    /// Audio output stream
    _stream: Stream,

    /// Ring buffer (shared with callback)
    ring_buffer: Arc<Mutex<RingBuffer>>,

    /// Player running flag
    running: Arc<AtomicBool>,

    /// Playback started flag (after prebuffering)
    #[allow(dead_code)]
    started: Arc<AtomicBool>,

    /// Target prebuffer size (samples)
    #[allow(dead_code)]
    prebuffer_threshold: usize,

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
    pub fn new(sample_rate: u32, channels: u16) -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .context("No output device available")?;

        info!("Using audio device: {}", device.name()?);

        let config = StreamConfig {
            channels,
            sample_rate: SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        debug!("Audio config: {:?}", config);

        // Create ring buffer for audio samples
        let buffer_samples = (sample_rate as usize * BUFFER_MS / 1000) * channels as usize;
        let prebuffer_samples =
            (sample_rate as usize * PREBUFFER_MS / 1000) * channels as usize;
        let ring_buffer = Arc::new(Mutex::new(RingBuffer::new(buffer_samples)));

        let running = Arc::new(AtomicBool::new(true));
        let started = Arc::new(AtomicBool::new(false));

        let running_clone = running.clone();
        let started_clone = started.clone();
        let ring_buffer_clone = ring_buffer.clone();

        // Create audio output stream
        let stream = device.build_output_stream(
            &config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                audio_callback(
                    data,
                    &ring_buffer_clone,
                    &running_clone,
                    &started_clone,
                    prebuffer_samples,
                );
            },
            move |err| {
                warn!("Audio stream error: {}", err);
            },
            None,
        )?;

        stream.play()?;
        info!(
            "Audio player started: {}Hz, {} channels, buffer={}ms, prebuffer={}ms",
            sample_rate, channels, BUFFER_MS, PREBUFFER_MS
        );

        Ok(Self {
            _stream: stream,
            ring_buffer,
            running,
            started,
            prebuffer_threshold: prebuffer_samples,
            sample_rate,
            channels,
        })
    }

    /// Play decoded audio frame
    pub fn play(&self, audio: &DecodedAudio) -> Result<()> {
        // Validate sample rate and channels match
        if audio.sample_rate != self.sample_rate {
            anyhow::bail!(
                "Sample rate mismatch: expected {}, got {}",
                self.sample_rate,
                audio.sample_rate
            );
        }

        if audio.channels != self.channels {
            anyhow::bail!(
                "Channel count mismatch: expected {}, got {}",
                self.channels,
                audio.channels
            );
        }

        // Write samples to ring buffer
        let mut buffer = self.ring_buffer.lock().unwrap();
        let written = buffer.write(&audio.samples);

        if written < audio.samples.len() {
            warn!(
                "Buffer overflow: dropped {} samples",
                audio.samples.len() - written
            );
        }

        Ok(())
    }

    /// Get buffer fill level (0.0 to 1.0)
    pub fn buffer_level(&self) -> f32 {
        self.ring_buffer.lock().unwrap().fill_level()
    }

    /// Stop playback
    pub fn stop(&self) { self.running.store(false, Ordering::Relaxed); }
}

/// Audio output callback with prebuffering support
fn audio_callback(
    output: &mut [f32],
    ring_buffer: &Arc<Mutex<RingBuffer>>,
    running: &AtomicBool,
    started: &AtomicBool,
    prebuffer_threshold: usize,
) {
    if !running.load(Ordering::Relaxed) {
        output.fill(0.0);
        return;
    }

    let mut buffer = ring_buffer.lock().unwrap();

    // Check if we should start playback (prebuffering phase)
    let is_started = started.load(Ordering::Relaxed);
    if !is_started {
        // Wait until buffer is filled to prebuffer threshold
        if buffer.size < prebuffer_threshold {
            // Still prebuffering, output silence
            output.fill(0.0);
            return;
        }

        // Prebuffer complete, start playback
        started.store(true, Ordering::Relaxed);
        debug!("Audio prebuffering complete, starting playback");
    }

    // Normal playback: read from buffer
    buffer.read(output);
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        self.stop();
        debug!("Audio player stopped");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_player_creation() {
        let player = AudioPlayer::new(48000, 2);
        assert!(player.is_ok());
    }
}
