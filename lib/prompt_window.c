/* AiOS — Prompt Window
   Renders the AI prompt in a movable, resizable window.
   All prompt text output goes through this module instead of raw console. */

#include "include/prompt_window.h"
#include "include/window.h"
#include "include/framebuffer.h"
#include "include/string.h"
#include "include/stdio.h"

static int pw_id = -1;    /* Window ID for the prompt */
static int pw_color = 0x07;  /* Current VGA color attribute (unused for now) */

void prompt_win_init(void) {
    int sw = fb_get_width();
    int sh = fb_get_height();

    /* Nearly maximized: 40px margin on each side */
    int margin = 40;
    int wx = margin;
    int wy = margin;
    int ww = sw - 2 * margin;
    int wh = sh - 2 * margin;

    pw_id = win_create("AiOS Prompt", wx, wy, ww, wh,
                       WIN_CLOSABLE | WIN_RESIZABLE | WIN_SCROLLABLE | WIN_VISIBLE);
}

void prompt_win_print(const char *str) {
    if (pw_id < 0 || !str) return;

    /* Check if window was closed — recreate it */
    if (!win_is_active(pw_id)) {
        prompt_win_init();
        if (pw_id < 0) return;
    }

    win_append_text(pw_id, str);
}

void prompt_win_putchar(int ch) {
    if (pw_id < 0) return;
    if (!win_is_active(pw_id)) {
        prompt_win_init();
        if (pw_id < 0) return;
    }

    if (ch == 8) {
        /* Backspace — remove last character from window text */
        /* Access window internals via extern */
        extern window_t *win_get_ptr(int id);
        window_t *win = win_get_ptr(pw_id);
        if (win && win->text && win->text_len > 0) {
            win->text_len--;
            win->text[win->text_len] = '\0';
            win->dirty = 1;
        }
        return;
    }

    char s[2] = { (char)ch, '\0' };
    win_append_text(pw_id, s);
}

void prompt_win_clear(void) {
    if (pw_id < 0) return;
    if (!win_is_active(pw_id)) return;

    extern window_t *win_get_ptr(int id);
    window_t *win = win_get_ptr(pw_id);
    if (win && win->text) {
        win->text[0] = '\0';
        win->text_len = 0;
        win->scroll_y = 0;
        win->dirty = 1;
    }
}

void prompt_win_set_color(int attr) {
    pw_color = attr;
    /* Color attributes are not directly used in text windows —
       the window uses theme colors. This is kept for API compatibility. */
}

void prompt_win_newline(void) {
    prompt_win_putchar('\n');
}

void prompt_win_print_dec(int n) {
    char buf[16];
    if (n < 0) {
        prompt_win_putchar('-');
        n = -n;
    }
    if (n == 0) {
        prompt_win_putchar('0');
        return;
    }
    int i = 0;
    while (n > 0 && i < 15) {
        buf[i++] = '0' + (n % 10);
        n /= 10;
    }
    while (i > 0) {
        prompt_win_putchar(buf[--i]);
    }
}

void prompt_win_print_hex(int n) {
    static const char hex[] = "0123456789ABCDEF";
    prompt_win_putchar('0');
    prompt_win_putchar('x');
    for (int i = 28; i >= 0; i -= 4) {
        prompt_win_putchar(hex[(n >> i) & 0xF]);
    }
}
