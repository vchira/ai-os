/* AiOS — Persistent OS Settings
   Key-value settings that persist across reboots.
   Configurable via /settings command or AI set_setting tool. */

#ifndef AIOS_SETTINGS_H
#define AIOS_SETTINGS_H

#include "include/types.h"

/* Maximum number of settings and key/value lengths */
#define SETTINGS_MAX      32
#define SETTING_KEY_MAX   32
#define SETTING_VALUE_MAX 64

/* Initialize settings (loads from disk). */
void settings_init(void);

/* Get a setting value by key. Returns NULL if not found. */
const char *setting_get(const char *key);

/* Get a setting as an integer. Returns def if not found or invalid. */
int setting_get_int(const char *key, int def);

/* Set a setting. Creates or updates. Returns 0 on success, -1 on full. */
int setting_set(const char *key, const char *value);

/* Set a setting to an integer value. */
int setting_set_int(const char *key, int value);

/* Delete a setting by key. Returns 0 if found, -1 if not. */
int setting_delete(const char *key);

/* Apply all settings to the OS (theme, mouse sensitivity, etc.). */
void settings_apply(void);

/* Save settings to disk. */
void settings_save(void);

/* Dump all settings into a buffer for display. */
int settings_dump(char *buf, int max);

#endif
