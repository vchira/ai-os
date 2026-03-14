/* AiOS — UI Theme System
   Configurable color palette for all window chrome, widgets, and UI elements. */

#ifndef AIOS_THEME_H
#define AIOS_THEME_H

#include "include/types.h"

typedef struct {
    const char *name;

    /* Window chrome */
    uint32_t win_title_active;
    uint32_t win_title_inactive;
    uint32_t win_title_text;
    uint32_t win_border;
    uint32_t win_bg;
    uint32_t win_text;

    /* Scrollbar */
    uint32_t scroll_track;
    uint32_t scroll_thumb;

    /* Close button */
    uint32_t close_bg;
    uint32_t close_icon;

    /* Resize grip */
    uint32_t grip;

    /* Cursor */
    uint32_t cursor_fg;
    uint32_t cursor_bg;

    /* Buttons */
    uint32_t btn_bg;
    uint32_t btn_fg;
    uint32_t btn_border;
    uint32_t btn_primary_bg;
    uint32_t btn_primary_fg;

    /* Text input */
    uint32_t input_bg;
    uint32_t input_fg;
    uint32_t input_border;
    uint32_t input_focus_border;
    uint32_t input_cursor;
    uint32_t input_placeholder;

    /* Checkbox / Radio */
    uint32_t check_bg;
    uint32_t check_border;
    uint32_t check_mark;

    /* Labels */
    uint32_t label_fg;
    uint32_t label_dim;

    /* Status colors */
    uint32_t accent;
    uint32_t success;
    uint32_t warning;
    uint32_t error;

    /* Desktop / console background */
    uint32_t desktop_bg;
} ui_theme_t;

/* Built-in theme IDs */
enum {
    THEME_DARK,
    THEME_NORD,
    THEME_SOLARIZED,
    THEME_LIGHT,
    THEME_RETRO,
    THEME_COUNT
};

/* Get current active theme */
const ui_theme_t *theme_get(void);

/* Set active theme by ID */
void theme_set(int theme_id);

/* Get theme name by ID */
const char *theme_name(int theme_id);

/* Get number of available themes */
int theme_count(void);

/* Get current theme ID */
int theme_current_id(void);

#endif
