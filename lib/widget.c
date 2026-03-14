/* AiOS — UI Widget System
   Renders interactive UI components and manages dialog popups. */

#include "include/widget.h"
#include "include/theme.h"
#include "include/framebuffer.h"
#include "include/window.h"
#include "include/mouse.h"
#include "include/string.h"
#include "include/heap.h"

#define FONT_W 8
#define FONT_H 16

/* ========================================================================= */
/* Widget rendering                                                          */
/* ========================================================================= */

static void render_label(const widget_t *w, int cx, int cy) {
    const ui_theme_t *t = theme_get();
    uint32_t fg = (w->flags & WFLAG_DISABLED) ? t->label_dim : t->label_fg;
    fb_draw_text_xy(cx + w->rx, cy + w->ry, w->label, fg, t->win_bg);
}

static void render_button(const widget_t *w, int cx, int cy) {
    const ui_theme_t *t = theme_get();
    int x = cx + w->rx, y = cy + w->ry;
    uint32_t bg   = (w->flags & WFLAG_PRIMARY) ? t->btn_primary_bg : t->btn_bg;
    uint32_t fg   = (w->flags & WFLAG_PRIMARY) ? t->btn_primary_fg : t->btn_fg;
    uint32_t bord = t->btn_border;

    if (w->flags & WFLAG_DISABLED) {
        bg = t->btn_bg;
        fg = t->label_dim;
        bord = t->label_dim;
    }

    /* Border */
    fb_fill_rect(x, y, w->rw, 1, bord);
    fb_fill_rect(x, y + w->rh - 1, w->rw, 1, bord);
    fb_fill_rect(x, y, 1, w->rh, bord);
    fb_fill_rect(x + w->rw - 1, y, 1, w->rh, bord);
    /* Background */
    fb_fill_rect(x + 1, y + 1, w->rw - 2, w->rh - 2, bg);
    /* Label centered */
    int tlen = strlen(w->label);
    int tx = x + (w->rw - tlen * FONT_W) / 2;
    int ty = y + (w->rh - FONT_H) / 2;
    fb_draw_text_xy(tx, ty, w->label, fg, bg);
}

static void render_input(const widget_t *w, int cx, int cy) {
    const ui_theme_t *t = theme_get();
    int x = cx + w->rx, y = cy + w->ry;
    uint32_t bord = (w->flags & WFLAG_FOCUSED) ? t->input_focus_border : t->input_border;

    /* Border */
    fb_fill_rect(x, y, w->rw, 1, bord);
    fb_fill_rect(x, y + w->rh - 1, w->rw, 1, bord);
    fb_fill_rect(x, y, 1, w->rh, bord);
    fb_fill_rect(x + w->rw - 1, y, 1, w->rh, bord);
    /* Background */
    fb_fill_rect(x + 1, y + 1, w->rw - 2, w->rh - 2, t->input_bg);

    /* Text or placeholder */
    int tx = x + 4;
    int ty = y + (w->rh - FONT_H) / 2;
    int max_chars = (w->rw - 8) / FONT_W;
    if (max_chars < 1) max_chars = 1;

    if (w->value[0]) {
        /* Show text, scrolled so cursor is visible */
        int vlen = strlen(w->value);
        int start = 0;
        if (w->cursor_pos > max_chars - 1)
            start = w->cursor_pos - max_chars + 1;
        if (start > vlen) start = vlen;

        for (int i = 0; i < max_chars && start + i < vlen; i++) {
            fb_draw_char_xy(tx + i * FONT_W, ty,
                            (uint8_t)w->value[start + i], t->input_fg, t->input_bg);
        }

        /* Draw cursor */
        if (w->flags & WFLAG_FOCUSED) {
            int cx2 = tx + (w->cursor_pos - start) * FONT_W;
            if (cx2 >= x + 1 && cx2 < x + w->rw - 2)
                fb_fill_rect(cx2, ty, 2, FONT_H, t->input_cursor);
        }
    } else {
        /* Placeholder text */
        const char *ph = w->label;
        for (int i = 0; i < max_chars && ph[i]; i++) {
            fb_draw_char_xy(tx + i * FONT_W, ty,
                            (uint8_t)ph[i], t->input_placeholder, t->input_bg);
        }
        /* Cursor at start */
        if (w->flags & WFLAG_FOCUSED)
            fb_fill_rect(tx, ty, 2, FONT_H, t->input_cursor);
    }
}

static void render_checkbox(const widget_t *w, int cx, int cy) {
    const ui_theme_t *t = theme_get();
    int x = cx + w->rx, y = cy + w->ry;
    int bsz = 14;
    int by = y + (w->rh - bsz) / 2;

    /* Box border */
    fb_fill_rect(x, by, bsz, 1, t->check_border);
    fb_fill_rect(x, by + bsz - 1, bsz, 1, t->check_border);
    fb_fill_rect(x, by, 1, bsz, t->check_border);
    fb_fill_rect(x + bsz - 1, by, 1, bsz, t->check_border);
    /* Box fill */
    fb_fill_rect(x + 1, by + 1, bsz - 2, bsz - 2, t->check_bg);

    /* Checkmark */
    if (w->state) {
        fb_fill_rect(x + 3, by + 7, 2, 3, t->check_mark);
        fb_fill_rect(x + 5, by + 8, 2, 2, t->check_mark);
        fb_fill_rect(x + 7, by + 5, 2, 5, t->check_mark);
        fb_fill_rect(x + 9, by + 3, 2, 4, t->check_mark);
    }

    /* Label */
    int tx = x + bsz + 6;
    int ty = y + (w->rh - FONT_H) / 2;
    fb_draw_text_xy(tx, ty, w->label, t->label_fg, t->win_bg);
}

static void render_radio(const widget_t *w, int cx, int cy) {
    const ui_theme_t *t = theme_get();
    int x = cx + w->rx, y = cy + w->ry;
    int bsz = 14;
    int by = y + (w->rh - bsz) / 2;

    /* Outer circle approximation (small rounded box) */
    fb_fill_rect(x + 2, by, bsz - 4, 1, t->check_border);
    fb_fill_rect(x + 2, by + bsz - 1, bsz - 4, 1, t->check_border);
    fb_fill_rect(x, by + 2, 1, bsz - 4, t->check_border);
    fb_fill_rect(x + bsz - 1, by + 2, 1, bsz - 4, t->check_border);
    fb_fill_rect(x + 1, by + 1, 1, 1, t->check_border);
    fb_fill_rect(x + bsz - 2, by + 1, 1, 1, t->check_border);
    fb_fill_rect(x + 1, by + bsz - 2, 1, 1, t->check_border);
    fb_fill_rect(x + bsz - 2, by + bsz - 2, 1, 1, t->check_border);
    /* Inner fill */
    fb_fill_rect(x + 2, by + 1, bsz - 4, bsz - 2, t->check_bg);
    fb_fill_rect(x + 1, by + 2, bsz - 2, bsz - 4, t->check_bg);

    /* Dot if selected */
    if (w->state) {
        fb_fill_rect(x + 4, by + 4, bsz - 8, bsz - 8, t->check_mark);
        fb_fill_rect(x + 5, by + 3, bsz - 10, bsz - 6, t->check_mark);
        fb_fill_rect(x + 3, by + 5, bsz - 6, bsz - 10, t->check_mark);
    }

    /* Label */
    int tx = x + bsz + 6;
    int ty = y + (w->rh - FONT_H) / 2;
    fb_draw_text_xy(tx, ty, w->label, t->label_fg, t->win_bg);
}

void widget_render(const widget_t *widgets, int count, int cx, int cy) {
    for (int i = 0; i < count; i++) {
        const widget_t *w = &widgets[i];
        switch (w->type) {
            case WIDGET_LABEL:    render_label(w, cx, cy);    break;
            case WIDGET_BUTTON:   render_button(w, cx, cy);   break;
            case WIDGET_INPUT:    render_input(w, cx, cy);    break;
            case WIDGET_CHECKBOX: render_checkbox(w, cx, cy); break;
            case WIDGET_RADIO:    render_radio(w, cx, cy);    break;
            default: break;
        }
    }
}

/* ========================================================================= */
/* Widget interaction                                                        */
/* ========================================================================= */

int widget_handle_click(widget_t *widgets, int count, int cx, int cy,
                        int mx, int my) {
    for (int i = 0; i < count; i++) {
        widget_t *w = &widgets[i];
        if (w->type == WIDGET_NONE || w->type == WIDGET_LABEL) continue;
        if (w->flags & WFLAG_DISABLED) continue;

        int x = cx + w->rx, y = cy + w->ry;
        if (mx < x || mx >= x + w->rw || my < y || my >= y + w->rh)
            continue;

        /* Hit! */
        switch (w->type) {
            case WIDGET_BUTTON:
                return w->id;

            case WIDGET_INPUT:
                /* Focus this input */
                for (int j = 0; j < count; j++)
                    widgets[j].flags &= ~WFLAG_FOCUSED;
                w->flags |= WFLAG_FOCUSED;
                return w->id;

            case WIDGET_CHECKBOX:
                w->state = !w->state;
                return w->id;

            case WIDGET_RADIO:
                /* Deselect others in same group */
                for (int j = 0; j < count; j++) {
                    if (widgets[j].type == WIDGET_RADIO &&
                        widgets[j].group == w->group)
                        widgets[j].state = 0;
                }
                w->state = 1;
                return w->id;

            default:
                break;
        }
    }
    return -1;
}

int widget_handle_key(widget_t *widgets, int count, int *focus_idx, int ch) {
    /* Find focused input widget */
    int fi = *focus_idx;
    if (fi < 0 || fi >= count) return 0;
    widget_t *w = &widgets[fi];
    if (w->type != WIDGET_INPUT || !(w->flags & WFLAG_FOCUSED))
        return 0;

    int vlen = strlen(w->value);

    if (ch == 8 || ch == 127) {
        /* Backspace */
        if (w->cursor_pos > 0 && vlen > 0) {
            memmove(w->value + w->cursor_pos - 1,
                    w->value + w->cursor_pos,
                    vlen - w->cursor_pos + 1);
            w->cursor_pos--;
        }
        return 1;
    }

    if (ch == '\t' || ch == 9) {
        /* Tab: move focus to next input */
        for (int j = 1; j <= count; j++) {
            int ni = (fi + j) % count;
            if (widgets[ni].type == WIDGET_INPUT &&
                !(widgets[ni].flags & WFLAG_DISABLED)) {
                w->flags &= ~WFLAG_FOCUSED;
                widgets[ni].flags |= WFLAG_FOCUSED;
                *focus_idx = ni;
                return 1;
            }
        }
        return 1;
    }

    if (ch == '\n' || ch == 10 || ch == 13) {
        /* Enter: not consumed here — let dialog handle it */
        return 0;
    }

    /* Printable character */
    if (ch >= 32 && ch < 127 && vlen < WIDGET_VALUE_MAX - 1) {
        memmove(w->value + w->cursor_pos + 1,
                w->value + w->cursor_pos,
                vlen - w->cursor_pos + 1);
        w->value[w->cursor_pos] = (char)ch;
        w->cursor_pos++;
        return 1;
    }

    return 0;
}

/* ========================================================================= */
/* Dialog system                                                             */
/* ========================================================================= */

/* External keyboard functions from keyboard.asm */
extern int keyboard_has_input(void);
extern int keyboard_getchar(void);
extern void system_poll(void);

/* Dialog state */
static volatile int dlg_result;
static int dlg_win_id = -1;

/* Poll loop: process keyboard + mouse until dialog resolves */
static void dialog_poll_loop(widget_t *widgets, int wcount, int *focus_idx) {
    dlg_result = DIALOG_PENDING;

    while (dlg_result == DIALOG_PENDING) {
        /* Process keyboard */
        if (keyboard_has_input()) {
            int ch = keyboard_getchar();

            /* Enter => click primary/OK button */
            if (ch == '\n' || ch == 10 || ch == 13) {
                for (int i = 0; i < wcount; i++) {
                    if (widgets[i].type == WIDGET_BUTTON &&
                        (widgets[i].flags & WFLAG_PRIMARY)) {
                        dlg_result = widgets[i].id;
                        break;
                    }
                }
                if (dlg_result != DIALOG_PENDING) break;
            }

            /* Escape => cancel */
            if (ch == 27) {
                dlg_result = DIALOG_CANCEL;
                break;
            }

            /* Route to focused input widget */
            if (widget_handle_key(widgets, wcount, focus_idx, ch)) {
                /* Mark window dirty for repaint */
                if (dlg_win_id >= 0) {
                    win_set_dirty(dlg_win_id);
                }
            }
        }

        /* Process mouse & window events */
        win_poll();

        /* Check if dialog window was closed via close button */
        if (dlg_win_id >= 0 && !win_is_active(dlg_win_id)) {
            dlg_result = DIALOG_CANCEL;
            break;
        }

        /* Check if a button was clicked via win_get_clicked_widget */
        int clicked = win_get_clicked_widget(dlg_win_id);
        if (clicked >= 0) {
            dlg_result = clicked;
            break;
        }

        /* Yield CPU */
        __asm__ volatile("hlt");
    }
}

/* Helper: parse comma-separated string into items */
static int parse_choices(const char *csv, char items[][48], int max_items) {
    int count = 0;
    int ci = 0;
    for (int i = 0; csv[i] && count < max_items; i++) {
        if (csv[i] == ',') {
            items[count][ci] = '\0';
            count++;
            ci = 0;
        } else if (ci < 47) {
            items[count][ci++] = csv[i];
        }
    }
    if (ci > 0) {
        items[count][ci] = '\0';
        count++;
    }
    return count;
}

/* ── dialog_input ── */
int dialog_input(const char *title, const char *prompt,
                 const char *placeholder, char *value_out, int max) {
    int dw = 380, dh = 160;
    int sx = (fb_get_width() - dw) / 2;
    int sy = (fb_get_height() - dh) / 2;

    dlg_win_id = win_create(title, sx, sy, dw, dh, WIN_CLOSABLE | WIN_VISIBLE);
    if (dlg_win_id < 0) return DIALOG_CANCEL;

    /* Build widgets */
    widget_t widgets[4];
    memset(widgets, 0, sizeof(widgets));
    int wc = 0;

    /* Label: prompt */
    widgets[wc].type = WIDGET_LABEL;
    widgets[wc].rx = 10; widgets[wc].ry = 8;
    widgets[wc].rw = dw - 30; widgets[wc].rh = FONT_H;
    strncpy(widgets[wc].label, prompt, WIDGET_LABEL_MAX - 1);
    widgets[wc].id = 10;
    wc++;

    /* Input field */
    widgets[wc].type = WIDGET_INPUT;
    widgets[wc].rx = 10; widgets[wc].ry = 32;
    widgets[wc].rw = dw - 30; widgets[wc].rh = FONT_H + 10;
    if (placeholder) strncpy(widgets[wc].label, placeholder, WIDGET_LABEL_MAX - 1);
    widgets[wc].flags = WFLAG_FOCUSED;
    widgets[wc].id = 11;
    wc++;

    /* OK button */
    widgets[wc].type = WIDGET_BUTTON;
    widgets[wc].rx = dw - 30 - 80; widgets[wc].ry = 70;
    widgets[wc].rw = 80; widgets[wc].rh = 28;
    strncpy(widgets[wc].label, "OK", WIDGET_LABEL_MAX - 1);
    widgets[wc].flags = WFLAG_PRIMARY;
    widgets[wc].id = DIALOG_OK;
    wc++;

    /* Cancel button */
    widgets[wc].type = WIDGET_BUTTON;
    widgets[wc].rx = dw - 30 - 80 - 90; widgets[wc].ry = 70;
    widgets[wc].rw = 80; widgets[wc].rh = 28;
    strncpy(widgets[wc].label, "Cancel", WIDGET_LABEL_MAX - 1);
    widgets[wc].id = DIALOG_CANCEL;
    wc++;

    win_set_widgets(dlg_win_id, widgets, wc);

    int focus = 1; /* input field */
    dialog_poll_loop(widgets, wc, &focus);

    /* Copy result */
    int result = (dlg_result == DIALOG_OK) ? DIALOG_OK : DIALOG_CANCEL;
    if (result == DIALOG_OK && value_out) {
        strncpy(value_out, widgets[1].value, max - 1);
        value_out[max - 1] = '\0';
    }

    if (win_is_active(dlg_win_id))
        win_close(dlg_win_id);
    dlg_win_id = -1;
    return result;
}

/* ── dialog_confirm ── */
int dialog_confirm(const char *title, const char *prompt,
                   const char *yes_label, const char *no_label) {
    int dw = 340, dh = 130;
    int sx = (fb_get_width() - dw) / 2;
    int sy = (fb_get_height() - dh) / 2;

    dlg_win_id = win_create(title, sx, sy, dw, dh, WIN_CLOSABLE | WIN_VISIBLE);
    if (dlg_win_id < 0) return DIALOG_CANCEL;

    widget_t widgets[3];
    memset(widgets, 0, sizeof(widgets));
    int wc = 0;

    /* Label */
    widgets[wc].type = WIDGET_LABEL;
    widgets[wc].rx = 10; widgets[wc].ry = 10;
    widgets[wc].rw = dw - 30; widgets[wc].rh = FONT_H;
    strncpy(widgets[wc].label, prompt, WIDGET_LABEL_MAX - 1);
    widgets[wc].id = 10;
    wc++;

    /* Yes button */
    widgets[wc].type = WIDGET_BUTTON;
    widgets[wc].rx = dw - 30 - 80; widgets[wc].ry = 50;
    widgets[wc].rw = 80; widgets[wc].rh = 28;
    strncpy(widgets[wc].label, yes_label ? yes_label : "Yes", WIDGET_LABEL_MAX - 1);
    widgets[wc].flags = WFLAG_PRIMARY;
    widgets[wc].id = DIALOG_OK;
    wc++;

    /* No button */
    widgets[wc].type = WIDGET_BUTTON;
    widgets[wc].rx = dw - 30 - 80 - 90; widgets[wc].ry = 50;
    widgets[wc].rw = 80; widgets[wc].rh = 28;
    strncpy(widgets[wc].label, no_label ? no_label : "No", WIDGET_LABEL_MAX - 1);
    widgets[wc].id = DIALOG_CANCEL;
    wc++;

    win_set_widgets(dlg_win_id, widgets, wc);

    int focus = -1;
    dialog_poll_loop(widgets, wc, &focus);

    int result = (dlg_result == DIALOG_OK) ? DIALOG_OK : DIALOG_CANCEL;
    if (win_is_active(dlg_win_id))
        win_close(dlg_win_id);
    dlg_win_id = -1;
    return result;
}

/* ── dialog_choice ── */
int dialog_choice(const char *title, const char *prompt,
                  const char *choices, int default_idx) {
    char items[8][48];
    int nitems = parse_choices(choices, items, 8);
    if (nitems == 0) return -1;

    int dw = 360;
    int dh = 80 + nitems * 24 + 40;
    if (dh > 450) dh = 450;
    int sx = (fb_get_width() - dw) / 2;
    int sy = (fb_get_height() - dh) / 2;

    dlg_win_id = win_create(title, sx, sy, dw, dh, WIN_CLOSABLE | WIN_VISIBLE);
    if (dlg_win_id < 0) return -1;

    /* 1 label + N radios + 2 buttons */
    int max_w = 1 + nitems + 2;
    if (max_w > WIN_MAX_WIDGETS) max_w = WIN_MAX_WIDGETS;

    widget_t widgets[WIN_MAX_WIDGETS];
    memset(widgets, 0, sizeof(widgets));
    int wc = 0;

    /* Label */
    widgets[wc].type = WIDGET_LABEL;
    widgets[wc].rx = 10; widgets[wc].ry = 8;
    widgets[wc].rw = dw - 30; widgets[wc].rh = FONT_H;
    strncpy(widgets[wc].label, prompt, WIDGET_LABEL_MAX - 1);
    widgets[wc].id = 100;
    wc++;

    /* Radio buttons */
    for (int i = 0; i < nitems && wc < max_w - 2; i++) {
        widgets[wc].type = WIDGET_RADIO;
        widgets[wc].rx = 16; widgets[wc].ry = 32 + i * 24;
        widgets[wc].rw = dw - 46; widgets[wc].rh = 20;
        strncpy(widgets[wc].label, items[i], WIDGET_LABEL_MAX - 1);
        widgets[wc].group = 1;
        widgets[wc].state = (i == default_idx) ? 1 : 0;
        widgets[wc].id = 200 + i;
        wc++;
    }

    /* OK button */
    int btn_y = 32 + nitems * 24 + 8;
    widgets[wc].type = WIDGET_BUTTON;
    widgets[wc].rx = dw - 30 - 80; widgets[wc].ry = btn_y;
    widgets[wc].rw = 80; widgets[wc].rh = 28;
    strncpy(widgets[wc].label, "OK", WIDGET_LABEL_MAX - 1);
    widgets[wc].flags = WFLAG_PRIMARY;
    widgets[wc].id = DIALOG_OK;
    wc++;

    /* Cancel button */
    widgets[wc].type = WIDGET_BUTTON;
    widgets[wc].rx = dw - 30 - 80 - 90; widgets[wc].ry = btn_y;
    widgets[wc].rw = 80; widgets[wc].rh = 28;
    strncpy(widgets[wc].label, "Cancel", WIDGET_LABEL_MAX - 1);
    widgets[wc].id = DIALOG_CANCEL;
    wc++;

    win_set_widgets(dlg_win_id, widgets, wc);

    int focus = -1;
    dialog_poll_loop(widgets, wc, &focus);

    int result = -1;
    if (dlg_result == DIALOG_OK) {
        /* Find selected radio */
        for (int i = 0; i < wc; i++) {
            if (widgets[i].type == WIDGET_RADIO && widgets[i].state)
                result = widgets[i].id - 200;
        }
    }

    if (win_is_active(dlg_win_id))
        win_close(dlg_win_id);
    dlg_win_id = -1;
    return result;
}

/* ── dialog_notify ── */
void dialog_notify(const char *title, const char *message, int type) {
    const ui_theme_t *t = theme_get();
    (void)t;

    int dw = 340, dh = 120;
    int sx = (fb_get_width() - dw) / 2;
    int sy = (fb_get_height() - dh) / 2;

    dlg_win_id = win_create(title, sx, sy, dw, dh, WIN_CLOSABLE | WIN_VISIBLE);
    if (dlg_win_id < 0) return;

    widget_t widgets[3];
    memset(widgets, 0, sizeof(widgets));
    int wc = 0;

    /* Status indicator */
    static const char *icons[] = { "[i]", "[OK]", "[!]", "[X]" };
    if (type < 0 || type > 3) type = 0;
    widgets[wc].type = WIDGET_LABEL;
    widgets[wc].rx = 10; widgets[wc].ry = 10;
    widgets[wc].rw = 40; widgets[wc].rh = FONT_H;
    strncpy(widgets[wc].label, icons[type], WIDGET_LABEL_MAX - 1);
    widgets[wc].id = 50;
    wc++;

    /* Message */
    widgets[wc].type = WIDGET_LABEL;
    widgets[wc].rx = 40; widgets[wc].ry = 10;
    widgets[wc].rw = dw - 60; widgets[wc].rh = FONT_H;
    strncpy(widgets[wc].label, message, WIDGET_LABEL_MAX - 1);
    widgets[wc].id = 51;
    wc++;

    /* OK button */
    widgets[wc].type = WIDGET_BUTTON;
    widgets[wc].rx = (dw - 4 - 80) / 2; widgets[wc].ry = 50;
    widgets[wc].rw = 80; widgets[wc].rh = 28;
    strncpy(widgets[wc].label, "OK", WIDGET_LABEL_MAX - 1);
    widgets[wc].flags = WFLAG_PRIMARY;
    widgets[wc].id = DIALOG_OK;
    wc++;

    win_set_widgets(dlg_win_id, widgets, wc);

    int focus = -1;
    dialog_poll_loop(widgets, wc, &focus);

    if (win_is_active(dlg_win_id))
        win_close(dlg_win_id);
    dlg_win_id = -1;
}
