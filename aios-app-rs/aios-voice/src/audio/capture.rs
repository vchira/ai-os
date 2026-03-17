//! Audio capture using [`cpal`].
//!
//! Opens the default input device and records mono f32 audio at 16 kHz into
//! an in-memory buffer.  The interface is intentionally synchronous —
//! [`AudioCapture::start_recording`] spawns a cpal input stream on its own
//! thread and [`AudioCapture::stop_recording`] joins it, returning the
//! collected samples.

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use tracing::{debug, warn};

use crate::error::VoiceError;

/// Desired sample rate for STT input.
pub const CAPTURE_SAMPLE_RATE: u32 = 16_000;

/// Number of channels (mono).
pub const CAPTURE_CHANNELS: u16 = 1;

/// Shared recording buffer.
type Buffer = Arc<Mutex<Vec<f32>>>;

/// Audio capture handle.
///
/// Records from the system's default input device.  Thread-safe — the inner
/// buffer and stream state are protected by a mutex.
pub struct AudioCapture {
    /// Accumulated samples while recording.
    buffer: Buffer,
    /// Active cpal stream, present only while recording.
    stream: Option<cpal::Stream>,
    /// Whether we are currently recording.
    recording: bool,
}

impl AudioCapture {
    /// Create a new capture handle.  Does **not** open any device yet.
    pub fn new() -> Self {
        Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
            stream: None,
            recording: false,
        }
    }

    /// Begin recording from the default input device.
    ///
    /// Returns an error if no input device is available or the stream cannot
    /// be created.  Calling this while already recording is a no-op.
    pub fn start_recording(&mut self) -> Result<(), VoiceError> {
        if self.recording {
            debug!("start_recording called while already recording — ignoring");
            return Ok(());
        }

        // Clear previous samples.
        if let Ok(mut buf) = self.buffer.lock() {
            buf.clear();
        }

        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| VoiceError::AudioDevice("no default input device".into()))?;

        debug!(device = ?device.name(), "opening input device");

        let config = cpal::StreamConfig {
            channels: CAPTURE_CHANNELS,
            sample_rate: cpal::SampleRate(CAPTURE_SAMPLE_RATE),
            buffer_size: cpal::BufferSize::Default,
        };

        let buffer = Arc::clone(&self.buffer);

        let stream = device
            .build_input_stream(
                &config,
                move |data: &[f32], _info: &cpal::InputCallbackInfo| {
                    if let Ok(mut buf) = buffer.lock() {
                        buf.extend_from_slice(data);
                    }
                },
                move |err| {
                    warn!("input stream error: {err}");
                },
                None,
            )
            .map_err(|e| VoiceError::Capture(format!("failed to build input stream: {e}")))?;

        stream
            .play()
            .map_err(|e| VoiceError::Capture(format!("failed to start input stream: {e}")))?;

        self.stream = Some(stream);
        self.recording = true;
        debug!("recording started");
        Ok(())
    }

    /// Stop recording and return the captured samples.
    ///
    /// Returns an empty vec if recording was not active.
    pub fn stop_recording(&mut self) -> Vec<f32> {
        if !self.recording {
            debug!("stop_recording called while not recording — returning empty");
            return Vec::new();
        }

        // Dropping the stream stops cpal's callback.
        self.stream.take();
        self.recording = false;

        let samples = if let Ok(mut buf) = self.buffer.lock() {
            std::mem::take(&mut *buf)
        } else {
            Vec::new()
        };

        debug!(samples = samples.len(), "recording stopped");
        samples
    }

    /// Whether the capture is currently recording.
    pub fn is_recording(&self) -> bool {
        self.recording
    }

    /// Access the shared recording buffer for real-time processing.
    ///
    /// Callers can lock the buffer, drain samples, and process them
    /// (e.g. for VAD) without stopping the recording.
    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    /// Duration of audio currently in the buffer, in seconds.
    pub fn buffered_duration(&self) -> f32 {
        if let Ok(buf) = self.buffer.lock() {
            buf.len() as f32 / CAPTURE_SAMPLE_RATE as f32
        } else {
            0.0
        }
    }
}

impl Default for AudioCapture {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_capture_not_recording() {
        let cap = AudioCapture::new();
        assert!(!cap.is_recording());
        assert_eq!(cap.buffered_duration(), 0.0);
    }

    #[test]
    fn stop_without_start_returns_empty() {
        let mut cap = AudioCapture::new();
        let samples = cap.stop_recording();
        assert!(samples.is_empty());
    }
}
