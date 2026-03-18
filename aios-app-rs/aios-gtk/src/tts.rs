//! TTS (Text-to-Speech) functions for AiOS.
//!
//! Extracted from app.rs. Handles speaking AI responses via Piper or espeak-ng,
//! with optional LLM-based summarization for long responses.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use aios_core::config::ConfigManager;

/// Kill any running TTS processes (piper, espeak-ng, aplay).
pub(crate) fn stop_tts() {
    std::thread::spawn(|| {
        let _ = std::process::Command::new("pkill")
            .args(["-f", "piper"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        let _ = std::process::Command::new("pkill")
            .args(["-f", "espeak-ng"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        let _ = std::process::Command::new("pkill")
            .args(["-f", "aplay.*raw"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    });
}

/// Strip markup and code blocks from text, returning clean plain text.
pub(crate) fn strip_for_tts(raw: &str) -> (String, u32) {
    let mut text = String::new();
    let mut in_code = false;
    let mut code_block_count = 0u32;
    for line in raw.lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
            if !in_code {
                code_block_count += 1;
            }
            continue;
        }
        if !in_code {
            text.push_str(line);
            text.push('\n');
        }
    }
    let plain = aios_core::types::to_plain(&text);
    (plain.trim().to_string(), code_block_count)
}

/// For short responses, return plain text directly.
/// For long responses, return None — caller should use LLM summary.
pub(crate) fn prepare_tts_text_short(raw: &str) -> Option<String> {
    let (plain, code_blocks) = strip_for_tts(raw);
    if plain.is_empty() {
        return Some(String::new());
    }

    let suffix = if code_blocks > 0 {
        format!(
            " I also included {} code {}.",
            code_blocks,
            if code_blocks == 1 { "block" } else { "blocks" }
        )
    } else {
        String::new()
    };

    // Short enough to read directly
    if plain.len() <= 300 {
        return Some(format!("{plain}{suffix}"));
    }

    // Long — needs LLM summary
    None
}

/// Summarize a long response using Claude Haiku (fast, cheap).
/// Falls back to sentence truncation if API call fails.
pub(crate) fn summarize_for_tts(raw: &str, api_key: &str) -> String {
    let (plain, code_blocks) = strip_for_tts(raw);

    let code_mention = if code_blocks > 0 {
        format!(
            " I also included {code_blocks} code {}.",
            if code_blocks == 1 { "block" } else { "blocks" }
        )
    } else {
        String::new()
    };

    // Try Haiku summarization
    if !api_key.is_empty() {
        let body = serde_json::json!({
            "model": "claude-haiku-4-5-20251001",
            "max_tokens": 100,
            "messages": [{
                "role": "user",
                "content": format!(
                    "Summarize the following AI assistant response in exactly ONE short spoken sentence (max 30 words). \
                     No markdown, no special characters, no asterisks, no hashtags — just plain spoken English. \
                     End with: The detailed answer is in the chat.\n\n---\n{}",
                    &plain[..plain.len().min(2000)]
                )
            }]
        });

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build();

        if let Ok(client) = client {
            let resp = client
                .post("https://api.anthropic.com/v1/messages")
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .body(body.to_string())
                .send();

            if let Ok(resp) = resp {
                if let Ok(json) = resp.json::<serde_json::Value>() {
                    if let Some(text) = json["content"][0]["text"].as_str() {
                        let summary = text.trim().to_string();
                        if !summary.is_empty() {
                            tracing::debug!("TTS summary from Haiku: {summary}");
                            return format!("{summary}{code_mention}");
                        }
                    }
                }
            }
        }
    }

    // Fallback: truncate at sentence boundary
    let boundary = plain[..plain.len().min(300)]
        .rfind(|c: char| c == '.' || c == '!' || c == '?')
        .map(|i| i + 1)
        .unwrap_or(plain.len().min(300));
    format!(
        "{} The detailed answer is in the chat.{code_mention}",
        &plain[..boundary].trim(),
    )
}

/// Speak text using TTS if enabled.
///
/// Runs in a background thread so it doesn't block the GTK main loop.
/// For long responses, uses Claude Haiku to generate a one-sentence summary.
pub(crate) fn speak_if_enabled(text: &str, config: &ConfigManager) {
    speak_if_enabled_with_signal(text, config, None);
}

/// Speak the response text via TTS if enabled.
///
/// If `tts_started` is provided, the flag is set to `true` when the TTS
/// audio actually begins playing (not when summarization finishes). This
/// keeps the "thinking" indicator visible until the voice starts.
pub(crate) fn speak_if_enabled_with_signal(
    text: &str,
    config: &ConfigManager,
    tts_started: Option<Arc<AtomicBool>>,
) {
    let tts_enabled = config.get_bool("voice.tts_enabled", true);
    if !tts_enabled {
        // Signal immediately if TTS is off — caller should remove thinking now
        if let Some(flag) = tts_started {
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        return;
    }

    // Short responses: speak directly (no LLM call needed)
    if let Some(short) = prepare_tts_text_short(text) {
        if short.is_empty() {
            if let Some(flag) = tts_started {
                flag.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            return;
        }
        let speak_text = short;
        std::thread::spawn(move || {
            do_tts_with_signal(&speak_text, tts_started);
        });
        return;
    }

    // Long responses: summarize with Haiku in background thread
    let raw = text.to_string();
    let api_key = config.get_str("llm.claude_api_key", "");
    std::thread::spawn(move || {
        let speak_text = summarize_for_tts(&raw, &api_key);
        do_tts_with_signal(&speak_text, tts_started);
    });
}

/// Perform TTS with an optional signal that fires when audio starts playing.
pub(crate) fn do_tts_with_signal(
    speak_text: &str,
    tts_started: Option<Arc<AtomicBool>>,
) {
    let signal = |flag: &Option<Arc<AtomicBool>>| {
        if let Some(f) = flag {
            f.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    };

    // Try Piper first (high-quality, natural sounding voice)
    let piper_model = "/home/aios/.aios/models/piper/en_US-amy-medium.onnx";
    if std::path::Path::new(piper_model).exists() {
        let mut child = match std::process::Command::new("piper")
            .args(["--model", piper_model, "--output_raw"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(_) => {
                // Piper not available, fall through to espeak
                signal(&tts_started);
                let _ = std::process::Command::new("espeak-ng")
                    .args(["-v", "en", "-s", "170"])
                    .arg(speak_text)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
                return;
            }
        };

        // Write text to piper's stdin
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            let _ = stdin.write_all(speak_text.as_bytes());
            drop(stdin); // Close stdin to signal EOF
        }

        // Pipe piper's raw audio output to aplay — signal when aplay starts
        if let Some(stdout) = child.stdout.take() {
            signal(&tts_started); // Voice is about to start!
            let _ = std::process::Command::new("aplay")
                .args(["-r", "22050", "-f", "S16_LE", "-t", "raw", "-c", "1"])
                .stdin(stdout)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        } else {
            signal(&tts_started);
        }
        let _ = child.wait();
        return;
    }

    // Fallback: espeak-ng — signal right before it speaks
    signal(&tts_started);
    let _ = std::process::Command::new("espeak-ng")
        .args(["-v", "en", "-s", "170"])
        .arg(speak_text)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// Actually perform TTS (called from background thread, no signal).
pub(crate) fn do_tts(speak_text: &str) {
    do_tts_with_signal(speak_text, None);
}
