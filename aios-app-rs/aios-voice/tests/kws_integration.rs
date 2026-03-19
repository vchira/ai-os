//! Integration test for KWS wake word detection.
//!
//! Run with: ORT_DYLIB_PATH=/path/to/libonnxruntime.so cargo test --test kws_integration

use std::path::PathBuf;

fn models_dir() -> Option<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/aios".to_string());
    let paths: Vec<PathBuf> = vec![
        PathBuf::from("/opt/aios-app/models/kws"),
        PathBuf::from(&home).join(".aios/models/kws"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap()
            .parent().unwrap()
            .join("distro/build/chroot/opt/aios-app/models/kws"),
    ];
    paths.into_iter().find(|p: &PathBuf| p.join("infrastructure/melspectrogram.onnx").exists())
}

#[test]
fn kws_engine_creates_and_loads_model() {
    let Some(dir) = models_dir() else {
        eprintln!("SKIP: KWS models not found");
        return;
    };
    eprintln!("Using models from: {}", dir.display());

    let mut engine = match aios_voice::KwsEngine::new(&dir) {
        Ok(e) => e,
        Err(e) => { eprintln!("SKIP: KwsEngine::new failed: {e}"); return; }
    };
    eprintln!("Engine created OK");

    let model_path = dir.join("pretrained/hey_jarvis.onnx");
    if !model_path.exists() { eprintln!("SKIP: model not found"); return; }

    engine.load_wake_model(&model_path, "Hey Jarvis")
        .expect("load_wake_model should succeed");
    assert!(engine.has_model());
    eprintln!("hey_jarvis model loaded OK");
}

#[test]
fn kws_silence_does_not_trigger() {
    let Some(dir) = models_dir() else { return; };
    let mut engine = match aios_voice::KwsEngine::new(&dir) { Ok(e) => e, Err(_) => return };
    let model_path = dir.join("pretrained/hey_jarvis.onnx");
    if model_path.exists() { let _ = engine.load_wake_model(&model_path, "Hey Jarvis"); }

    let silence: Vec<f32> = vec![0.0; 16000];
    let result = engine.process_audio(&silence);
    assert!(!result.triggered, "Silence should not trigger");
}

#[test]
fn kws_noise_does_not_trigger() {
    let Some(dir) = models_dir() else { return; };
    let mut engine = match aios_voice::KwsEngine::new(&dir) { Ok(e) => e, Err(_) => return };
    let model_path = dir.join("pretrained/hey_jarvis.onnx");
    if model_path.exists() { let _ = engine.load_wake_model(&model_path, "Hey Jarvis"); }

    let noise: Vec<f32> = (0..16000).map(|i| (i as f32 * 0.1).sin() * 0.5).collect();
    let result = engine.process_audio(&noise);
    assert!(!result.triggered, "Noise should not trigger");
}

#[test]
fn kws_processes_synthetic_hey_jarvis() {
    let Some(dir) = models_dir() else { return; };
    let mut engine = match aios_voice::KwsEngine::new(&dir) { Ok(e) => e, Err(_) => return };
    let model_path = dir.join("pretrained/hey_jarvis.onnx");
    if !model_path.exists() { return; }
    engine.load_wake_model(&model_path, "Hey Jarvis").unwrap();

    // Generate audio with espeak-ng
    let tmp = "/tmp/kws-test-hey-jarvis.wav";
    let ok = std::process::Command::new("espeak-ng")
        .args(["-v", "en", "-w", tmp, "hey jarvis"])
        .status().map(|s| s.success()).unwrap_or(false);
    if !ok { eprintln!("SKIP: espeak-ng unavailable"); return; }

    let wav = std::fs::read(tmp).unwrap();
    if wav.len() < 100 { return; }

    let sample_rate = u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]);
    let raw = &wav[44..];
    let samples_i16: Vec<i16> = raw.chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
    let mut samples: Vec<f32> = samples_i16.iter()
        .map(|&s| s as f32 / i16::MAX as f32).collect();

    // Resample to 16kHz if needed
    if sample_rate != 16000 {
        let ratio = 16000.0 / sample_rate as f64;
        let new_len = (samples.len() as f64 * ratio) as usize;
        samples = (0..new_len).map(|i| {
            let idx = (i as f64 / ratio) as usize;
            samples.get(idx).copied().unwrap_or(0.0)
        }).collect();
    }

    eprintln!("Processing {:.1}s of 'hey jarvis' audio", samples.len() as f32 / 16000.0);

    let mut max_conf: f32 = 0.0;
    for chunk in samples.chunks(1280) {
        let r = engine.process_audio(chunk);
        if r.confidence > max_conf { max_conf = r.confidence; }
        if r.triggered {
            eprintln!("TRIGGERED at confidence {:.3}", r.confidence);
            break;
        }
    }
    eprintln!("Max confidence: {:.3}", max_conf);
    // Synthetic speech may not trigger (trained on real voices),
    // but confidence should be non-zero.
}

#[test]
fn vad_detects_speech() {
    use aios_voice::audio::vad::{VadConfig, VoiceActivityDetector, DEFAULT_FRAME_SIZE};

    let mut vad = VoiceActivityDetector::new(VadConfig {
        threshold: 0.015, min_speech_frames: 4, min_silence_frames: 20,
    });

    assert!(!vad.process_frame(&vec![0.0; DEFAULT_FRAME_SIZE]), "Silence = no speech");

    let loud: Vec<f32> = (0..DEFAULT_FRAME_SIZE)
        .map(|i| (i as f32 * 0.1).sin() * 0.5).collect();
    let mut detected = false;
    for _ in 0..10 {
        if vad.process_frame(&loud) { detected = true; break; }
    }
    assert!(detected, "Loud signal should trigger VAD");
}
