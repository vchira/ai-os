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

/* Cursor queries */
int fb_get_cursor_row(void);
int fb_get_cursor_col(void);

/* Framebuffer info queries */
int  fb_is_ready(void);
int  fb_get_width(void);
int  fb_get_height(void);
int  fb_get_cols(void);
int  fb_get_rows(void);

#endif
