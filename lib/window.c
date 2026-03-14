/* AiOS — Window Manager
   Provides movable, resizable, scrollable, closable windows with widgets,
   rendered over the VESA framebuffer text console.
   Uses double buffering and theme system for flicker-free, themable UI. */

#include "include/window.h"
#include "include/theme.h"
#include "include/framebuffer.h"
#include "include/mouse.h"
#include "include/string.h"
#include "include/heap.h"

extern uint32_t sys_now(void);

/* Frame rate limiter — min ms between full repaints during drag/resize */
#define DRAG_FRAME_MS  25   /* ~40fps cap during drag */
static uint32_t last_render_time = 0;

/* Layout constants */
#define TITLE_H       20
#define BORDER_W      2
#define CLOSE_SZ      16
#define SCROLL_W      14
#define FONT_W        8
#define FONT_H        16
#define MIN_W         120
#define MIN_H         80

/* Window storage and z-order */
static window_t windows[WIN_MAX];
static int z_order[WIN_MAX];   /* indices into windows[], front = last */
static int z_count = 0;

/* Interaction state */
static int drag_id = -1, drag_ox, drag_oy;
static int resize_id = -1, rsz_sw, rsz_sh, rsz_mx, rsz_my;
static int prev_buttons = 0;
static int needs_repaint = 0;

/* Cursor bitmap (8x14 arrow) */
static const uint8_t cursor_shape[14] = {
    0x80, 0xC0, 0xE0, 0xF0, 0xF8, 0xFC, 0xFE,
    0xF0, 0xD8, 0x18, 0x0C, 0x0C, 0x00, 0x00
};
static const uint8_t cursor_mask[14] = {
    0xC0, 0xE0, 0xF0, 0xF8, 0xFC, 0xFE, 0xFF,
    0xF8, 0xFC, 0x3C, 0x1E, 0x1E, 0x00, 0x00
};
#define CUR_W 8
#define CUR_H 12

/* Cursor save/restore for efficient cursor-only redraws */
static uint32_t cursor_save_buf[CUR_W * CUR_H];
static int cursor_save_x = -1, cursor_save_y = -1;
static int cursor_saved = 0;

/* ========================================================================= */
/* Helpers                                                                    */
/* ========================================================================= */

static int count_lines(const char *text, int len, int cpl) {
    if (!text || len == 0 || cpl <= 0) return 0;
    int lines = 1, col = 0;
    for (int i = 0; i < len; i++) {
        if (text[i] == '\n') { lines++; col = 0; }
        else { col++; if (col >= cpl) { lines++; col = 0; } }
    }
    return lines;
}

/* ========================================================================= */
/* Window API                                                                 */
/* ========================================================================= */

int win_create(const char *title, int x, int y, int w, int h, int flags) {
    int id = -1;
    for (int i = 0; i < WIN_MAX; i++)
        if (!windows[i].active) { id = i; break; }
    if (id < 0) return -1;

    window_t *win = &windows[id];
    memset(win, 0, sizeof(*win));

    int tl = strlen(title);
    if (tl >= (int)sizeof(win->title)) tl = sizeof(win->title) - 1;
    memcpy(win->title, title, tl);
    win->title[tl] = '\0';

    win->x = x; win->y = y; win->w = w; win->h = h;
    win->flags = flags | WIN_VISIBLE;
    win->type = WIN_TEXT;
    win->active = 1;
    win->dirty = 1;
    win->focus_widget = -1;
    win->clicked_widget = -1;

    win->text_cap = 8192;
    win->text = malloc(win->text_cap);
    if (win->text) { win->text[0] = '\0'; win->text_len = 0; }

    z_order[z_count++] = id;
    win_focus(id);
    needs_repaint = 1;
    return id;
}

void win_close(int id) {
    if (id < 0 || id >= WIN_MAX || !windows[id].active) return;
    window_t *win = &windows[id];
    if (win->text) { free(win->text); win->text = NULL; }
    if (win->img)  { free(win->img);  win->img = NULL;  }
    win->active = 0;

    /* Remove from z-order */
    int found = 0;
    for (int i = 0; i < z_count; i++) {
        if (z_order[i] == id) found = 1;
        if (found && i + 1 < z_count) z_order[i] = z_order[i + 1];
    }
    if (found) z_count--;
    needs_repaint = 1;
}

void win_focus(int id) {
    if (id < 0 || id >= WIN_MAX || !windows[id].active) return;

    /* Move to top of z-order */
    int found = 0;
    for (int i = 0; i < z_count; i++) {
        if (z_order[i] == id) found = 1;
        if (found && i + 1 < z_count) z_order[i] = z_order[i + 1];
    }
    if (found || z_count > 0) z_order[z_count - 1] = id;

    for (int i = 0; i < WIN_MAX; i++) {
        if (!windows[i].active) continue;
        int old = windows[i].flags & WIN_FOCUSED;
        if (i == id) windows[i].flags |= WIN_FOCUSED;
        else         windows[i].flags &= ~WIN_FOCUSED;
        if ((windows[i].flags & WIN_FOCUSED) != old) windows[i].dirty = 1;
    }
    needs_repaint = 1;
}

void win_append_text(int id, const char *text) {
    if (id < 0 || id >= WIN_MAX || !windows[id].active) return;
    window_t *win = &windows[id];
    if (!win->text) return;

    int add = strlen(text);
    while (win->text_len + add >= win->text_cap) {
        if (win->text_cap < 65536) {
            int nc = win->text_cap * 2;
            char *nb = malloc(nc);
            if (nb) {
                memcpy(nb, win->text, win->text_len + 1);
                free(win->text);
                win->text = nb;
                win->text_cap = nc;
            } else break;
        } else {
            int keep = win->text_cap / 2;
            memmove(win->text, win->text + win->text_len - keep, keep);
            win->text_len = keep;
            break;
        }
    }

    if (win->text_len + add < win->text_cap) {
        memcpy(win->text + win->text_len, text, add);
        win->text_len += add;
        win->text[win->text_len] = '\0';
    }

    win->scroll_y = 0x7FFFFFFF;
    win->dirty = 1;
    needs_repaint = 1;
}

void win_set_image(int id, const uint32_t *pixels, int w, int h) {
    if (id < 0 || id >= WIN_MAX || !windows[id].active) return;
    window_t *win = &windows[id];
    win->type = WIN_IMAGE;
    if (win->img) free(win->img);
    win->img_w = w; win->img_h = h;
    win->img = malloc(w * h * 4);
    if (win->img) memcpy(win->img, pixels, w * h * 4);
    win->dirty = 1;
    needs_repaint = 1;
}

void win_set_widgets(int id, const widget_t *widgets, int count) {
    if (id < 0 || id >= WIN_MAX || !windows[id].active) return;
    window_t *win = &windows[id];
    if (count > WIN_MAX_WIDGETS) count = WIN_MAX_WIDGETS;
    memcpy(win->widgets, widgets, count * sizeof(widget_t));
    win->widget_count = count;
    win->focus_widget = -1;
    win->clicked_widget = -1;
    /* Find first input widget for focus */
    for (int i = 0; i < count; i++) {
        if (widgets[i].type == WIDGET_INPUT && (widgets[i].flags & WFLAG_FOCUSED)) {
            win->focus_widget = i;
            break;
        }
    }
    win->dirty = 1;
    needs_repaint = 1;
}

void win_set_dirty(int id) {
    if (id < 0 || id >= WIN_MAX || !windows[id].active) return;
    windows[id].dirty = 1;
    needs_repaint = 1;
}

int win_is_active(int id) {
    if (id < 0 || id >= WIN_MAX) return 0;
    return windows[id].active;
}

int win_get_clicked_widget(int id) {
    if (id < 0 || id >= WIN_MAX || !windows[id].active) return -1;
    int c = windows[id].clicked_widget;
    windows[id].clicked_widget = -1;
    return c;
}

/* ========================================================================= */
/* Rendering                                                                  */
/* ========================================================================= */

static void draw_window(window_t *win) {
    const ui_theme_t *t = theme_get();
    int x = win->x, y = win->y, w = win->w, h = win->h;
    int focused = win->flags & WIN_FOCUSED;

    /* Border */
    fb_fill_rect(x, y, w, BORDER_W, t->win_border);
    fb_fill_rect(x, y + h - BORDER_W, w, BORDER_W, t->win_border);
    fb_fill_rect(x, y, BORDER_W, h, t->win_border);
    fb_fill_rect(x + w - BORDER_W, y, BORDER_W, h, t->win_border);

    /* Title bar */
    uint32_t tc = focused ? t->win_title_active : t->win_title_inactive;
    fb_fill_rect(x + BORDER_W, y + BORDER_W, w - 2 * BORDER_W, TITLE_H, tc);
    fb_draw_text_xy(x + BORDER_W + 6, y + BORDER_W + 2, win->title, t->win_title_text, tc);

    /* Close button */
    if (win->flags & WIN_CLOSABLE) {
        int bx = x + w - BORDER_W - CLOSE_SZ - 2;
        int by = y + BORDER_W + (TITLE_H - CLOSE_SZ) / 2;
        fb_fill_rect(bx, by, CLOSE_SZ, CLOSE_SZ, t->close_bg);
        for (int d = 3; d < CLOSE_SZ - 3; d++) {
            fb_fill_rect(bx + d, by + d, 2, 1, t->close_icon);
            fb_fill_rect(bx + CLOSE_SZ - d - 2, by + d, 2, 1, t->close_icon);
        }
    }

    /* Content area dimensions */
    int cx = x + BORDER_W;
    int cy = y + BORDER_W + TITLE_H;
    int cw = w - 2 * BORDER_W;
    int ch = h - 2 * BORDER_W - TITLE_H;
    if (ch <= 0) return;

    int has_sb = (win->flags & WIN_SCROLLABLE) && win->type == WIN_TEXT;
    int tw = has_sb ? cw - SCROLL_W : cw;

    /* Content background */
    fb_fill_rect(cx, cy, cw, ch, t->win_bg);

    /* Render widgets if present */
    if (win->widget_count > 0) {
        widget_render(win->widgets, win->widget_count, cx, cy);
    }
    else if (win->type == WIN_TEXT && win->text && win->text_len > 0) {
        int cpl = tw / FONT_W;
        if (cpl < 1) cpl = 1;
        int vis = ch / FONT_H;
        int total = count_lines(win->text, win->text_len, cpl);

        int max_sc = total - vis;
        if (max_sc < 0) max_sc = 0;
        if (win->scroll_y > max_sc) win->scroll_y = max_sc;
        if (win->scroll_y < 0) win->scroll_y = 0;

        int line = 0, col = 0, draw_y = 0;
        for (int i = 0; i < win->text_len && draw_y < vis; i++) {
            char c = win->text[i];
            if (c == '\n') {
                line++; col = 0;
                if (line > win->scroll_y) draw_y++;
                continue;
            }
            if (line >= win->scroll_y && draw_y < vis && col < cpl) {
                fb_draw_char_xy(cx + col * FONT_W, cy + draw_y * FONT_H,
                                (uint8_t)c, t->win_text, t->win_bg);
            }
            col++;
            if (col >= cpl) {
                line++; col = 0;
                if (line > win->scroll_y) draw_y++;
            }
        }

        if (has_sb && total > vis) {
            int sx = cx + tw;
            fb_fill_rect(sx, cy, SCROLL_W, ch, t->scroll_track);
            int th = ch * vis / total;
            if (th < 20) th = 20;
            int ty = cy;
            if (max_sc > 0) ty = cy + (ch - th) * win->scroll_y / max_sc;
            fb_fill_rect(sx + 2, ty, SCROLL_W - 4, th, t->scroll_thumb);
        }
    } else if (win->type == WIN_IMAGE && win->img) {
        int ix = cx + (tw - win->img_w) / 2;
        int iy = cy + (ch - win->img_h) / 2;
        if (ix < cx) ix = cx;
        if (iy < cy) iy = cy;
        fb_blit(ix, iy, win->img_w, win->img_h, win->img);
    }

    /* Resize grip */
    if (win->flags & WIN_RESIZABLE) {
        for (int i = 0; i < 3; i++)
            fb_fill_rect(x + w - 10 + i * 3, y + h - 10 + (2 - i) * 3,
                         2, 2 + i * 3, t->grip);
    }
}

/* Save pixels under cursor position from the current render target */
static void save_under_cursor(int mx, int my) {
    uint8_t *buf = fb_get_target();
    uint32_t pitch = fb_get_pitch();
    int sw = fb_get_width(), sh = fb_get_height();

    for (int y = 0; y < CUR_H; y++) {
        int sy = my + y;
        if (sy < 0 || sy >= sh) {
            for (int x = 0; x < CUR_W; x++)
                cursor_save_buf[y * CUR_W + x] = 0;
            continue;
        }
        uint32_t *row = (uint32_t *)(buf + sy * pitch);
        for (int x = 0; x < CUR_W; x++) {
            int sx = mx + x;
            if (sx < 0 || sx >= sw)
                cursor_save_buf[y * CUR_W + x] = 0;
            else
                cursor_save_buf[y * CUR_W + x] = row[sx];
        }
    }
    cursor_save_x = mx;
    cursor_save_y = my;
    cursor_saved = 1;
}

/* Restore pixels under cursor on the current render target */
static void restore_under_cursor(void) {
    if (!cursor_saved) return;
    uint8_t *buf = fb_get_target();
    uint32_t pitch = fb_get_pitch();
    int sw = fb_get_width(), sh = fb_get_height();

    for (int y = 0; y < CUR_H; y++) {
        int sy = cursor_save_y + y;
        if (sy < 0 || sy >= sh) continue;
        uint32_t *row = (uint32_t *)(buf + sy * pitch);
        for (int x = 0; x < CUR_W; x++) {
            int sx = cursor_save_x + x;
            if (sx < 0 || sx >= sw) continue;
            row[sx] = cursor_save_buf[y * CUR_W + x];
        }
    }
    cursor_saved = 0;
}

static void draw_cursor(int mx, int my) {
    const ui_theme_t *t = theme_get();
    uint8_t *buf = fb_get_target();
    uint32_t pitch = fb_get_pitch();
    int sw = fb_get_width(), sh = fb_get_height();

    for (int y = 0; y < CUR_H; y++) {
        int sy = my + y;
        if (sy < 0 || sy >= sh) continue;
        uint32_t *row = (uint32_t *)(buf + sy * pitch);
        uint8_t mask = cursor_mask[y];
        uint8_t bits = cursor_shape[y];
        for (int x = 0; x < CUR_W; x++) {
            int sx = mx + x;
            if (sx < 0 || sx >= sw) continue;
            if (mask & (0x80 >> x))
                row[sx] = (bits & (0x80 >> x)) ? t->cursor_fg : t->cursor_bg;
        }
    }
}

void win_render_all(void) {
    fb_begin_frame();

    fb_render_console();

    for (int i = 0; i < z_count; i++) {
        int id = z_order[i];
        if (id >= 0 && id < WIN_MAX && windows[id].active &&
            (windows[id].flags & WIN_VISIBLE))
            draw_window(&windows[id]);
    }

    int mx = mouse_get_x(), my = mouse_get_y();
    if (mouse_is_installed()) {
        save_under_cursor(mx, my);
        draw_cursor(mx, my);
    }

    fb_end_frame();

    needs_repaint = 0;
    for (int i = 0; i < WIN_MAX; i++)
        windows[i].dirty = 0;
}

static void redraw_cursor_only(void) {
    fb_begin_frame();
    restore_under_cursor();
    int mx = mouse_get_x(), my = mouse_get_y();
    save_under_cursor(mx, my);
    draw_cursor(mx, my);
    fb_end_frame();
}

/* ========================================================================= */
/* Mouse interaction                                                          */
/* ========================================================================= */

static int hit_test(int mx, int my) {
    for (int i = z_count - 1; i >= 0; i--) {
        int id = z_order[i];
        window_t *w = &windows[id];
        if (!w->active || !(w->flags & WIN_VISIBLE)) continue;
        if (mx >= w->x && mx < w->x + w->w &&
            my >= w->y && my < w->y + w->h)
            return id;
    }
    return -1;
}

static int in_close(window_t *w, int mx, int my) {
    if (!(w->flags & WIN_CLOSABLE)) return 0;
    int bx = w->x + w->w - BORDER_W - CLOSE_SZ - 2;
    int by = w->y + BORDER_W + (TITLE_H - CLOSE_SZ) / 2;
    return mx >= bx && mx < bx + CLOSE_SZ && my >= by && my < by + CLOSE_SZ;
}

static int in_title(window_t *w, int mx, int my) {
    return mx >= w->x + BORDER_W && mx < w->x + w->w - BORDER_W &&
           my >= w->y + BORDER_W && my < w->y + BORDER_W + TITLE_H &&
           !in_close(w, mx, my);
}

static int in_grip(window_t *w, int mx, int my) {
    if (!(w->flags & WIN_RESIZABLE)) return 0;
    return mx >= w->x + w->w - 16 && mx < w->x + w->w &&
           my >= w->y + w->h - 16 && my < w->y + w->h;
}

void win_poll(void) {
    if (!mouse_is_installed()) return;

    int mouse_moved = mouse_poll_changed();
    if (!mouse_moved && !needs_repaint) return;

    int mx = mouse_get_x();
    int my = mouse_get_y();
    int mb = mouse_get_buttons();
    int left_down = (mb & 1) && !(prev_buttons & 1);
    int left_held = mb & 1;
    int left_up   = !(mb & 1) && (prev_buttons & 1);

    /* Handle drag */
    if (drag_id >= 0) {
        if (left_held) {
            window_t *w = &windows[drag_id];
            w->x = mx - drag_ox;
            w->y = my - drag_oy;
            if (w->y < 0) w->y = 0;
            needs_repaint = 1;
        }
        if (left_up) {
            drag_id = -1;
            needs_repaint = 1;
            /* Force final repaint at release */
            last_render_time = 0;
        }
        prev_buttons = mb;
        if (needs_repaint) {
            uint32_t now = sys_now();
            if (now - last_render_time >= DRAG_FRAME_MS || !left_held) {
                win_render_all();
                last_render_time = now;
            }
        } else if (mouse_moved) {
            redraw_cursor_only();
        }
        return;
    }

    /* Handle resize */
    if (resize_id >= 0) {
        if (left_held) {
            window_t *w = &windows[resize_id];
            int nw = rsz_sw + (mx - rsz_mx);
            int nh = rsz_sh + (my - rsz_my);
            if (nw < MIN_W) nw = MIN_W;
            if (nh < MIN_H) nh = MIN_H;
            w->w = nw; w->h = nh;
            needs_repaint = 1;
        }
        if (left_up) {
            resize_id = -1;
            needs_repaint = 1;
            last_render_time = 0;
        }
        prev_buttons = mb;
        if (needs_repaint) {
            uint32_t now = sys_now();
            if (now - last_render_time >= DRAG_FRAME_MS || !left_held) {
                win_render_all();
                last_render_time = now;
            }
        }
        return;
    }

    /* New click */
    if (left_down) {
        int id = hit_test(mx, my);
        if (id >= 0) {
            window_t *w = &windows[id];
            win_focus(id);

            if (in_close(w, mx, my)) {
                win_close(id);
            } else if (in_title(w, mx, my)) {
                drag_id = id;
                drag_ox = mx - w->x;
                drag_oy = my - w->y;
            } else if (in_grip(w, mx, my)) {
                resize_id = id;
                rsz_sw = w->w; rsz_sh = w->h;
                rsz_mx = mx; rsz_my = my;
            } else {
                /* Content area: check widgets first, then scrollbar */
                int cx = w->x + BORDER_W;
                int cy = w->y + BORDER_W + TITLE_H;

                if (w->widget_count > 0) {
                    int wid = widget_handle_click(w->widgets, w->widget_count,
                                                   cx, cy, mx, my);
                    if (wid >= 0) {
                        w->clicked_widget = wid;
                        /* Update focus for input widgets */
                        for (int j = 0; j < w->widget_count; j++) {
                            if (w->widgets[j].id == wid &&
                                w->widgets[j].type == WIDGET_INPUT)
                                w->focus_widget = j;
                        }
                        needs_repaint = 1;
                    }
                }

                if ((w->flags & WIN_SCROLLABLE) && w->type == WIN_TEXT) {
                    int sb_x = w->x + BORDER_W + w->w - 2 * BORDER_W - SCROLL_W;
                    if (mx >= sb_x) {
                        int cy2 = w->y + BORDER_W + TITLE_H;
                        int ch = w->h - 2 * BORDER_W - TITLE_H;
                        int mid = cy2 + ch / 2;
                        int vis = ch / FONT_H;
                        if (my < mid) w->scroll_y -= vis;
                        else          w->scroll_y += vis;
                        if (w->scroll_y < 0) w->scroll_y = 0;
                        needs_repaint = 1;
                    }
                }
            }
        }
    }

    prev_buttons = mb;

    if (needs_repaint) {
        win_render_all();
    } else if (mouse_moved && win_any_visible()) {
        redraw_cursor_only();
    }
}

int win_any_visible(void) {
    for (int i = 0; i < WIN_MAX; i++)
        if (windows[i].active && (windows[i].flags & WIN_VISIBLE))
            return 1;
    return 0;
}

window_t *win_get_ptr(int id) {
    if (id < 0 || id >= WIN_MAX || !windows[id].active) return 0;
    return &windows[id];
}
