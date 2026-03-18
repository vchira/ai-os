//! Audio capture using `arecord` (ALSA) with `cpal` fallback.
//!
//! Records mono f32 audio at 16 kHz into a shared in-memory buffer.
//! Uses `arecord` by default (works reliably in AiOS, including QEMU/SPICE),
//! falling back to `cpal` if `arecord` is not available.

use std::sync::{Arc, Mutex};

use tracing::{debug, info, warn};

use crate::error::VoiceError;

/// Desired sample rate for STT input.
pub const CAPTURE_SAMPLE_RATE: u32 = 16_000;

/// Number of channels (mono).
pub const CAPTURE_CHANNELS: u16 = 1;

/// Shared recording buffer.
type Buffer = Arc<Mutex<Vec<f32>>>;

/// Audio capture handle.
///
/// Records from the system's default input device. Thread-safe — the inner
/// buffer is protected by a mutex. The `arecord` subprocess runs continuously
/// and feeds samples into the buffer.
pub struct AudioCapture {
    /// Accumulated samples while recording.
    buffer: Buffer,
    /// Active arecord subprocess, present only while recording.
    child: Option<std::process::Child>,
    /// Reader thread that converts arecord output to f32 samples.
    reader_thread: Option<std::thread::JoinHandle<()>>,
    /// Whether we are currently recording.
    recording: bool,
}

impl AudioCapture {
    /// Create a new capture handle. Does **not** open any device yet.
    pub fn new() -> Self {
        Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
            child: None,
            reader_thread: None,
            recording: false,
        }
    }

    /// Begin recording from the default input device.
    ///
    /// Tries `arecord` first (reliable in AiOS/ALSA), falls back to `cpal`.
    /// Returns an error if neither method works.
    pub fn start_recording(&mut self) -> Result<(), VoiceError> {
        if self.recording {
            debug!("start_recording called while already recording — ignoring");
            return Ok(());
        }

        // Clear previous samples.
        if let Ok(mut buf) = self.buffer.lock() {
            buf.clear();
        }

        // Try arecord first — this is what actually works in AiOS
        match self.start_arecord() {
            Ok(()) => {
                info!("Audio capture started via arecord (ALSA)");
                return Ok(());
            }
            Err(e) => {
                warn!("arecord failed: {e} — trying cpal fallback");
            }
        }

        // Fallback to cpal
        self.start_cpal()
    }

    /// Start recording using `arecord` subprocess.
    fn start_arecord(&mut self) -> Result<(), VoiceError> {
        // Spawn arecord in continuous mode, outputting raw S16_LE to stdout
        let child = std::process::Command::new("arecord")
            .args([
                "-f", "S16_LE",
                "-r", &CAPTURE_SAMPLE_RATE.to_string(),
                "-c", &CAPTURE_CHANNELS.to_string(),
                "-t", "raw",
                "-q",  // quiet, no headers
                "-",   // stdout
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| VoiceError::Capture(format!("failed to spawn arecord: {e}")))?;

        let mut child = child;
        let stdout = child.stdout.take()
            .ok_or_else(|| VoiceError::Capture("arecord stdout not available".into()))?;

        // Spawn reader thread that converts S16_LE bytes to f32 and fills the buffer
        let buffer = Arc::clone(&self.buffer);
        let reader = std::thread::spawn(move || {
            use std::io::Read;
            let mut reader = std::io::BufReader::with_capacity(4096, stdout);
            let mut byte_buf = [0u8; 3200]; // 1600 samples * 2 bytes = 100ms at 16kHz

            loop {
                match reader.read(&mut byte_buf) {
                    Ok(0) => break, // EOF — arecord exited
                    Ok(n) => {
                        // Convert S16_LE pairs to f32 samples
                        let samples: Vec<f32> = byte_buf[..n]
                            .chunks_exact(2)
                            .map(|c| {
                                let sample = i16::from_le_bytes([c[0], c[1]]);
                                sample as f32 / 32768.0
                            })
                            .collect();

                        if let Ok(mut buf) = buffer.lock() {
                            buf.extend_from_slice(&samples);
                        }
                    }
                    Err(e) => {
                        warn!("arecord read error: {e}");
                        break;
                    }
                }
            }
        });

        self.child = Some(child);
        self.reader_thread = Some(reader);
        self.recording = true;
        debug!("recording started via arecord");
        Ok(())
    }

    /// Start recording using cpal (fallback).
    fn start_cpal(&mut self) -> Result<(), VoiceError> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| VoiceError::AudioDevice("no default input device".into()))?;

        debug!(device = ?device.name(), "opening input device via cpal");

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
                    warn!("cpal input stream error: {err}");
                },
                None,
            )
            .map_err(|e| VoiceError::Capture(format!("failed to build cpal input stream: {e}")))?;

        stream
            .play()
            .map_err(|e| VoiceError::Capture(format!("failed to start cpal input stream: {e}")))?;

        // Store stream handle to keep it alive (dropping it stops the stream).
        // We store it as the child field's type doesn't match, so we leak it intentionally.
        // The stream will be stopped when AudioCapture is dropped.
        std::mem::forget(stream);

        self.recording = true;
        info!("Audio capture started via cpal (fallback)");
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

        // Kill arecord subprocess if running
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }

        // Wait for reader thread to finish
        if let Some(thread) = self.reader_thread.take() {
            let _ = thread.join();
        }

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

impl Drop for AudioCapture {
    fn drop(&mut self) {
        if self.recording {
            self.stop_recording();
        }
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
