#ifndef AIOS_FRAMEBUFFER_H
#define AIOS_FRAMEBUFFER_H

#include "include/types.h"

/* Initialize framebuffer from multiboot info structure.
   Must be called AFTER paging_init (needs PSE for mapping). */
void fb_init(uint32_t *mbi);

/* Text console operations — same semantics as old VGA driver */
void fb_clear(void);
void fb_putchar(int ch);
void fb_print(const char *str);
void fb_newline(void);
void fb_set_color(int attr);    /* VGA-style attribute: (bg << 4) | fg */

/* Scrollback navigation */
void fb_page_up(void);
void fb_page_down(void);

/* Cursor queries */
int fb_get_cursor_row(void);
int fb_get_cursor_col(void);

/* Framebuffer info queries */
int  fb_is_ready(void);
int  fb_get_width(void);
int  fb_get_height(void);
int  fb_get_cols(void);
int  fb_get_rows(void);

/* Graphics primitives for window manager */
void     fb_fill_rect(int x, int y, int w, int h, uint32_t color);
void     fb_draw_char_xy(int px, int py, uint8_t ch, uint32_t fg, uint32_t bg);
void     fb_draw_text_xy(int px, int py, const char *text, uint32_t fg, uint32_t bg);
void     fb_blit(int x, int y, int w, int h, const uint32_t *pixels);
void     fb_render_console(void);
uint8_t *fb_get_addr(void);
uint32_t fb_get_pitch(void);

/* Double buffering — use for flicker-free window rendering */
void     fb_begin_frame(void);   /* redirect rendering to back buffer */
void     fb_end_frame(void);     /* flip back buffer to screen */
uint8_t *fb_get_target(void);    /* current render target */

#endif
