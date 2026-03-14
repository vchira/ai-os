/* AiOS — Cloud Audio APIs (TTS + STT)
   OpenAI TTS returns raw PCM. Whisper STT accepts WAV. */

#ifndef AIOS_AUDIO_API_H
#define AIOS_AUDIO_API_H

#include "types.h"

/* Text-to-Speech: send text, get PCM audio.
   out_pcm  = buffer for S16LE 24kHz mono PCM samples
   max_samples = max number of 16-bit samples that fit in out_pcm
   voice    = "alloy", "echo", "fable", "onyx", "nova", "shimmer"
   Returns number of samples written, or -1 on error. */
int tts_speak(const char *text, int16_t *out_pcm, int max_samples,
              const char *voice);

/* Speech-to-Text: send PCM audio, get text.
   pcm      = S16LE 24kHz mono PCM samples
   num_samples = number of samples
   out_text = buffer for transcription
   max_len  = size of out_text
   Returns length of transcription, or -1 on error. */
int stt_transcribe(const int16_t *pcm, int num_samples, int sample_rate,
                   char *out_text, int max_len);

#endif
