//! Audio playback using [`cpal`].
//!
//! Opens the default output device and plays mono f32 audio.  Playback is
//! non-blocking — a cpal output stream is started and audio is fed from a
//! shared buffer via the callback.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use tracing::{debug, warn};

use crate::error::VoiceError;

/// Audio playback handle.
///
/// Plays f32 audio samples through the default output device.  The playback
/// is callback-driven and non-blocking.
pub struct AudioPlayback {
    /// Active output stream (present while playing).
    stream: Option<cpal::Stream>,
    /// Shared flag to signal playback completion.
    finished: Arc<AtomicBool>,
}

impl AudioPlayback {
    /// Create a new playback handle.
    pub fn new() -> Self {
        Self {
            stream: None,
            finished: Arc::new(AtomicBool::new(true)),
        }
    }

    /// Play audio samples at the given sample rate.
    ///
    /// This is non-blocking — it starts a cpal output stream that drains the
    /// provided sample buffer.  If audio is already playing, it is stopped
    /// first.
    pub fn play(&mut self, samples: &[f32], sample_rate: u32) -> Result<(), VoiceError> {
        // Stop any previous playback.
        self.stop();

        if samples.is_empty() {
            return Ok(());
        }

        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| VoiceError::AudioDevice("no default output device".into()))?;

        debug!(
            device = ?device.name(),
            sample_rate,
            samples = samples.len(),
            "starting playback"
        );

        let config = cpal::StreamConfig {
            channels: 1,
            sample_rate: cpal::SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let buffer = Arc::new(Mutex::new(PlaybackBuffer {
            samples: samples.to_vec(),
            position: 0,
        }));

        let finished = Arc::clone(&self.finished);
        finished.store(false, Ordering::SeqCst);

        let buf_clone = Arc::clone(&buffer);
        let fin_clone = Arc::clone(&finished);

        let stream = device
            .build_output_stream(
                &config,
                move |output: &mut [f32], _info: &cpal::OutputCallbackInfo| {
                    let mut buf = match buf_clone.lock() {
                        Ok(b) => b,
                        Err(_) => {
                            output.fill(0.0);
                            return;
                        }
                    };

                    for sample in output.iter_mut() {
                        if buf.position < buf.samples.len() {
                            *sample = buf.samples[buf.position];
                            buf.position += 1;
                        } else {
                            *sample = 0.0;
                            fin_clone.store(true, Ordering::SeqCst);
                        }
                    }
                },
                move |err| {
                    warn!("output stream error: {err}");
                },
                None,
            )
            .map_err(|e| VoiceError::Playback(format!("failed to build output stream: {e}")))?;

        stream
            .play()
            .map_err(|e| VoiceError::Playback(format!("failed to start output stream: {e}")))?;

        self.stream = Some(stream);
        debug!("playback started");
        Ok(())
    }

    /// Stop any active playback immediately.
    pub fn stop(&mut self) {
        if self.stream.take().is_some() {
            debug!("playback stopped");
        }
        self.finished.store(true, Ordering::SeqCst);
    }

    /// Whether playback has finished (all samples consumed).
    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::SeqCst)
    }

    /// Whether the output stream is currently active.
    pub fn is_playing(&self) -> bool {
        self.stream.is_some() && !self.is_finished()
    }

    /// Block until playback completes or timeout is reached.
    ///
    /// Returns `true` if playback finished, `false` on timeout.
    pub fn wait_until_done(&self, timeout: std::time::Duration) -> bool {
        let start = std::time::Instant::now();
        while !self.is_finished() {
            if start.elapsed() >= timeout {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        true
    }
}

impl Default for AudioPlayback {
    fn default() -> Self {
        Self::new()
    }
}

/// Internal buffer tracking playback progress.
struct PlaybackBuffer {
    samples: Vec<f32>,
    position: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_playback_is_finished() {
        let pb = AudioPlayback::new();
        assert!(pb.is_finished());
        assert!(!pb.is_playing());
    }

    #[test]
    fn play_empty_samples_is_ok() {
        let mut pb = AudioPlayback::new();
        // Playing empty samples should succeed without opening a device.
        let result = pb.play(&[], 16000);
        assert!(result.is_ok());
    }
}
