#![allow(dead_code)]
//! Voice listener — wake word detection and speech-to-text pipeline.
//!
//! Extracted from app.rs. Runs in a background thread, continuously capturing
//! audio, detecting wake words (KWS or fallback), and transcribing speech.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use aios_voice::audio::capture::AudioCapture;
use aios_voice::audio::vad::{VadConfig, VoiceActivityDetector, DEFAULT_FRAME_SIZE};
use tracing::{debug, info, warn};

/// Listener state machine for wake-word detection and speech capture.
enum ListenerState {
    /// Waiting for wake word (KWS or fallback) or passing audio through.
    Idle,
    /// Wake word triggered — capturing command audio until silence or timeout.
    Capture {
        command_buffer: Vec<f32>,
        capture_start: std::time::Instant,
        silence_start: Option<std::time::Instant>,
    },
}

/// Samples to skip after wake word trigger (300 ms at 16 kHz).
const POST_WAKE_DELAY_SAMPLES: usize = 4800;

/// Silence duration that ends a capture (1500 ms).
const CAPTURE_SILENCE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1500);

/// Maximum capture duration before forced transcription (15 s).
const CAPTURE_MAX_DURATION: std::time::Duration = std::time::Duration::from_secs(15);

/// Circular buffer capacity — 2 seconds of audio at 16 kHz.
const RING_BUFFER_CAPACITY: usize = 32000;

/// Start a background voice listener thread.
///
/// Continuously captures audio, detects speech via VAD, runs KWS for wake word
/// detection, and transcribes using whisper-cpp. Transcribed text is sent back
/// to the GTK thread via the provided sender.
pub(crate) fn start_voice_listener(
    stt_tx: std::sync::mpsc::Sender<String>,
    stt_enabled: Arc<AtomicBool>,
    wake_enabled: Arc<AtomicBool>,
    wake_training_in_progress: Arc<AtomicBool>,
    kws_engine: Arc<Mutex<Option<aios_voice::KwsEngine>>>,
    audio_level: Arc<AtomicU32>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        info!("Voice listener thread started");

        let mut capture = AudioCapture::new();
        let mut vad = VoiceActivityDetector::new(VadConfig {
            threshold: 0.015,
            min_speech_frames: 4,
            min_silence_frames: 20,
        });

        let mut state = ListenerState::Idle;
        let mut ring_buf: VecDeque<f32> = VecDeque::with_capacity(RING_BUFFER_CAPACITY);
        let mut speech_buffer: Vec<f32> = Vec::new();
        let mut was_active = false;

        let wake_detector =
            aios_voice::WakeWordDetector::new(aios_voice::WakeWordConfig::default());

        let mut post_wake_skip: usize = 0;

        if let Err(e) = capture.start_recording() {
            warn!("Voice listener: no microphone available: {e}");
            let _ = stt_tx.send(String::new());
            let is_vm = std::path::Path::new("/sys/class/dmi/id/product_name")
                .read_dir()
                .is_ok()
                || std::fs::read_to_string("/sys/class/dmi/id/chassis_type")
                    .map(|s| s.trim() == "1")
                    .unwrap_or(false);
            if is_vm {
                info!("Voice listener: running in VM — microphone not available via SPICE.");
            }
            return;
        }
        info!("Voice listener: recording started, waiting for speech...");

        loop {
            if !stt_enabled.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(200));
                continue;
            }

            if wake_training_in_progress.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(500));
                continue;
            }

            std::thread::sleep(std::time::Duration::from_millis(30));

            let samples = {
                if let Ok(mut buf) = capture.buffer().lock() {
                    std::mem::take(&mut *buf)
                } else {
                    continue;
                }
            };

            if samples.is_empty() {
                audio_level.store(0, Ordering::Relaxed);
                continue;
            }

            // Compute RMS energy level for the VU meter (0-100 scale)
            let rms = {
                let sum: f32 = samples.iter().map(|s| s * s).sum();
                (sum / samples.len() as f32).sqrt()
            };
            let level = ((rms / 0.1) * 100.0).min(100.0) as u32;
            audio_level.store(level, Ordering::Relaxed);

            // Debug: log audio level periodically (every ~3 seconds)
            static FRAME_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let frame = FRAME_COUNT.fetch_add(1, Ordering::Relaxed);
            if frame % 100 == 0 {
                debug!("Audio: {} samples, rms={:.4}, level={}, wake_on={}",
                    samples.len(), rms, level, wake_enabled.load(Ordering::Relaxed));
            }

            let wake_on = wake_enabled.load(Ordering::Relaxed);
            let has_kws_model = kws_engine
                .lock()
                .map(|e| e.as_ref().map(|eng| eng.has_model()).unwrap_or(false))
                .unwrap_or(false);

            match state {
                ListenerState::Idle => {
                    if !wake_on {
                        // Wake disabled: pass all speech directly to Whisper
                        for chunk in samples.chunks(DEFAULT_FRAME_SIZE) {
                            let is_active = vad.process_frame(chunk);

                            if is_active {
                                speech_buffer.extend_from_slice(chunk);
                            } else if was_active && !is_active {
                                let audio_duration = speech_buffer.len() as f32 / 16000.0;
                                if audio_duration > 0.5 && audio_duration < 30.0 {
                                    info!("Voice listener: speech detected ({audio_duration:.1}s), transcribing...");
                                    match transcribe_with_whisper(&speech_buffer) {
                                        Ok(text) if !text.is_empty() => {
                                            info!("Voice listener: transcribed: {}", &text[..text.len().min(50)]);
                                            let _ = stt_tx.send(text);
                                        }
                                        Ok(_) => {
                                            debug!("Voice listener: empty transcription, ignoring");
                                        }
                                        Err(e) => {
                                            warn!("Voice listener: transcription failed: {e}");
                                        }
                                    }
                                }
                                speech_buffer.clear();
                                vad.reset();
                            }
                            was_active = is_active;
                        }

                        if speech_buffer.len() > 16000 * 30 {
                            warn!("Voice listener: speech buffer too large, clearing");
                            speech_buffer.clear();
                            vad.reset();
                        }
                    } else if has_kws_model {
                        // KWS path: feed audio to ONNX engine
                        for &s in &samples {
                            if ring_buf.len() >= RING_BUFFER_CAPACITY {
                                ring_buf.pop_front();
                            }
                            ring_buf.push_back(s);
                        }

                        let result = kws_engine
                            .lock()
                            .map(|mut e| {
                                e.as_mut()
                                    .map(|eng| eng.process_audio(&samples))
                                    .unwrap_or(aios_voice::KwsResult {
                                        confidence: 0.0,
                                        triggered: false,
                                    })
                            })
                            .unwrap_or(aios_voice::KwsResult {
                                confidence: 0.0,
                                triggered: false,
                            });

                        if result.triggered {
                            info!(
                                "Voice listener: KWS triggered (confidence: {:.2})",
                                result.confidence
                            );

                            let buf_vec: Vec<f32> = ring_buf.iter().copied().collect();
                            let keep = buf_vec.len().saturating_sub(POST_WAKE_DELAY_SAMPLES);
                            let seed: Vec<f32> = if keep > 0 {
                                buf_vec[..keep].to_vec()
                            } else {
                                Vec::new()
                            };

                            state = ListenerState::Capture {
                                command_buffer: seed,
                                capture_start: std::time::Instant::now(),
                                silence_start: None,
                            };
                            ring_buf.clear();
                            post_wake_skip = POST_WAKE_DELAY_SAMPLES;

                            if let Ok(mut e) = kws_engine.lock() {
                                if let Some(ref mut eng) = *e {
                                    eng.reset();
                                }
                            }
                            vad.reset();
                            was_active = false;
                        }
                    } else {
                        // Fallback path: Whisper + keyword match
                        for &s in &samples {
                            if ring_buf.len() >= RING_BUFFER_CAPACITY {
                                ring_buf.pop_front();
                            }
                            ring_buf.push_back(s);
                        }

                        for chunk in samples.chunks(DEFAULT_FRAME_SIZE) {
                            let is_active = vad.process_frame(chunk);

                            if is_active {
                                speech_buffer.extend_from_slice(chunk);
                            } else if was_active && !is_active {
                                let audio_duration = speech_buffer.len() as f32 / 16000.0;
                                if audio_duration > 0.3 && audio_duration < 10.0 {
                                    match transcribe_with_whisper(&speech_buffer) {
                                        Ok(text) if !text.is_empty() => {
                                            if wake_detector.matches_wake_word(&text) {
                                                info!("Voice listener: wake word detected via fallback: \"{}\"", &text[..text.len().min(50)]);
                                                state = ListenerState::Capture {
                                                    command_buffer: Vec::new(),
                                                    capture_start: std::time::Instant::now(),
                                                    silence_start: None,
                                                };
                                                ring_buf.clear();
                                                speech_buffer.clear();
                                                vad.reset();
                                                was_active = false;
                                                break;
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                speech_buffer.clear();
                                vad.reset();
                            }
                            was_active = is_active;
                        }

                        if speech_buffer.len() > 16000 * 30 {
                            speech_buffer.clear();
                            vad.reset();
                        }
                    }
                }

                ListenerState::Capture {
                    ref mut command_buffer,
                    capture_start,
                    ref mut silence_start,
                } => {
                    let effective_samples = if post_wake_skip > 0 {
                        let skip = post_wake_skip.min(samples.len());
                        post_wake_skip -= skip;
                        &samples[skip..]
                    } else {
                        &samples[..]
                    };

                    for chunk in effective_samples.chunks(DEFAULT_FRAME_SIZE) {
                        let is_active = vad.process_frame(chunk);

                        if is_active {
                            command_buffer.extend_from_slice(chunk);
                            *silence_start = None;
                        } else {
                            command_buffer.extend_from_slice(chunk);
                            if silence_start.is_none() {
                                *silence_start = Some(std::time::Instant::now());
                            }
                        }
                    }

                    let elapsed = capture_start.elapsed();
                    let silence_exceeded = silence_start
                        .map(|s| s.elapsed() >= CAPTURE_SILENCE_TIMEOUT)
                        .unwrap_or(false);
                    let timeout_exceeded = elapsed >= CAPTURE_MAX_DURATION;

                    if silence_exceeded || timeout_exceeded {
                        let reason = if timeout_exceeded {
                            "timeout"
                        } else {
                            "silence"
                        };
                        let audio_duration = command_buffer.len() as f32 / 16000.0;
                        info!("Voice listener: capture ended ({reason}), {audio_duration:.1}s of audio");

                        if audio_duration > 0.3 && !command_buffer.is_empty() {
                            match transcribe_with_whisper(command_buffer) {
                                Ok(text) if !text.is_empty() => {
                                    info!("Voice listener: command transcribed: {}", &text[..text.len().min(50)]);
                                    let _ = stt_tx.send(text);
                                }
                                Ok(_) => {
                                    debug!(
                                        "Voice listener: empty command transcription, ignoring"
                                    );
                                }
                                Err(e) => {
                                    warn!("Voice listener: command transcription failed: {e}");
                                }
                            }
                        }

                        state = ListenerState::Idle;
                        speech_buffer.clear();
                        vad.reset();
                        was_active = false;
                        post_wake_skip = 0;
                    }

                    if let ListenerState::Capture {
                        ref command_buffer, ..
                    } = state
                    {
                        if command_buffer.len() > 16000 * 30 {
                            warn!(
                                "Voice listener: command buffer too large, forcing transcription"
                            );
                        }
                    }
                }
            }
        }
    })
}

/// Transcribe audio samples using the `WhisperStt` engine from `aios-voice`.
///
/// Uses the proper `SttEngine` trait instead of shelling out directly.
/// Model path resolution handles both system (ISO) and user home paths.
pub(crate) fn transcribe_with_whisper(samples: &[f32]) -> Result<String, String> {
    use aios_voice::stt::{SttBackend, create_stt_engine};

    let mut engine = create_stt_engine(SttBackend::Whisper);

    // Try to load a model in preference order: configured size, then fallbacks.
    // The engine's transcribe() has lazy-loading built in, but we attempt an
    // explicit load for better error reporting.
    let model_sizes = ["medium", "base", "small", "tiny", "large"];
    let mut loaded = false;
    for size in &model_sizes {
        if engine.load_model(size).is_ok() {
            loaded = true;
            break;
        }
    }
    if !loaded {
        tracing::warn!("No whisper model found — transcribe will attempt lazy fallback");
    }

    let result = engine
        .transcribe(samples, None)
        .map_err(|e| format!("Whisper transcription failed: {e}"))?;

    Ok(result.text)
}

/// Strip the wake phrase from the beginning of transcribed text.
pub(crate) fn strip_wake_phrase(text: &str, wake_word: &str) -> String {
    let lower = text.to_lowercase();
    let wake_lower = wake_word.to_lowercase();
    if lower.starts_with(&wake_lower) {
        text[wake_lower.len()..].trim().to_string()
    } else {
        text.to_string()
    }
}
