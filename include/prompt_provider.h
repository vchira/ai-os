/* AiOS — Pluggable Prompt Provider Interface
   Abstracts how humans interact with the AI.
   Text prompt = keyboard + VGA.  Voice prompt = mic + speaker + STT/TTS. */

#ifndef AIOS_PROMPT_PROVIDER_H
#define AIOS_PROMPT_PROVIDER_H

typedef struct prompt_provider {
    const char *name;           /* "text", "voice", etc. */

    /* Initialize this provider. Returns 0 on success. */
    int (*init)(void);

    /* Get user input as text (blocking).
       For text: reads keyboard until Enter.
       For voice: records audio, sends to STT API, returns transcript.
       Returns length of input, or -1 on error. */
    int (*get_input)(char *buf, int max_len);

    /* Present AI response to user.
       For text: prints to VGA.
       For voice: sends to TTS API, plays audio, also prints to VGA.
       Returns 0 on success. */
    int (*show_output)(const char *text);

    /* Check if this provider's hardware is available. */
    int (*is_available)(void);
} prompt_provider_t;

/* Register a prompt provider. Returns its ID (0-based). */
int prompt_register(prompt_provider_t *provider);

/* Get number of registered providers */
int prompt_get_count(void);

/* Get provider by ID */
prompt_provider_t *prompt_get(int id);

/* Get/set active provider */
int prompt_get_active(void);
int prompt_set_active(int id);

/* Get active provider struct */
prompt_provider_t *prompt_active(void);

/* Initialize the prompt system and register built-in providers */
void prompt_system_init(void);

#endif
