/* AiOS — Cloud Audio APIs
   TTS: OpenAI /v1/audio/speech → raw PCM (24kHz S16LE mono)
   STT: OpenAI /v1/audio/transcriptions ← WAV file (multipart) */

#include "include/audio_api.h"
#include "include/tls_client.h"
#include "include/string.h"
#include "include/heap.h"
#include "include/io.h"
#include "include/stdio.h"
#include "include/arch/cc.h"

extern u32_t sys_now(void);
#include "include/debug_log.h"

/* API key accessor (from llm_provider.c) */
extern const char *get_openai_key(void);

#if AIOS_DEBUG
#define ADBG(fmt, ...) do { \
    char _d[120]; \
    snprintf(_d, sizeof(_d), "[%lu] audio: " fmt, (unsigned long)sys_now(), ##__VA_ARGS__); \
    dbg_log(_d); \
} while(0)
#else
#define ADBG(fmt, ...) ((void)0)
#endif

/* ========================================================================= */
/* TTS — Text to Speech via OpenAI                                           */
/* ========================================================================= */

int tts_speak(const char *text, int16_t *out_pcm, int max_samples,
              const char *voice) {
    const char *api_key = get_openai_key();
    if (!api_key || strcmp(api_key, "your-api-key-here") == 0)
        return -1;

    if (!voice) voice = "alloy";

    /* Build JSON body */
    char *body = malloc(4096);
    if (!body) return -1;

    int pos = 0;
    const char *s;
    s = "{\"model\":\"tts-1\",\"voice\":\"";
    memcpy(body + pos, s, strlen(s)); pos += strlen(s);
    memcpy(body + pos, voice, strlen(voice)); pos += strlen(voice);
    s = "\",\"input\":\"";
    memcpy(body + pos, s, strlen(s)); pos += strlen(s);

    /* JSON-escape the text */
    const char *t = text;
    while (*t && pos < 3900) {
        if (*t == '"' || *t == '\\') body[pos++] = '\\';
        else if (*t == '\n') { body[pos++] = '\\'; body[pos++] = 'n'; t++; continue; }
        body[pos++] = *t++;
    }

    s = "\",\"response_format\":\"pcm\"}";
    memcpy(body + pos, s, strlen(s)); pos += strlen(s);
    body[pos] = '\0';

    /* Headers */
    char headers[256];
    int hpos = 0;
    s = "Content-Type: application/json\r\nAuthorization: Bearer ";
    memcpy(headers + hpos, s, strlen(s)); hpos += strlen(s);
    memcpy(headers + hpos, api_key, strlen(api_key)); hpos += strlen(api_key);
    s = "\r\n";
    memcpy(headers + hpos, s, strlen(s)); hpos += strlen(s);
    headers[hpos] = '\0';

    /* Response buffer — PCM data is binary, not text.
       We use the resp_buf as raw bytes. max_samples * 2 bytes. */
    int resp_max = max_samples * 2;
    if (resp_max > 512 * 1024) resp_max = 512 * 1024;
    char *resp_buf = malloc(resp_max);
    if (!resp_buf) { free(body); return -1; }

    ADBG("TTS: sending %d chars to OpenAI...\n", (int)strlen(text));

    int ret = https_post("api.openai.com", "/v1/audio/speech",
                          headers, body, resp_buf, resp_max);
    free(body);

    if (ret <= 0) {
        ADBG("TTS: https_post failed ret=%d\n", ret);
        free(resp_buf);
        return -1;
    }

    /* Response is raw PCM bytes (24kHz S16LE mono) */
    int num_samples = ret / 2;
    if (num_samples > max_samples) num_samples = max_samples;
    memcpy(out_pcm, resp_buf, num_samples * 2);
    free(resp_buf);

    ADBG("TTS: got %d samples (%dms at 24kHz)\n",
         num_samples, num_samples * 1000 / 24000);
    return num_samples;
}

/* ========================================================================= */
/* STT — Speech to Text via OpenAI Whisper                                   */
/* ========================================================================= */

/* Build a minimal WAV header for S16LE PCM */
static void build_wav_header(uint8_t *hdr, int num_samples, int sample_rate,
                              int channels) {
    int data_size = num_samples * channels * 2;
    int file_size = 36 + data_size;
    int byte_rate = sample_rate * channels * 2;
    int block_align = channels * 2;

    /* RIFF header */
    memcpy(hdr + 0, "RIFF", 4);
    hdr[4] = file_size & 0xFF;
    hdr[5] = (file_size >> 8) & 0xFF;
    hdr[6] = (file_size >> 16) & 0xFF;
    hdr[7] = (file_size >> 24) & 0xFF;
    memcpy(hdr + 8, "WAVE", 4);

    /* fmt chunk */
    memcpy(hdr + 12, "fmt ", 4);
    hdr[16] = 16; hdr[17] = 0; hdr[18] = 0; hdr[19] = 0;  /* chunk size = 16 */
    hdr[20] = 1; hdr[21] = 0;  /* PCM format */
    hdr[22] = channels; hdr[23] = 0;
    hdr[24] = sample_rate & 0xFF;
    hdr[25] = (sample_rate >> 8) & 0xFF;
    hdr[26] = (sample_rate >> 16) & 0xFF;
    hdr[27] = (sample_rate >> 24) & 0xFF;
    hdr[28] = byte_rate & 0xFF;
    hdr[29] = (byte_rate >> 8) & 0xFF;
    hdr[30] = (byte_rate >> 16) & 0xFF;
    hdr[31] = (byte_rate >> 24) & 0xFF;
    hdr[32] = block_align & 0xFF;
    hdr[33] = (block_align >> 8) & 0xFF;
    hdr[34] = 16; hdr[35] = 0;  /* bits per sample */

    /* data chunk */
    memcpy(hdr + 36, "data", 4);
    hdr[40] = data_size & 0xFF;
    hdr[41] = (data_size >> 8) & 0xFF;
    hdr[42] = (data_size >> 16) & 0xFF;
    hdr[43] = (data_size >> 24) & 0xFF;
}

/* Build multipart/form-data body for Whisper API.
   Returns total body length, writes boundary string to boundary_out. */
static int build_multipart_body(char *out, int max,
                                 const int16_t *pcm, int num_samples,
                                 int sample_rate, int channels,
                                 const char *boundary) {
    int pos = 0;
    const char *s;

    /* Part 1: model field */
    s = "--"; memcpy(out + pos, s, 2); pos += 2;
    memcpy(out + pos, boundary, strlen(boundary)); pos += strlen(boundary);
    s = "\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\nwhisper-1\r\n";
    memcpy(out + pos, s, strlen(s)); pos += strlen(s);

    /* Part 2: response_format field */
    s = "--"; memcpy(out + pos, s, 2); pos += 2;
    memcpy(out + pos, boundary, strlen(boundary)); pos += strlen(boundary);
    s = "\r\nContent-Disposition: form-data; name=\"response_format\"\r\n\r\ntext\r\n";
    memcpy(out + pos, s, strlen(s)); pos += strlen(s);

    /* Part 3: audio file */
    s = "--"; memcpy(out + pos, s, 2); pos += 2;
    memcpy(out + pos, boundary, strlen(boundary)); pos += strlen(boundary);
    s = "\r\nContent-Disposition: form-data; name=\"file\"; filename=\"audio.wav\"\r\n"
        "Content-Type: audio/wav\r\n\r\n";
    memcpy(out + pos, s, strlen(s)); pos += strlen(s);

    /* WAV header (44 bytes) */
    if (pos + 44 > max) return -1;
    build_wav_header((uint8_t *)(out + pos), num_samples, sample_rate, channels);
    pos += 44;

    /* PCM data */
    int data_bytes = num_samples * channels * 2;
    if (pos + data_bytes > max - 64) return -1;
    memcpy(out + pos, pcm, data_bytes);
    pos += data_bytes;

    /* Closing boundary */
    s = "\r\n--"; memcpy(out + pos, s, 4); pos += 4;
    memcpy(out + pos, boundary, strlen(boundary)); pos += strlen(boundary);
    s = "--\r\n"; memcpy(out + pos, s, 4); pos += 4;

    return pos;
}

int stt_transcribe(const int16_t *pcm, int num_samples, int sample_rate,
                   char *out_text, int max_len) {
    const char *api_key = get_openai_key();
    if (!api_key || strcmp(api_key, "your-api-key-here") == 0)
        return -1;

    /* Use a fixed boundary for multipart encoding */
    const char *boundary = "----AiOSAudioBoundary9876";

    /* Build multipart body (WAV header + PCM data) */
    int body_max = 44 + num_samples * 2 + 2048;  /* WAV + PCM + multipart overhead */
    char *body = malloc(body_max);
    if (!body) return -1;

    int body_len = build_multipart_body(body, body_max, pcm, num_samples,
                                         sample_rate, 1, boundary);
    if (body_len < 0) {
        free(body);
        return -1;
    }

    /* Build headers with Content-Type: multipart/form-data */
    char headers[512];
    int hpos = 0;
    const char *s;
    s = "Content-Type: multipart/form-data; boundary=";
    memcpy(headers + hpos, s, strlen(s)); hpos += strlen(s);
    memcpy(headers + hpos, boundary, strlen(boundary)); hpos += strlen(boundary);
    s = "\r\nAuthorization: Bearer ";
    memcpy(headers + hpos, s, strlen(s)); hpos += strlen(s);
    memcpy(headers + hpos, api_key, strlen(api_key)); hpos += strlen(api_key);
    s = "\r\n";
    memcpy(headers + hpos, s, strlen(s)); hpos += strlen(s);
    headers[hpos] = '\0';

    char *resp_buf = malloc(4096);
    if (!resp_buf) { free(body); return -1; }

    ADBG("STT: sending %d samples (%dHz) to Whisper...\n", num_samples, sample_rate);

    int ret = https_post_bin("api.openai.com", "/v1/audio/transcriptions",
                             headers, body, body_len, resp_buf, 4096);
    free(body);

    if (ret <= 0) {
        ADBG("STT: https_post failed ret=%d\n", ret);
        free(resp_buf);
        return -1;
    }

    /* Response format "text" = plain text, no JSON wrapping */
    int text_len = ret;
    if (text_len >= max_len) text_len = max_len - 1;
    memcpy(out_text, resp_buf, text_len);
    out_text[text_len] = '\0';
    free(resp_buf);

    /* Trim trailing whitespace */
    while (text_len > 0 && (out_text[text_len - 1] == '\n' ||
           out_text[text_len - 1] == '\r' || out_text[text_len - 1] == ' ')) {
        out_text[--text_len] = '\0';
    }

    ADBG("STT: transcription = \"%s\" (%d chars)\n", out_text, text_len);
    return text_len;
}
