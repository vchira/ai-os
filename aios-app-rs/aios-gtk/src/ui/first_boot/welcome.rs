//! Welcome, audio output test, and audio input test steps.

use gtk4::prelude::*;
use gtk4::{self as gtk, Align, Orientation};

use aios_core::i18n::{set_language, t, t_fmt};

use super::{SetupConversation, SetupStep};

impl SetupConversation {
    // -- Step 1: Welcome ----------------------------------------------------

    pub(super) fn show_welcome(&self) {
        let input_box = gtk::Box::new(Orientation::Vertical, 8);
        input_box.set_margin_top(8);

        // Language selector row.
        let lang_row = gtk::Box::new(Orientation::Horizontal, 8);
        lang_row.set_valign(Align::Center);

        let lang_label = gtk::Label::new(Some(&t("setup.welcome.language")));
        lang_label.set_halign(Align::Start);
        lang_row.append(&lang_label);

        // Language entries: (code, native display name).
        // Only en and de are currently available.
        let lang_entries: &[(&str, &str)] = &[
            ("en", "English"),
            ("de", "Deutsch"),
        ];
        let display_names: Vec<&str> = lang_entries.iter().map(|(_, name)| *name).collect();
        let lang_model = gtk::StringList::new(&display_names);
        let lang_dropdown = gtk::DropDown::new(Some(lang_model), gtk::Expression::NONE);
        lang_dropdown.set_hexpand(true);

        // Pre-select the current language.
        let current = aios_core::i18n::current_language();
        for (i, (code, _)) in lang_entries.iter().enumerate() {
            if *code == current {
                lang_dropdown.set_selected(i as u32);
                break;
            }
        }
        lang_row.append(&lang_dropdown);
        input_box.append(&lang_row);

        // Hint that more languages are coming.
        let hint_label = gtk::Label::new(Some(&t("setup.welcome.language_hint")));
        hint_label.add_css_class("dim-label");
        hint_label.set_halign(Align::Start);
        input_box.append(&hint_label);

        // "Get Started" button.
        let btn = gtk::Button::with_label(&t("setup.welcome.button"));
        btn.add_css_class("suggested-action");
        btn.add_css_class("pill");
        btn.set_halign(Align::Start);
        btn.set_margin_top(4);
        input_box.append(&btn);

        // Build the card with title and description labels that we can refresh.
        let title_text = t("setup.welcome.title");
        let desc_text = t("setup.welcome.description");
        let handle = self.chat_view.add_setup_card(
            "starred-symbolic",
            &title_text,
            &desc_text,
            Some(input_box.upcast_ref()),
        );

        // When the language dropdown changes, update the i18n system,
        // persist the selection, and refresh all translatable labels on this card.
        {
            let lang_label_ref = lang_label.clone();
            let hint_label_ref = hint_label.clone();
            let btn_ref = btn.clone();
            let config_ref = self.config.clone();
            lang_dropdown.connect_selected_notify(move |dd| {
                let idx = dd.selected() as usize;
                if idx < lang_entries.len() {
                    let (code, _) = lang_entries[idx];
                    set_language(code);

                    // Persist the language selection to config.
                    if let Some(ref mut cfg) = *config_ref.borrow_mut() {
                        let _ = cfg.set("assistant.language", serde_json::json!(code));
                    }

                    // Refresh translatable text on this card.
                    lang_label_ref.set_label(&t("setup.welcome.language"));
                    hint_label_ref.set_label(&t("setup.welcome.language_hint"));
                    btn_ref.set_label(&t("setup.welcome.button"));
                }
            });
        }

        let this = self.clone();
        btn.connect_clicked(move |_| {
            if let Some(ref h) = handle { h.dismiss(&t("setup.welcome.dismiss")); }
            this.advance(SetupStep::TestAudioOutput);
        });

        self.speak(&t("setup.welcome.tts"));
    }

    // -- Step 1b: Test Audio Output -----------------------------------------

    pub(super) fn show_test_audio_output(&self) {
        let input_box = gtk::Box::new(Orientation::Vertical, 8);
        input_box.set_margin_top(8);

        // "Replay" button — speaks the test phrase again.
        let replay_btn = gtk::Button::with_label(&t("setup.audio_output.replay"));
        replay_btn.set_halign(Align::Start);

        let this_for_replay = self.clone();
        replay_btn.connect_clicked(move |_| {
            this_for_replay.speak(&t("setup.audio_output.tts"));
        });
        input_box.append(&replay_btn);

        // Row of yes/no buttons.
        let btn_box = gtk::Box::new(Orientation::Horizontal, 8);
        btn_box.set_margin_top(4);

        let yes_btn = gtk::Button::with_label(&t("setup.audio_output.yes"));
        yes_btn.add_css_class("suggested-action");
        btn_box.append(&yes_btn);

        let no_btn = gtk::Button::with_label(&t("setup.audio_output.no"));
        btn_box.append(&no_btn);
        input_box.append(&btn_box);

        let handle = self.chat_view.add_setup_card(
            "audio-speakers-symbolic",
            &t("setup.audio_output.title"),
            &t("setup.audio_output.description"),
            Some(input_box.upcast_ref()),
        );

        let this = self.clone();
        let h = handle.clone();
        yes_btn.connect_clicked(move |_| {
            if let Some(ref h) = h { h.dismiss(&t("setup.audio_output.dismiss_works")); }
            this.chat_view.add_message("user", &t("setup.audio_output.user_yes"));
            this.advance(SetupStep::TestAudioInput);
        });

        let this = self.clone();
        no_btn.connect_clicked(move |_| {
            if let Some(ref h) = handle { h.dismiss(&t("setup.audio_output.dismiss_skipped")); }
            this.chat_view.add_message("user", &t("setup.audio_output.user_skip"));
            this.chat_view.add_message("system",
                &t("setup.audio_output.skipped"));
            this.advance(SetupStep::Country);
        });

        // Speak the test phrase.
        self.speak(&t("setup.audio_output.tts"));
    }

    // -- Step 1c: Test Audio Input ------------------------------------------

    pub(super) fn show_test_audio_input(&self) {
        let input_box = gtk::Box::new(Orientation::Vertical, 8);
        input_box.set_margin_top(8);

        let status_label = gtk::Label::new(Some(&t("setup.audio_input.speak_now")));
        status_label.set_halign(Align::Start);
        input_box.append(&status_label);

        // VU meter — real-time audio level display
        let level_bar = gtk::LevelBar::for_interval(0.0, 1.0);
        level_bar.set_value(0.0);
        level_bar.set_hexpand(true);
        level_bar.set_margin_top(4);
        level_bar.set_margin_bottom(4);
        // Add color thresholds: green (low), yellow (mid), red (high)
        level_bar.remove_offset_value(Some("low"));
        level_bar.remove_offset_value(Some("high"));
        level_bar.remove_offset_value(Some("full"));
        level_bar.add_offset_value("low", 0.3);
        level_bar.add_offset_value("high", 0.6);
        level_bar.add_offset_value("full", 0.9);
        level_bar.set_size_request(-1, 20);
        input_box.append(&level_bar);

        let level_label = gtk::Label::new(Some(&t("setup.audio_input.no_audio")));
        level_label.set_halign(Align::Start);
        level_label.add_css_class("dim-label");
        input_box.append(&level_label);

        // Buttons row
        let btn_box = gtk::Box::new(Orientation::Horizontal, 8);
        btn_box.set_margin_top(8);

        let works_btn = gtk::Button::with_label(&t("setup.audio_input.works"));
        works_btn.add_css_class("suggested-action");
        works_btn.set_sensitive(false); // enabled once audio is detected
        btn_box.append(&works_btn);

        let skip_btn = gtk::Button::with_label(&t("setup.audio_input.skip"));
        btn_box.append(&skip_btn);
        input_box.append(&btn_box);

        let handle = self.chat_view.add_setup_card(
            "audio-input-microphone-symbolic",
            &t("setup.audio_input.title"),
            &t("setup.audio_input.description"),
            Some(input_box.upcast_ref()),
        );

        // Start audio capture for the VU meter
        let mic_active = std::rc::Rc::new(std::cell::Cell::new(true));
        let mic_buffer = std::sync::Arc::new(std::sync::Mutex::new(Vec::<f32>::new()));

        // Background thread: capture mic audio
        let buf_writer = mic_buffer.clone();
        let active_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let flag_for_thread = active_flag.clone();
        std::thread::spawn(move || {
            use aios_voice::audio::capture::AudioCapture;
            let mut capture = AudioCapture::new();
            if capture.start_recording().is_err() {
                return; // no mic available
            }
            while flag_for_thread.load(std::sync::atomic::Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(50));
                if let Ok(mut buf) = capture.buffer().lock() {
                    if !buf.is_empty() {
                        let samples: Vec<f32> = std::mem::take(&mut *buf);
                        if let Ok(mut target) = buf_writer.lock() {
                            *target = samples;
                        }
                    }
                }
            }
            capture.stop_recording();
        });

        // GTK timer: update the VU meter every 60ms
        let buf_reader = mic_buffer.clone();
        let mic_active_for_timer = mic_active.clone();
        let level_bar_ref = level_bar.clone();
        let level_label_ref = level_label.clone();
        let works_btn_ref = works_btn.clone();
        let mut peak_seen = false;
        gtk::glib::timeout_add_local(std::time::Duration::from_millis(60), move || {
            if !mic_active_for_timer.get() {
                level_bar_ref.set_value(0.0);
                return gtk::glib::ControlFlow::Break;
            }

            let rms = if let Ok(samples) = buf_reader.lock() {
                if samples.is_empty() {
                    0.0
                } else {
                    let sum_sq: f32 = samples.iter().map(|&s| s * s).sum();
                    (sum_sq / samples.len() as f32).sqrt()
                }
            } else {
                0.0
            };

            // Scale RMS to 0..1 range (typical speech is 0.01-0.15)
            let level = (rms * 8.0).min(1.0);
            level_bar_ref.set_value(level as f64);

            if level > 0.05 && !peak_seen {
                peak_seen = true;
                level_label_ref.set_text(&t("setup.audio_input.detected"));
                works_btn_ref.set_sensitive(true);
            } else if level > 0.05 {
                // Keep updating with current level
                let bars = (level * 20.0) as usize;
                let bar_str: String = "\u{2588}".repeat(bars);
                level_label_ref.set_text(&format!("\u{1f3a4} {bar_str}"));
            } else if !peak_seen {
                level_label_ref.set_text(&t("setup.audio_input.waiting"));
            }

            gtk::glib::ControlFlow::Continue
        });

        // "Mic works" button
        let this = self.clone();
        let h = handle.clone();
        let mic_active_for_works = mic_active.clone();
        let flag_for_works = active_flag.clone();
        works_btn.connect_clicked(move |_| {
            mic_active_for_works.set(false);
            flag_for_works.store(false, std::sync::atomic::Ordering::Relaxed);
            if let Some(ref h) = h { h.dismiss(&t("setup.audio_input.dismiss_works")); }
            this.chat_view.add_message("user", &t("setup.audio_input.user_works"));
            this.advance(SetupStep::Country);
        });

        // "Skip" button
        let this = self.clone();
        let mic_active_for_skip = mic_active.clone();
        let flag_for_skip = active_flag.clone();
        skip_btn.connect_clicked(move |_| {
            mic_active_for_skip.set(false);
            flag_for_skip.store(false, std::sync::atomic::Ordering::Relaxed);
            if let Some(ref h) = handle { h.dismiss(&t("setup.audio_input.dismiss_skipped")); }
            this.chat_view.add_message("user", &t("setup.audio_input.user_skip"));
            this.chat_view.add_message("system",
                &t("setup.audio_input.skipped"));
            this.advance(SetupStep::Country);
        });

        self.speak(&t("setup.audio_input.tts"));
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Generate a 1-second 440Hz sine wave as a WAV file in memory.
pub(super) fn generate_test_beep() -> Result<Vec<u8>, std::io::Error> {
    let sample_rate: u32 = 44100;
    let duration_secs: f32 = 1.0;
    let frequency: f32 = 440.0;
    let num_samples = (sample_rate as f32 * duration_secs) as u32;
    let bits_per_sample: u16 = 16;
    let num_channels: u16 = 1;
    let byte_rate = sample_rate * u32::from(num_channels) * u32::from(bits_per_sample) / 8;
    let block_align = num_channels * bits_per_sample / 8;
    let data_size = num_samples * u32::from(block_align);

    let mut buf: Vec<u8> = Vec::with_capacity(44 + data_size as usize);

    // WAV header
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data_size).to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes()); // chunk size
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
    buf.extend_from_slice(&num_channels.to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&block_align.to_le_bytes());
    buf.extend_from_slice(&bits_per_sample.to_le_bytes());
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_size.to_le_bytes());

    // Generate sine wave samples with fade-in/out to avoid clicks
    let fade_samples = (sample_rate as f32 * 0.05) as u32; // 50ms fade
    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let mut amplitude = (2.0 * std::f32::consts::PI * frequency * t).sin();

        // Fade in
        if i < fade_samples {
            amplitude *= i as f32 / fade_samples as f32;
        }
        // Fade out
        if i > num_samples - fade_samples {
            amplitude *= (num_samples - i) as f32 / fade_samples as f32;
        }

        let sample = (amplitude * 0.5 * i16::MAX as f32) as i16;
        buf.extend_from_slice(&sample.to_le_bytes());
    }

    Ok(buf)
}
