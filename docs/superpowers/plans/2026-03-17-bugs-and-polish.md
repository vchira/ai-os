# Bugs & Polish Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix critical bugs, UX issues, and polish items from gedanken.txt — 23 items covering crashes, display timing, voice controls, dropdowns, and visual polish.

**Architecture:** Mostly isolated fixes across GTK UI, voice system, config dialogs, and build scripts. Grouped by file/area to minimize conflicts.

**Tech Stack:** Rust, GTK4/libadwaita, CSS, JavaScript (web client), labwc config

---

## Critical Bugs

### Task 1: Fix crash when closing chat window (#18)

Closing the GTK window causes the OS to crash. The app should handle window close gracefully — either prevent close (kiosk mode) or clean shutdown.

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/app.rs`
- Modify: `aios-app-rs/aios-gtk/src/ui/main_window.rs`

- [ ] **Step 1: Read main_window.rs and app.rs to find the window close handler**
- [ ] **Step 2: Add a `close-request` signal handler that prevents default close and instead restarts or shows a confirmation**

Since AiOS IS the desktop environment, closing the window should either:
- Be prevented entirely (return `true` from `close-request` to inhibit)
- Or gracefully restart the app

```rust
window.connect_close_request(|_| {
    // Don't close — AiOS is the desktop shell
    gtk::glib::Propagation::Stop
});
```

- [ ] **Step 3: Verify compilation and commit**

---

### Task 2: Chat auto-scroll to bottom on new messages (#23)

Messages don't always scroll to bottom when new content arrives. User has to manually scroll.

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/ui/chat_view.rs`

- [ ] **Step 1: Read chat_view.rs, find the `add_message()` and `add_setup_card()` methods**
- [ ] **Step 2: After appending any content, scroll the ScrolledWindow to bottom**

```rust
// After appending content to the chat box:
if let Some(adj) = scrolled_window.vadjustment() {
    // Use idle_add to scroll after layout is complete
    gtk::glib::idle_add_local_once(move || {
        adj.set_value(adj.upper() - adj.page_size());
    });
}
```

Ensure this runs after EVERY content addition: `add_message()`, `add_setup_card()`, `add_level_message()`, `add_widget()`.

- [ ] **Step 3: Test and commit**

---

### Task 3: Show user message immediately, not after AI responds (#3)

Currently the user's message only appears in chat after the AI response comes back. It should appear instantly when sent.

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/app.rs`

- [ ] **Step 1: Read the prompt submit handler (on_submit callback) in both transition_to_normal_mode and activate_main**
- [ ] **Step 2: Ensure `chat_view.add_message("user", &text)` happens BEFORE `send_to_llm()` is called**

The current code likely already does this — check if the issue is that `add_message` is called but the display update is deferred. May need `chat_view.queue_draw()` or the auto-scroll fix from Task 2 to make it visible.

- [ ] **Step 3: Verify and commit**

---

## Voice Issues

### Task 4: Voice/text timing — text should appear alongside voice (#1)

Voice starts playing but text only appears after voice finishes. Text and voice should be concurrent.

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/app.rs` — the LLM response handler

- [ ] **Step 1: Read how LLM responses are displayed and how TTS is triggered**
- [ ] **Step 2: Ensure text is added to chat_view FIRST, then TTS starts in a background thread**

The pattern should be:
```rust
// 1. Display text immediately
chat_view.add_message("assistant", &response_text);
// 2. Start TTS in background (non-blocking)
let text_for_tts = response_text.clone();
std::thread::spawn(move || { speak(&text_for_tts); });
```

NOT:
```rust
// BAD: speak blocks, then display
speak(&response_text); // blocks until done
chat_view.add_message("assistant", &response_text);
```

- [ ] **Step 3: Verify and commit**

---

### Task 5: Stop voice button per message (#2)

Add a button to stop TTS for the current message (not mute all).

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/ui/chat_view.rs`

- [ ] **Step 1: When an assistant message is added while TTS is active, add a small "Stop" button (speaker-off icon) next to the message**
- [ ] **Step 2: Clicking it kills the current TTS process (pkill piper/espeak-ng/aplay)**
- [ ] **Step 3: The button disappears after TTS finishes or is stopped**

```rust
let stop_btn = gtk::Button::from_icon_name("audio-volume-muted-symbolic");
stop_btn.add_css_class("flat");
stop_btn.add_css_class("circular");
stop_btn.set_tooltip_text(Some("Stop reading"));
stop_btn.connect_clicked(|_| {
    // Kill TTS processes
    let _ = std::process::Command::new("pkill").args(["-f", "piper"]).status();
    let _ = std::process::Command::new("pkill").args(["-f", "espeak-ng"]).status();
    let _ = std::process::Command::new("pkill").args(["-f", "aplay"]).status();
});
```

- [ ] **Step 4: Commit**

---

### Task 6: Voice throughout first-boot setup (#4)

TTS only works in the first setup message. Should work for all setup cards.

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/ui/first_boot.rs`

- [ ] **Step 1: Check the `speak()` method in SetupConversation — verify it's called in every show_* method**
- [ ] **Step 2: Ensure every setup step calls `self.speak()` with appropriate text**
- [ ] **Step 3: Check if the issue is that TTS is killed when advancing steps (stop_speaking) — ensure new step's TTS starts AFTER the kill**
- [ ] **Step 4: Commit**

---

## Setup & Config UX

### Task 7: Hide configure button during setup (#5)

The settings gear icon in the header bar shouldn't be visible until first-boot setup is complete.

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/ui/main_window.rs`
- Modify: `aios-app-rs/aios-gtk/src/app.rs`

- [ ] **Step 1: In main_window, make the settings button initially hidden (set_visible(false))**
- [ ] **Step 2: In transition_to_normal_mode() and activate_main(), set it visible**
- [ ] **Step 3: Commit**

---

### Task 8: Only show configured providers in dropdown (#6)

After setup, the provider dropdown shows all providers including unconfigured ones. Only show providers with API keys.

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/app.rs`

- [ ] **Step 1: Read how the provider dropdown is populated in activate_main() and transition_to_normal_mode()**
- [ ] **Step 2: Filter to only providers that have a non-empty API key in config**

This may already be partially done (we added `available_providers` filtering). Check and fix if needed.

- [ ] **Step 3: Commit**

---

### Task 9: Hide message input until setup completes (#20)

The text input field should not be visible during first-boot setup. The "Start Chatting" button at the end of setup should make it appear.

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/ui/main_window.rs`
- Modify: `aios-app-rs/aios-gtk/src/app.rs`
- Modify: `aios-app-rs/aios-gtk/src/ui/first_boot.rs`

- [ ] **Step 1: In run_first_boot_setup(), hide the prompt input: `prompt_input.set_visible(false)`**
- [ ] **Step 2: In the setup's "Start Chatting" button click / finish(), show it: `prompt_input.set_visible(true)`**
- [ ] **Step 3: In activate_main() (normal boot), keep it visible as-is**
- [ ] **Step 4: Commit**

---

### Task 10: API keys edit-only in config dialog (#9)

In the settings/config dialog, API keys should be editable but not viewable (password-style entry showing dots).

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/ui/settings_dialog.rs`

- [ ] **Step 1: Find the API key fields in the settings dialog**
- [ ] **Step 2: Change from gtk::Entry to gtk::PasswordEntry (or set visibility to false)**
- [ ] **Step 3: Show placeholder like "••••••••" if a key exists, empty if not**
- [ ] **Step 4: Only save if the user actually typed something new (don't overwrite with dots)**
- [ ] **Step 5: Commit**

---

### Task 11: Model selection as dropdown (#11)

Model config should be a dropdown, not a text input. Show human-friendly names.

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/ui/settings_dialog.rs`

- [ ] **Step 1: Replace model text input with dropdown**
- [ ] **Step 2: Populate with available models per provider:**

Claude models:
- "Claude Sonnet 4" → `claude-sonnet-4-20250514`
- "Claude Opus 4" → `claude-opus-4-20250514`
- "Claude Haiku 3.5" → `claude-haiku-4-5-20251001`

ChatGPT models:
- "GPT-4o" → `gpt-4o`
- "GPT-4o Mini" → `gpt-4o-mini`
- "GPT-4 Turbo" → `gpt-4-turbo`

- [ ] **Step 3: Switch model list when provider changes**
- [ ] **Step 4: Commit**

---

### Task 12: Voice selection as dropdown (#13)

Voice config should be a dropdown listing available Piper voices.

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/ui/settings_dialog.rs`

- [ ] **Step 1: Scan ~/.aios/models/piper/ for .onnx files**
- [ ] **Step 2: Create human-readable names: "en_US-amy-medium" → "Amy (English US, Medium Quality)"**
- [ ] **Step 3: Replace text input with dropdown populated from scan**
- [ ] **Step 4: Add espeak-ng voices as fallback option**
- [ ] **Step 5: Commit**

---

### Task 13: Keyboard layout dropdown with human names (#15)

Keyboard layout config should be a dropdown with "German", "US English", etc. — not "de", "us".

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/ui/settings_dialog.rs`

- [ ] **Step 1: Create a mapping of layout codes to human names**

```rust
const KEYBOARD_LAYOUTS: &[(&str, &str)] = &[
    ("us", "US English"),
    ("de", "German"),
    ("fr", "French"),
    ("es", "Spanish"),
    ("it", "Italian"),
    ("pt", "Portuguese"),
    ("ro", "Romanian"),
    ("gb", "British English"),
    ("nl", "Dutch"),
    ("pl", "Polish"),
    ("cz", "Czech"),
    ("hu", "Hungarian"),
    ("se", "Swedish"),
    ("no", "Norwegian"),
    ("dk", "Danish"),
    ("fi", "Finnish"),
    ("jp", "Japanese"),
    ("kr", "Korean"),
    ("ru", "Russian"),
    ("tr", "Turkish"),
    ("ar", "Arabic"),
];
```

- [ ] **Step 2: Replace text input with dropdown using these names**
- [ ] **Step 3: Same pattern for locale config (#16)**
- [ ] **Step 4: Commit**

---

### Task 14: Toggle button icons for mic/speaker (#17)

Mic and speaker toggle buttons don't visually change when toggled. Need distinct icons and colors.

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/ui/main_window.rs`

- [ ] **Step 1: Find the mic and speaker toggle buttons**
- [ ] **Step 2: On toggle, change icons:**

```rust
// Speaker
if active {
    speaker_btn.set_icon_name("audio-volume-high-symbolic"); // green
    speaker_btn.remove_css_class("muted");
} else {
    speaker_btn.set_icon_name("audio-volume-muted-symbolic"); // red
    speaker_btn.add_css_class("muted");
}

// Mic
if active {
    mic_btn.set_icon_name("audio-input-microphone-symbolic"); // green
    mic_btn.remove_css_class("muted");
} else {
    mic_btn.set_icon_name("microphone-disabled-symbolic"); // red
    mic_btn.add_css_class("muted");
}
```

- [ ] **Step 3: Add CSS for .muted class: red tint**
- [ ] **Step 4: Commit**

---

## Command System

### Task 15: Command popup should execute on selection (#21, #22)

When typing `/` and selecting a command from the popup, it should execute immediately — not just insert text. Also support keyboard navigation (arrow keys + Enter).

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/ui/prompt_input.rs` (or wherever the autocomplete popup is)

- [ ] **Step 1: Find the command autocomplete popup implementation**
- [ ] **Step 2: On click or Enter, execute the command instead of just inserting text**
- [ ] **Step 3: Ensure arrow keys navigate the list and Enter selects**
- [ ] **Step 4: Commit**

---

### Task 16: Commands should open UI_PANELs like the AI would (#24)

Commands like `/keyboard` should display an interactive panel (dropdown to choose layout) instead of just printing text. Same as what the AI would show if asked.

**Files:**
- Modify: `aios-app-rs/aios-core/src/config/commands.rs`
- Modify: `aios-app-rs/aios-gtk/src/app.rs`

- [ ] **Step 1: For commands that configure settings (keyboard, resolution, theme, voice, language), return a CommandResult variant that triggers a UI_PANEL**
- [ ] **Step 2: The panel shows the same dropdowns/options the AI would show**
- [ ] **Step 3: After selection, dismiss the panel (same as setup cards)**
- [ ] **Step 4: Commit**

---

## Visual Polish

### Task 17: Alt-Tab window list styling (#10)

The alt-tab window switcher shows technical/ugly names. Needs user-friendly labeling.

**Files:**
- Modify: `distro/_inner_build.sh` — labwc config section

- [ ] **Step 1: Read the labwc rc.xml configuration in the build script**
- [ ] **Step 2: Configure the window switcher to show application names (not window class)**
- [ ] **Step 3: Ensure AiOS windows have proper titles set via GTK**
- [ ] **Step 4: Commit**

---

### Task 18: Theme change implementation (#14)

Theme toggle (dark/light/auto) currently does nothing.

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/app.rs` or `settings_dialog.rs`

- [ ] **Step 1: Find the theme command/setting handler**
- [ ] **Step 2: Implement theme switching using libadwaita's color scheme API:**

```rust
let style_manager = adw::StyleManager::default();
match theme.as_str() {
    "dark" => style_manager.set_color_scheme(adw::ColorScheme::ForceDark),
    "light" => style_manager.set_color_scheme(adw::ColorScheme::ForceLight),
    "auto" => style_manager.set_color_scheme(adw::ColorScheme::Default),
    _ => {}
}
```

- [ ] **Step 3: Persist the choice in config**
- [ ] **Step 4: Apply on startup**
- [ ] **Step 5: Commit**

---

### Task 19: Boot menu screen styling (#19)

The GRUB/syslinux boot menu looks bad with strange characters. Needs a clean design.

**Files:**
- Modify: `distro/_inner_build.sh` — boot configuration

- [ ] **Step 1: Find the boot menu configuration (GRUB or syslinux/isolinux)**
- [ ] **Step 2: Clean up menu entries — "AiOS Live" as the only visible option, clean text**
- [ ] **Step 3: Set a dark background color, remove garbled characters**
- [ ] **Step 4: Auto-boot after 3 second timeout**
- [ ] **Step 5: Commit**

---

### Task 20: Resolution auto-detect (#25)

Resolution change should detect available resolutions and preselect the best one.

**Files:**
- Modify: `aios-app-rs/aios-core/src/config/commands.rs` — `/resolution` command

- [ ] **Step 1: Use `wlr-randr` or `swaymsg -t get_outputs` to detect available resolutions**
- [ ] **Step 2: Show as dropdown sorted by size (largest first)**
- [ ] **Step 3: Preselect the current/best resolution**
- [ ] **Step 4: Commit**

---

### Task 21: Remove or explain "extra system prompt" (#12)

The "extra system prompt" field in settings is confusing for users.

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/ui/settings_dialog.rs`

- [ ] **Step 1: Either remove the field entirely (YAGNI for most users) or add a clear tooltip/description**
- [ ] **Step 2: If keeping, rename to "Custom AI Instructions" with placeholder: "e.g., 'Always respond in German' or 'Be concise'"**
- [ ] **Step 3: Commit**

---

### Task 22: Backup provider question fix (#7)

The setup should always ask if the user wants to add a backup provider (even if no key is pre-configured). The pre-fill logic currently skips the question entirely when no second key exists.

**Files:**
- Modify: `aios-app-rs/aios-gtk/src/ui/first_boot.rs`

- [ ] **Step 1: In show_add_backup(), remove the early return when other provider has no key**
- [ ] **Step 2: Always show the "Add backup?" question — if they say yes and no key is pre-filled, they'll enter it manually**
- [ ] **Step 3: Commit**

---

### Task 23: Final verification

- [ ] **Step 1: Run full workspace tests**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo test --workspace`

- [ ] **Step 2: Verify release build**

Run: `cd aios-app-rs && CARGO_TARGET_DIR=/tmp/aios-check cargo check --release`

- [ ] **Step 3: Verify shell scripts**

Run: `bash -n distro/_inner_build.sh && echo OK`

- [ ] **Step 4: Commit any fixups**
