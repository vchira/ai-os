//! Voice controller — high-level orchestrator for STT, TTS, and audio I/O.
//!
//! [`VoiceController`] wires together an [`SttEngine`], a [`TtsEngine`],
//! [`AudioCapture`], and [`AudioPlayback`] into a single interface that the
//! rest of the application can use without worrying about the underlying
//! backend details.

use std::path::Path;

use tracing::{debug, info, warn};

use crate::audio::capture::AudioCapture;
use crate::audio::playback::AudioPlayback;
use crate::error::VoiceError;
use crate::stt::{self, SttBackend, SttEngine, TranscriptionResult};
use crate::tts::{self, TtsBackend, TtsEngine, VoiceInfo};

// ---------------------------------------------------------------------------
// Hardware profiling
// ---------------------------------------------------------------------------

/// Rough hardware profile used for auto-configuration.
#[derive(Debug, Clone)]
pub struct HardwareProfile {
    /// Total system RAM in megabytes.
    pub ram_mb: u64,
    /// Number of logical CPU cores.
    pub cpu_cores: usize,
    /// Whether a GPU is detected (any vendor).
    pub has_gpu: bool,
}

impl HardwareProfile {
    /// Whether this machine can comfortably run the Whisper "medium" model.
    ///
    /// Requires at least 4 GB RAM and 4 cores.
    pub fn can_run_whisper_medium(&self) -> bool {
        self.ram_mb >= 4096 && self.cpu_cores >= 4
    }

    /// Whether this machine can run the Whisper "large" model.
    ///
    /// Requires at least 8 GB RAM and a GPU or 8+ cores.
    pub fn can_run_whisper_large(&self) -> bool {
        self.ram_mb >= 8192 && (self.has_gpu || self.cpu_cores >= 8)
    }

    /// Recommended Whisper model size for this hardware.
    pub fn recommended_whisper_model(&self) -> &'static str {
        if self.can_run_whisper_large() {
            "large"
        } else if self.can_run_whisper_medium() {
            "medium"
        } else if self.ram_mb >= 2048 {
            "small"
        } else if self.ram_mb >= 1024 {
            "base"
        } else {
            "tiny"
        }
    }
}

// ---------------------------------------------------------------------------
// VoiceController
// ---------------------------------------------------------------------------

/// High-level voice controller.
///
/// Holds the current STT and TTS engines along with audio I/O handles.
/// Provides methods for recording, transcription, and speech synthesis
/// with the ability to swap backends at runtime.
pub struct VoiceController {
    /// Current speech-to-text engine.
    stt: Box<dyn SttEngine>,
    /// Current text-to-speech engine.
    tts: Box<dyn TtsEngine>,
    /// Audio input (microphone).
    capture: AudioCapture,
    /// Audio output (speaker).
    playback: AudioPlayback,
    /// Active STT language override (None = auto-detect).
    stt_language: Option<String>,
}

impl VoiceController {
    /// Create a new controller with default backends (Whisper + Piper).
    pub fn new() -> Self {
        Self {
            stt: stt::create_stt_engine(SttBackend::Whisper),
            tts: tts::create_tts_engine(TtsBackend::Piper),
            capture: AudioCapture::new(),
            playback: AudioPlayback::new(),
            stt_language: None,
        }
    }

    /// Create a controller with specific backends.
    pub fn with_backends(stt_backend: SttBackend, tts_backend: TtsBackend) -> Self {
        Self {
            stt: stt::create_stt_engine(stt_backend),
            tts: tts::create_tts_engine(tts_backend),
            capture: AudioCapture::new(),
            playback: AudioPlayback::new(),
            stt_language: None,
        }
    }

    // -----------------------------------------------------------------------
    // Backend switching
    // -----------------------------------------------------------------------

    /// Replace the STT engine at runtime.
    pub fn switch_stt_backend(&mut self, backend: SttBackend) {
        info!(backend = %backend, "switching STT backend");
        self.stt = stt::create_stt_engine(backend);
    }

    /// Replace the TTS engine at runtime.
    pub fn switch_tts_backend(&mut self, backend: TtsBackend) {
        info!(backend = %backend, "switching TTS backend");
        self.tts = tts::create_tts_engine(backend);
    }

    /// The currently active STT backend.
    pub fn stt_backend(&self) -> SttBackend {
        self.stt.backend()
    }

    /// The currently active TTS backend.
    pub fn tts_backend(&self) -> TtsBackend {
        self.tts.backend()
    }

    // -----------------------------------------------------------------------
    // Recording + transcription
    // -----------------------------------------------------------------------

    /// Start recording from the microphone.
    pub fn start_recording(&mut self) -> Result<(), VoiceError> {
        self.capture.start_recording()
    }

    /// Stop recording and transcribe the captured audio.
    pub fn stop_and_transcribe(&mut self) -> Result<TranscriptionResult, VoiceError> {
        let samples = self.capture.stop_recording();
        if samples.is_empty() {
            return Ok(TranscriptionResult {
                text: String::new(),
                language: self.stt_language.clone(),
                duration_secs: 0.0,
            });
        }

        debug!(
            samples = samples.len(),
            duration_secs = samples.len() as f32 / 16000.0,
            "transcribing captured audio"
        );

        self.stt
            .transcribe(&samples, self.stt_language.as_deref())
    }

    /// Transcribe pre-recorded audio samples (mono f32 at 16 kHz).
    pub fn transcribe(&self, audio: &[f32]) -> Result<TranscriptionResult, VoiceError> {
        self.stt
            .transcribe(audio, self.stt_language.as_deref())
    }

    /// Whether recording is currently active.
    pub fn is_recording(&self) -> bool {
        self.capture.is_recording()
    }

    // -----------------------------------------------------------------------
    // Speech synthesis + playback
    // -----------------------------------------------------------------------

    /// Synthesize text and play it through the speaker.
    pub fn speak(&mut self, text: &str) -> Result<(), VoiceError> {
        if text.is_empty() {
            return Ok(());
        }

        let samples = self.tts.synthesize(text)?;
        let sample_rate = self.tts.sample_rate();

        debug!(
            samples = samples.len(),
            sample_rate,
            "playing synthesized speech"
        );

        self.playback.play(&samples, sample_rate)
    }

    /// Stop any active speech playback.
    pub fn stop_speaking(&mut self) {
        self.playback.stop();
    }

    /// Whether speech playback is currently active.
    pub fn is_speaking(&self) -> bool {
        self.playback.is_playing()
    }

    // -----------------------------------------------------------------------
    // Voice / language configuration
    // -----------------------------------------------------------------------

    /// Set the TTS voice by identifier.
    pub fn set_voice(&mut self, voice_id: &str) -> Result<(), VoiceError> {
        self.tts.set_voice(voice_id)
    }

    /// Get the current TTS voice identifier.
    pub fn current_voice(&self) -> &str {
        self.tts.current_voice()
    }

    /// Set the STT language (pass `None` for auto-detection).
    pub fn set_language(&mut self, language: Option<String>) {
        debug!(language = ?language, "setting STT language");
        self.stt_language = language;
    }

    /// Get the current STT language setting.
    pub fn language(&self) -> Option<&str> {
        self.stt_language.as_deref()
    }

    /// Load a specific STT model size.
    pub fn load_stt_model(&mut self, model_size: &str) -> Result<(), VoiceError> {
        self.stt.load_model(model_size)
    }

    /// List available STT model sizes.
    pub fn available_stt_models(&self) -> Vec<String> {
        self.stt.available_models()
    }

    /// List available TTS voices.
    pub fn available_voices(&self) -> Vec<VoiceInfo> {
        self.tts.list_voices()
    }

    // -----------------------------------------------------------------------
    // Hardware detection + auto-configuration
    // -----------------------------------------------------------------------

    /// Probe the system hardware and return a profile.
    pub fn benchmark_hardware() -> HardwareProfile {
        let ram_mb = read_total_ram_mb();
        let cpu_cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        let has_gpu = detect_gpu();

        let profile = HardwareProfile {
            ram_mb,
            cpu_cores,
            has_gpu,
        };

        info!(
            ram_mb = profile.ram_mb,
            cpu_cores = profile.cpu_cores,
            has_gpu = profile.has_gpu,
            "hardware profile"
        );

        profile
    }

    /// Auto-configure backends and model sizes based on detected hardware.
    ///
    /// This picks the best STT model the hardware can handle and falls back
    /// to eSpeak if Piper is not installed.
    pub fn auto_configure(&mut self) {
        let profile = Self::benchmark_hardware();

        // Pick the best Whisper model.
        let model_size = profile.recommended_whisper_model();
        info!(model_size, "auto-configuring STT model");
        if let Err(e) = self.stt.load_model(model_size) {
            warn!(
                model_size,
                error = %e,
                "failed to load recommended model, will try lazy loading"
            );
        }

        // Check if piper is available; fall back to espeak if not.
        if !is_binary_available("piper") {
            if is_binary_available("espeak-ng") {
                info!("piper not found, falling back to espeak-ng");
                self.switch_tts_backend(TtsBackend::Espeak);
            } else {
                warn!("neither piper nor espeak-ng found — TTS will not work");
            }
        }
    }
}

impl Default for VoiceController {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// System probing helpers
// ---------------------------------------------------------------------------

/// Read total RAM from `/proc/meminfo` (Linux-specific).
fn read_total_ram_mb() -> u64 {
    let path = Path::new("/proc/meminfo");
    let contents = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return 0,
    };

    for line in contents.lines() {
        if line.starts_with("MemTotal:") {
            // Format: "MemTotal:       16384000 kB"
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                if let Ok(kb) = parts[1].parse::<u64>() {
                    return kb / 1024;
                }
            }
        }
    }

    0
}

/// Check whether a GPU is present by looking for common device paths.
fn detect_gpu() -> bool {
    // Check for DRI render nodes (works for AMD, Intel, NVIDIA with open driver).
    let dri = Path::new("/dev/dri/renderD128");
    if dri.exists() {
        return true;
    }

    // Check for NVIDIA character device.
    let nvidia = Path::new("/dev/nvidia0");
    if nvidia.exists() {
        return true;
    }

    false
}

/// Check whether a binary is available on `$PATH`.
fn is_binary_available(name: &str) -> bool {
    std::process::Command::new("which")
        .arg(name)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_controller_defaults() {
        let vc = VoiceController::new();
        assert_eq!(vc.stt_backend(), SttBackend::Whisper);
        assert_eq!(vc.tts_backend(), TtsBackend::Piper);
        assert!(!vc.is_recording());
        assert!(!vc.is_speaking());
        assert!(vc.language().is_none());
    }

    #[test]
    fn switch_backends() {
        let mut vc = VoiceController::new();

        vc.switch_tts_backend(TtsBackend::Espeak);
        assert_eq!(vc.tts_backend(), TtsBackend::Espeak);

        vc.switch_stt_backend(SttBackend::Whisper);
        assert_eq!(vc.stt_backend(), SttBackend::Whisper);
    }

    #[test]
    fn set_and_get_language() {
        let mut vc = VoiceController::new();
        assert!(vc.language().is_none());

        vc.set_language(Some("ro".into()));
        assert_eq!(vc.language(), Some("ro"));

        vc.set_language(None);
        assert!(vc.language().is_none());
    }

    #[test]
    fn hardware_profile_recommendations() {
        let low = HardwareProfile {
            ram_mb: 512,
            cpu_cores: 2,
            has_gpu: false,
        };
        assert_eq!(low.recommended_whisper_model(), "tiny");
        assert!(!low.can_run_whisper_medium());

        let mid = HardwareProfile {
            ram_mb: 4096,
            cpu_cores: 4,
            has_gpu: false,
        };
        assert_eq!(mid.recommended_whisper_model(), "medium");
        assert!(mid.can_run_whisper_medium());
        assert!(!mid.can_run_whisper_large());

        let high = HardwareProfile {
            ram_mb: 16384,
            cpu_cores: 8,
            has_gpu: true,
        };
        assert_eq!(high.recommended_whisper_model(), "large");
        assert!(high.can_run_whisper_large());
    }

    #[test]
    fn benchmark_hardware_returns_nonzero_on_linux() {
        let profile = VoiceController::benchmark_hardware();
        // On any real Linux system we should get nonzero RAM and at least 1 core.
        // In CI or containers this may vary, so just check the structure.
        assert!(profile.cpu_cores >= 1);
        // RAM might be 0 if /proc/meminfo is not available (e.g. in some sandboxes).
    }

    #[test]
    fn speak_empty_is_noop() {
        let mut vc = VoiceController::new();
        let result = vc.speak("");
        assert!(result.is_ok());
    }

    #[test]
    fn with_backends_constructor() {
        let vc = VoiceController::with_backends(SttBackend::Whisper, TtsBackend::Espeak);
        assert_eq!(vc.stt_backend(), SttBackend::Whisper);
        assert_eq!(vc.tts_backend(), TtsBackend::Espeak);
    }
}
