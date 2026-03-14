/* AiOS — AC97 Audio Controller Driver (Intel ICH 82801AA)
   Provides PCM playback and capture via DMA. */

#ifndef AIOS_AC97_H
#define AIOS_AC97_H

#include "types.h"

/* QEMU emulates Intel 82801AA AC97 */
#define AC97_VENDOR_ID  0x8086
#define AC97_DEVICE_ID  0x2415

/* Initialize AC97 hardware. Returns 0 on success, -1 if not found. */
int ac97_init(void);

/* Check if AC97 is ready */
int ac97_is_ready(void);

/* Set master volume (0=max, 63=min per channel). Mute bit = 0x8000. */
void ac97_set_master_volume(uint16_t vol);

/* Set PCM output volume (0=max, 31=min per channel). */
void ac97_set_pcm_volume(uint16_t vol);

/* Set sample rate for PCM output (e.g. 24000, 44100, 48000). */
int ac97_set_output_rate(uint16_t rate);

/* Set sample rate for PCM input/capture. */
int ac97_set_input_rate(uint16_t rate);

/* Play PCM audio (blocking).
   data    = S16LE samples (mono or stereo)
   samples = number of sample frames (1 frame = 1 sample if mono, L+R if stereo)
   channels = 1 (mono) or 2 (stereo)
   Returns 0 on success. */
int ac97_play(const int16_t *data, int samples, int channels);

/* Record PCM audio (blocking).
   buf     = output buffer for S16LE stereo samples
   frames  = max number of stereo frames to capture
   Returns number of frames captured. */
int ac97_record(int16_t *buf, int frames);

/* Stop any active playback/capture */
void ac97_stop_playback(void);
void ac97_stop_capture(void);

#endif
