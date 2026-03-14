/* AiOS — Prompt Provider System
   Manages pluggable input/output methods for human↔AI interaction.
   Built-in: text (keyboard+VGA), voice (mic+STT / TTS+speaker). */

#include "include/prompt_provider.h"
#include "include/ac97.h"
#include "include/audio_api.h"
#include "include/string.h"
#include "include/heap.h"
#include "include/io.h"
#include "include/stdio.h"
#include "include/arch/cc.h"

extern u32_t sys_now(void);
extern void fb_print(const char *str);
extern void fb_set_color(int attr);
extern void fb_newline(void);
#include "include/debug_log.h"

#if AIOS_DEBUG
#define PDBG(fmt, ...) do { \
    char _d[120]; \
    snprintf(_d, sizeof(_d), "[%lu] prompt: " fmt, (unsigned long)sys_now(), ##__VA_ARGS__); \
    dbg_log(_d); \
} while(0)
#else
#define PDBG(fmt, ...) ((void)0)
#endif

/* ========================================================================= */
/* Provider registry                                                         */
/* ========================================================================= */

#define MAX_PROVIDERS 8

static prompt_provider_t *providers[MAX_PROVIDERS];
static int num_providers = 0;
static int active_id = 0;

int prompt_register(prompt_provider_t *provider) {
    if (num_providers >= MAX_PROVIDERS) return -1;
    providers[num_providers] = provider;
    return num_providers++;
}

int prompt_get_count(void) { return num_providers; }

prompt_provider_t *prompt_get(int id) {
    if (id < 0 || id >= num_providers) return 0;
    return providers[id];
}

int prompt_get_active(void) { return active_id; }

int prompt_set_active(int id) {
    if (id < 0 || id >= num_providers) return -1;
    if (!providers[id]->is_available()) return -1;
    active_id = id;
    PDBG("switched to provider: %s\n", providers[id]->name);
    return 0;
}

prompt_provider_t *prompt_active(void) {
    if (active_id < 0 || active_id >= num_providers) return 0;
    return providers[active_id];
}

/* ========================================================================= */
/* Text provider — keyboard input, VGA output                                */
/* ========================================================================= */

static int text_init(void) {
    return 0;  /* keyboard + VGA are already initialized */
}

static int text_get_input(char *buf, int max_len) {
    /* Text input is currently handled by ai_prompt.asm's keyboard loop.
       When the prompt is migrated to C, this will use keyboard_read_line().
       For now, return -1 to indicate the asm loop should be used. */
    (void)buf; (void)max_len;
    return -1;
}

static int text_show_output(const char *text) {
    fb_set_color(0x0B);  /* light cyan */
    fb_print(text);
    fb_newline();
    fb_set_color(0x07);  /* reset to light gray */
    return 0;
}

static int text_is_available(void) {
    return 1;  /* keyboard + VGA always available */
}

static prompt_provider_t text_provider = {
    .name = "text",
    .init = text_init,
    .get_input = text_get_input,
    .show_output = text_show_output,
    .is_available = text_is_available,
};

/* ========================================================================= */
/* Voice provider — AC97 mic + STT input, TTS + AC97 speaker output         */
/* ========================================================================= */

/* Recording parameters */
#define VOICE_SAMPLE_RATE   24000
#define VOICE_RECORD_SEC    5       /* max recording time */
#define VOICE_MAX_SAMPLES   (VOICE_SAMPLE_RATE * VOICE_RECORD_SEC)

static int16_t *voice_rec_buf = 0;
static int16_t *voice_play_buf = 0;

static int voice_init(void) {
    if (!ac97_is_ready()) return -1;

    /* Set AC97 to 24kHz for TTS compatibility */
    ac97_set_output_rate(VOICE_SAMPLE_RATE);
    ac97_set_input_rate(VOICE_SAMPLE_RATE);

    /* Allocate recording buffer (5s @ 24kHz mono = 240KB) */
    if (!voice_rec_buf) {
        voice_rec_buf = malloc(VOICE_MAX_SAMPLES * 2);
        if (!voice_rec_buf) return -1;
    }
    /* Allocate playback buffer (10s @ 24kHz mono = 480KB) */
    if (!voice_play_buf) {
        voice_play_buf = malloc(VOICE_MAX_SAMPLES * 2 * 2);
        if (!voice_play_buf) return -1;
    }

    return 0;
}

static int voice_get_input(char *buf, int max_len) {
    if (!ac97_is_ready()) return -1;

    /* Show recording prompt */
    fb_set_color(0x0C);  /* light red */
    fb_print("[Recording... speak now]");
    fb_newline();
    fb_set_color(0x07);

    /* Record audio from microphone.
       AC97 captures stereo — we'll use the left channel as mono. */
    int16_t *stereo_buf = malloc(VOICE_MAX_SAMPLES * 4);
    if (!stereo_buf) return -1;

    int frames = ac97_record(stereo_buf, VOICE_MAX_SAMPLES);

    /* Extract mono from stereo (left channel) */
    for (int i = 0; i < frames && i < VOICE_MAX_SAMPLES; i++) {
        voice_rec_buf[i] = stereo_buf[i * 2];
    }
    free(stereo_buf);

    fb_set_color(0x0A);  /* light green */
    fb_print("[Processing speech...]");
    fb_newline();
    fb_set_color(0x07);

    /* Send to Whisper for transcription */
    int len = stt_transcribe(voice_rec_buf, frames, VOICE_SAMPLE_RATE,
                              buf, max_len);
    if (len < 0) {
        fb_set_color(0x04);
        fb_print("[STT failed]");
        fb_newline();
        fb_set_color(0x07);
        return -1;
    }

    /* Echo the transcription so user sees what was understood */
    fb_set_color(0x0E);  /* yellow */
    fb_print("[You said: ");
    fb_print(buf);
    fb_print("]");
    fb_newline();
    fb_set_color(0x07);

    return len;
}

static int voice_show_output(const char *text) {
    /* Always print to screen */
    fb_set_color(0x0B);
    fb_print(text);
    fb_newline();
    fb_set_color(0x07);

    /* Also speak it via TTS */
    if (!ac97_is_ready()) return 0;

    int max_play = VOICE_MAX_SAMPLES * 2;
    int num_samples = tts_speak(text, voice_play_buf, max_play, "alloy");
    if (num_samples > 0) {
        /* Set output rate to 24kHz (TTS output rate) */
        ac97_set_output_rate(24000);
        ac97_play(voice_play_buf, num_samples, 1);  /* mono */
    }

    return 0;
}

static int voice_is_available(void) {
    return ac97_is_ready();
}

static prompt_provider_t voice_provider = {
    .name = "voice",
    .init = voice_init,
    .get_input = voice_get_input,
    .show_output = voice_show_output,
    .is_available = voice_is_available,
};

/* ========================================================================= */
/* System init                                                               */
/* ========================================================================= */

void prompt_system_init(void) {
    num_providers = 0;
    active_id = 0;

    /* Register text provider (always available) */
    prompt_register(&text_provider);

    /* Register voice provider (available if AC97 found) */
    prompt_register(&voice_provider);
    if (voice_provider.is_available()) {
        voice_init();
        PDBG("voice provider registered (AC97 available)\n");
    }

    PDBG("prompt system: %d providers, active=%s\n",
         num_providers, providers[active_id]->name);
}
