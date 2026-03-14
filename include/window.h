/* AiOS — Window Manager */
#ifndef AIOS_WINDOW_H
#define AIOS_WINDOW_H

#include "include/types.h"
#include "include/widget.h"

#define WIN_MAX        8
#define WIN_CLOSABLE   0x01
#define WIN_RESIZABLE  0x02
#define WIN_SCROLLABLE 0x04
#define WIN_VISIBLE    0x08
#define WIN_FOCUSED    0x10

typedef enum { WIN_TEXT, WIN_IMAGE } win_content_t;

typedef struct {
    int x, y, w, h;         /* outer bounds (including titlebar + border) */
    char title[64];
    int flags;
    win_content_t type;

    /* Text content */
    char *text;
    int text_len;
    int text_cap;
    int scroll_y;            /* scroll offset in lines */

    /* Image content */
    uint32_t *img;
    int img_w, img_h;

    /* Widgets */
    widget_t widgets[WIN_MAX_WIDGETS];
    int widget_count;
    int focus_widget;        /* index of focused widget (-1 = none) */
    int clicked_widget;      /* last clicked widget ID (-1 = none) */

    int active;
    int dirty;
} window_t;

/* Create a window. Returns window ID (0-7) or -1 on failure. */
int win_create(const char *title, int x, int y, int w, int h, int flags);

/* Close and destroy a window. */
void win_close(int id);

/* Bring window to front and give it focus. */
void win_focus(int id);

/* Append text to a text window's content. */
void win_append_text(int id, const char *text);

/* Set image content for a window (copies pixel data). */
void win_set_image(int id, const uint32_t *pixels, int w, int h);

/* Set widgets for a window (copies widget array). */
void win_set_widgets(int id, const widget_t *widgets, int count);

/* Mark a window as needing repaint. */
void win_set_dirty(int id);

/* Check if a window is still active (not closed). */
int win_is_active(int id);

/* Get the last clicked widget ID for a window (resets after read). */
int win_get_clicked_widget(int id);

/* Render all visible windows (call after fb_render_console). */
void win_render_all(void);

/* Process mouse events and update windows. Call periodically. */
void win_poll(void);

/* Query state */
int win_any_visible(void);

/* Get raw pointer to a window struct (for prompt window backspace etc.) */
window_t *win_get_ptr(int id);

#endif
