//! Audio playback using cpal

use {
    super::DecodedAudio,
    anyhow::{Context, Result},
    cpal::{
        traits::{DeviceTrait, HostTrait, StreamTrait},
        SampleRate, Stream, StreamConfig,
    },
    crossbeam_channel::{bounded, Receiver, Sender},
    std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    tracing::{debug, info, warn},
};

/// Audio playback buffer size (milliseconds)
const BUFFER_MS: usize = 100;

/// Audio player using cpal
pub struct AudioPlayer {
    /// Audio output stream
    _stream: Stream,

    /// Audio sample sender
    sample_tx: Sender<f32>,

    /// Player running flag
    running: Arc<AtomicBool>,

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
        let (sample_tx, sample_rx) = bounded::<f32>(buffer_samples);

        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        // Create audio output stream
        let stream = device.build_output_stream(
            &config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                audio_callback(data, &sample_rx, &running_clone);
            },
            move |err| {
                warn!("Audio stream error: {}", err);
            },
            None,
        )?;

        stream.play()?;
        info!(
            "Audio player started: {}Hz, {} channels, {}ms buffer",
            sample_rate, channels, BUFFER_MS
        );

        Ok(Self {
            _stream: stream,
            sample_tx,
            running,
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

        // Send samples to playback thread
        for &sample in &audio.samples {
            self.sample_tx
                .send(sample)
                .context("Failed to send audio sample")?;
        }

        Ok(())
    }

    /// Get buffer fill level (0.0 to 1.0)
    pub fn buffer_level(&self) -> f32 {
        let capacity = self.sample_tx.capacity().unwrap_or(1);
        let available = capacity - self.sample_tx.len();
        available as f32 / capacity as f32
    }

    /// Stop playback
    pub fn stop(&self) { self.running.store(false, Ordering::Relaxed); }
}

/// Audio output callback
fn audio_callback(output: &mut [f32], sample_rx: &Receiver<f32>, running: &AtomicBool) {
    if !running.load(Ordering::Relaxed) {
        output.fill(0.0);
        return;
    }

    for sample in output.iter_mut() {
        *sample = sample_rx.try_recv().unwrap_or(0.0);
    }
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
