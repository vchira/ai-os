/* AiOS — UI Widget System
   Interactive UI components for windows: buttons, inputs, checkboxes, radios.
   Dialog API lets the AI show popups that collect user input. */

#ifndef AIOS_WIDGET_H
#define AIOS_WIDGET_H

#include "include/types.h"

/* ── Widget types ── */
typedef enum {
    WIDGET_NONE = 0,
    WIDGET_LABEL,
    WIDGET_BUTTON,
    WIDGET_INPUT,
    WIDGET_CHECKBOX,
    WIDGET_RADIO,
} widget_type_t;

/* ── Single widget ── */
#define WIDGET_LABEL_MAX   48
#define WIDGET_VALUE_MAX  128

#define WFLAG_PRIMARY    0x01   /* primary/accent button */
#define WFLAG_DISABLED   0x02
#define WFLAG_FOCUSED    0x04   /* has keyboard focus */

typedef struct {
    widget_type_t type;
    int rx, ry, rw, rh;           /* position relative to content area */
    char label[WIDGET_LABEL_MAX];
    char value[WIDGET_VALUE_MAX];  /* text input value */
    int state;                     /* checkbox: 1=checked, radio: 1=selected */
    int group;                     /* radio button group ID */
    int flags;                     /* WFLAG_* */
    int id;                        /* unique per window */
    int cursor_pos;                /* text cursor in input field */
} widget_t;

#define WIN_MAX_WIDGETS 16

/* ── Dialog result codes ── */
#define DIALOG_PENDING  (-1)
#define DIALOG_CANCEL     0
#define DIALOG_OK         1

/* ── Dialog API (blocking — polls until user responds) ── */

/* Show text input dialog. Returns DIALOG_OK or DIALOG_CANCEL.
   On OK, user's text is copied to value_out (up to max bytes). */
int dialog_input(const char *title, const char *prompt,
                 const char *placeholder, char *value_out, int max);

/* Show yes/no confirmation. Returns DIALOG_OK or DIALOG_CANCEL. */
int dialog_confirm(const char *title, const char *prompt,
                   const char *yes_label, const char *no_label);

/* Show single-choice selection. Returns selected index (0-based),
   or -1 if cancelled. choices is comma-separated: "A,B,C" */
int dialog_choice(const char *title, const char *prompt,
                  const char *choices, int default_idx);

/* Show notification with OK button. type: 0=info, 1=success, 2=warning, 3=error */
void dialog_notify(const char *title, const char *message, int type);

/* ── Widget rendering (called by window manager) ── */

/* Render all widgets for a window at content area (cx, cy) */
void widget_render(const widget_t *widgets, int count, int cx, int cy);

/* Handle mouse click at (mx, my) on widgets. Returns clicked widget ID or -1. */
int widget_handle_click(widget_t *widgets, int count, int cx, int cy,
                        int mx, int my);

/* Handle key press for focused input widget. Returns 1 if consumed. */
int widget_handle_key(widget_t *widgets, int count, int *focus_idx, int ch);

#endif
