/* AiOS — ARM Framebuffer Driver (Raspberry Pi)
   Uses the VideoCore mailbox to allocate a framebuffer and renders
   text using the same CP437 bitmap font as the x86 version. */

#include "include/types.h"
#include "include/string.h"

/* Mailbox functions (from mailbox.c) */
extern uint32_t mailbox_alloc_framebuffer(int width, int height, int depth,
                                           uint32_t *pitch, uint32_t *fb_size);

/* Console state */
static uint32_t *fb_addr = 0;
static uint32_t fb_width, fb_height;
static uint32_t fb_pitch;
static uint32_t fb_size;
static int cursor_x, cursor_y;
static int ready = 0;

/* Minimal 8x16 font (subset — full CP437 would be shared with x86) */
extern const uint8_t font_8x16[256][16];

#define FONT_W 8
#define FONT_H 16
#define FG_COLOR 0xFFCCCCCC
#define BG_COLOR 0xFF000000

void fb_arm_init(void) {
    fb_width = 1024;
    fb_height = 768;

    uint32_t addr = mailbox_alloc_framebuffer(fb_width, fb_height, 32,
                                               &fb_pitch, &fb_size);
    if (addr == 0) return;

    fb_addr = (uint32_t *)(uintptr_t)addr;
    cursor_x = 0;
    cursor_y = 0;

    /* Clear to black */
    uint32_t total = fb_width * fb_height;
    for (uint32_t i = 0; i < total; i++)
        fb_addr[i] = BG_COLOR;

    ready = 1;
}

static void draw_char(int px, int py, uint8_t ch, uint32_t fg, uint32_t bg) {
    if (!ready) return;
    const uint8_t *glyph = font_8x16[ch];
    for (int y = 0; y < FONT_H; y++) {
        int sy = py + y;
        if (sy < 0 || sy >= (int)fb_height) continue;
        uint32_t *row = (uint32_t *)((uint8_t *)fb_addr + sy * fb_pitch);
        uint8_t bits = glyph[y];
        for (int x = 0; x < FONT_W; x++) {
            int sx = px + x;
            if (sx < 0 || sx >= (int)fb_width) continue;
            row[sx] = (bits & (0x80 >> x)) ? fg : bg;
        }
    }
}

static void scroll_up(void) {
    int rows_to_copy = fb_height - FONT_H;
    uint8_t *dst = (uint8_t *)fb_addr;
    uint8_t *src = dst + FONT_H * fb_pitch;
    memcpy(dst, src, rows_to_copy * fb_pitch);
    /* Clear last line */
    uint8_t *last = dst + rows_to_copy * fb_pitch;
    memset(last, 0, FONT_H * fb_pitch);
}

void fb_arm_putc(char c) {
    if (!ready) return;

    int cols = fb_width / FONT_W;
    int rows = fb_height / FONT_H;

    if (c == '\n') {
        cursor_x = 0;
        cursor_y++;
    } else if (c == '\r') {
        cursor_x = 0;
    } else if (c == '\b') {
        if (cursor_x > 0) cursor_x--;
    } else {
        draw_char(cursor_x * FONT_W, cursor_y * FONT_H,
                  (uint8_t)c, FG_COLOR, BG_COLOR);
        cursor_x++;
    }

    if (cursor_x >= cols) {
        cursor_x = 0;
        cursor_y++;
    }
    if (cursor_y >= rows) {
        scroll_up();
        cursor_y = rows - 1;
    }
}

void fb_arm_puts(const char *s) {
    while (*s) fb_arm_putc(*s++);
}

int fb_arm_is_ready(void) { return ready; }
uint32_t fb_arm_get_width(void) { return fb_width; }
uint32_t fb_arm_get_height(void) { return fb_height; }
