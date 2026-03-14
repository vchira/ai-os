/* AiOS — VESA Framebuffer Driver
   Provides a text console on a 32-bit linear framebuffer set up by GRUB.
   Font is loaded from the VGA BIOS ROM (8x16 CP437).
   Supports double buffering for flicker-free rendering. */

#include "include/framebuffer.h"
#include "include/string.h"
#include "include/heap.h"

/* ========================================================================= */
/* Framebuffer state                                                         */
/* ========================================================================= */

static uint8_t  *fb_addr;       /* linear framebuffer base (hardware) */
static uint8_t  *fb_back;       /* back buffer for double buffering */
static uint8_t  *fb_console;    /* cached console rendering */
static int       con_dirty = 1; /* 1 = console needs re-render to cache */
static uint8_t  *render_buf;    /* current render target (fb_addr or fb_back) */
static uint32_t  fb_width;      /* pixels wide */
static uint32_t  fb_height;     /* pixels tall */
static uint32_t  fb_pitch;      /* bytes per scanline */
static uint32_t  fb_bpp;        /* bits per pixel */
static int       fb_ok = 0;     /* 1 = framebuffer operational */

/* ========================================================================= */
/* Text console state                                                        */
/* ========================================================================= */

#define FONT_W 8
#define FONT_H 16

static int      con_cols;       /* characters per row */
static int      con_rows;       /* rows of text */
static int      con_x;         /* current column (char) */
static int      con_y;         /* current row (char) */
static uint32_t con_fg;        /* foreground RGB */
static uint32_t con_bg;        /* background RGB */
static int      con_attr;     /* VGA attribute byte for ring buffer */

/* 8x16 bitmap font — 256 glyphs × 16 bytes each = 4096 bytes */
static uint8_t font_data[256 * FONT_H];

/* ========================================================================= */
/* Scrollback ring buffer                                                    */
/* ========================================================================= */

#define SCROLL_BUF_LINES 500
#define SCROLL_BUF_COLS  128
#define SCROLL_STEP      24     /* lines per Page Up/Down (half screen) */

typedef struct {
    uint8_t ch;
    uint8_t attr;
} scroll_cell_t;

static scroll_cell_t scroll_buf[SCROLL_BUF_LINES][SCROLL_BUF_COLS];
static int scroll_base;    /* ring index of top visible screen line */
static int scroll_total;   /* lines that have scrolled off the top */
static int scroll_view;    /* 0 = live view, >0 = lines scrolled back */

/* ========================================================================= */
/* VGA 16-color palette → 32-bit 0x00RRGGBB                                 */
/* ========================================================================= */

static const uint32_t vga_palette[16] = {
    0x000000,   /* 0: black        */
    0x0000AA,   /* 1: blue         */
    0x00AA00,   /* 2: green        */
    0x00AAAA,   /* 3: cyan         */
    0xAA0000,   /* 4: red          */
    0xAA00AA,   /* 5: magenta      */
    0xAA5500,   /* 6: brown        */
    0xAAAAAA,   /* 7: light gray   */
    0x555555,   /* 8: dark gray    */
    0x5555FF,   /* 9: light blue   */
    0x55FF55,   /* A: light green  */
    0x55FFFF,   /* B: light cyan   */
    0xFF5555,   /* C: light red    */
    0xFF55FF,   /* D: light magenta*/
    0xFFFF55,   /* E: yellow       */
    0xFFFFFF,   /* F: white        */
};

/* ========================================================================= */
/* Page directory (from paging.asm) — used to map framebuffer via PSE        */
/* ========================================================================= */

extern uint32_t page_directory[];

/* Map a physical address range into the page directory using 4MB PSE pages */
static void map_fb_pages(uint32_t phys, uint32_t size) {
    uint32_t start = phys & 0xFFC00000;                     /* align down to 4MB */
    uint32_t end   = (phys + size + 0x3FFFFF) & 0xFFC00000; /* align up to 4MB */

    for (uint32_t a = start; a < end; a += 0x400000) {
        int pde = a >> 22;
        /* PDE: base address | Present | R/W | PS (4MB page) */
        page_directory[pde] = a | 0x83;
    }

    /* Flush TLB — reload CR3 */
    __asm__ volatile(
        "mov %%cr3, %%eax \n"
        "mov %%eax, %%cr3 \n"
        ::: "eax"
    );
}

/* ========================================================================= */
/* Font loading                                                              */
/* ========================================================================= */

static void load_font(void) {
    /* Read INT 43h IVT entry at physical 0x10C.
       This points to the active 8x16 VGA font in the BIOS ROM.
       Format: [offset:16][segment:16] → linear = segment*16 + offset */
    uint16_t *ivt = (uint16_t *)0x10C;
    uint32_t seg = ivt[1];
    uint32_t off = ivt[0];
    uint32_t addr = seg * 16 + off;

    /* Validate: should point into VGA BIOS ROM (0xC0000-0xC7FFF)
       or system BIOS (0xF0000-0xFFFFF) */
    if (addr >= 0xC0000 && addr < 0x100000) {
        memcpy(font_data, (void *)addr, sizeof(font_data));
        return;
    }

    /* Fallback: generate crude but readable glyphs.
       This shouldn't happen in QEMU but ensures we always have text. */
    memset(font_data, 0, sizeof(font_data));

    /* Generate simple block letters for printable ASCII (32-126).
       Each character gets a centered 6×10 block with a unique pattern
       derived from its ASCII code — not beautiful but readable. */
    for (int ch = 33; ch < 127; ch++) {
        uint8_t *g = &font_data[ch * FONT_H];
        /* Top/bottom margins: rows 0-2, 13-15 blank */
        uint8_t pattern = (uint8_t)(ch * 37);  /* pseudo-unique pattern */
        g[3]  = 0x7E;           /* top bar */
        g[4]  = 0x42 | pattern;
        g[5]  = 0x42;
        g[6]  = 0x42 | (pattern >> 1);
        g[7]  = 0x7E;           /* middle bar */
        g[8]  = 0x42 | (pattern >> 2);
        g[9]  = 0x42;
        g[10] = 0x42 | (pattern >> 3);
        g[11] = 0x7E;           /* bottom bar */
        g[12] = 0x00;
    }
}

/* ========================================================================= */
/* Pixel-level rendering (all write to render_buf)                           */
/* ========================================================================= */

static inline void put_pixel(int x, int y, uint32_t color) {
    uint32_t *row = (uint32_t *)(render_buf + y * fb_pitch);
    row[x] = color;
}

static void fill_rect(int x, int y, int w, int h, uint32_t color) {
    int x2 = x + w; if (x2 > (int)fb_width) x2 = (int)fb_width;
    int y2 = y + h; if (y2 > (int)fb_height) y2 = (int)fb_height;
    if (x < 0) x = 0;
    if (y < 0) y = 0;
    int count = x2 - x;
    if (count <= 0 || y >= y2) return;

    /* Fill first row */
    uint32_t *first = (uint32_t *)(render_buf + y * fb_pitch) + x;
    for (int c = 0; c < count; c++)
        first[c] = color;

    /* Copy first row to subsequent rows (memcpy uses rep movsd = fast) */
    int bytes = count * 4;
    for (int r = y + 1; r < y2; r++) {
        uint32_t *row = (uint32_t *)(render_buf + r * fb_pitch) + x;
        memcpy(row, first, bytes);
    }
}

/* Render a single character at character grid position (col, row) */
static void render_char(int col, int row, uint8_t ch) {
    int px = col * FONT_W;
    int py = row * FONT_H;
    uint8_t *glyph = &font_data[ch * FONT_H];

    for (int y = 0; y < FONT_H; y++) {
        uint32_t *scanline = (uint32_t *)(render_buf + (py + y) * fb_pitch);
        uint8_t bits = glyph[y];
        for (int x = 0; x < FONT_W; x++) {
            scanline[px + x] = (bits & (0x80 >> x)) ? con_fg : con_bg;
        }
    }
}

/* ========================================================================= */
/* Console operations                                                        */
/* ========================================================================= */

static void scroll_up(void) {
    con_dirty = 1;
    /* Move all rows up by one text line (FONT_H pixel rows) */
    int line_bytes = FONT_H * fb_pitch;
    int move_bytes = (con_rows - 1) * line_bytes;
    memmove(render_buf, render_buf + line_bytes, move_bytes);

    /* Clear the last line */
    fill_rect(0, (con_rows - 1) * FONT_H, fb_width, FONT_H, con_bg);

    /* Advance ring buffer — old top line becomes history */
    scroll_base = (scroll_base + 1) % SCROLL_BUF_LINES;
    scroll_total++;

    /* Clear the new bottom line in the ring buffer */
    int new_bottom = (scroll_base + con_rows - 1) % SCROLL_BUF_LINES;
    for (int c = 0; c < con_cols; c++) {
        scroll_buf[new_bottom][c].ch = ' ';
        scroll_buf[new_bottom][c].attr = con_attr;
    }
}

static void advance_cursor(void) {
    con_x++;
    if (con_x >= con_cols) {
        con_x = 0;
        con_y++;
    }
    if (con_y >= con_rows) {
        scroll_up();
        con_y = con_rows - 1;
    }
}

/* ========================================================================= */
/* Public API                                                                */
/* ========================================================================= */

void fb_init(uint32_t *mbi) {
    if (!mbi) return;

    uint32_t flags = mbi[0];

    /* Multiboot flag bit 12 = framebuffer info present */
    if (!(flags & (1 << 12))) return;

    /* Extract framebuffer parameters from multiboot info.
       Offsets are byte-based; we index as u32 array: byte/4 = index.
       88/4=22, 96/4=24, 100/4=25, 104/4=26, bpp at byte 108. */
    uint32_t addr_lo = mbi[22];
    fb_pitch  = mbi[24];
    fb_width  = mbi[25];
    fb_height = mbi[26];
    fb_bpp    = ((uint8_t *)mbi)[108];

    if (fb_width == 0 || fb_height == 0 || fb_bpp != 32) return;

    /* Map framebuffer physical address into our page tables (PSE 4MB pages) */
    uint32_t fb_size = fb_height * fb_pitch;
    map_fb_pages(addr_lo, fb_size);

    fb_addr = (uint8_t *)(uintptr_t)addr_lo;
    render_buf = fb_addr;

    /* Allocate back buffer for double buffering */
    fb_back = malloc(fb_size);
    /* Allocate console cache for fast re-blit */
    fb_console = malloc(fb_size);
    /* If allocations fail, rendering goes directly to hardware */

    /* Load 8x16 VGA BIOS font */
    load_font();

    /* Set up text console */
    con_cols = fb_width / FONT_W;    /* 1024/8 = 128 columns */
    con_rows = fb_height / FONT_H;   /* 768/16 = 48 rows */
    con_x = 0;
    con_y = 0;
    con_fg = vga_palette[7];         /* default: light gray on black */
    con_bg = vga_palette[0];
    con_attr = 0x07;

    /* Initialize scroll buffer */
    scroll_base = 0;
    scroll_total = 0;
    scroll_view = 0;

    fb_ok = 1;

    /* Clear the screen */
    fb_clear();
}

void fb_clear(void) {
    if (!fb_ok) return;
    fill_rect(0, 0, fb_width, fb_height, con_bg);
    con_x = 0;
    con_y = 0;
    scroll_view = 0;
    con_dirty = 1;

    /* Clear visible lines in ring buffer */
    for (int r = 0; r < con_rows; r++) {
        int ring_line = (scroll_base + r) % SCROLL_BUF_LINES;
        for (int c = 0; c < con_cols; c++) {
            scroll_buf[ring_line][c].ch = ' ';
            scroll_buf[ring_line][c].attr = con_attr;
        }
    }
}

void fb_set_color(int attr) {
    if (!fb_ok) return;
    con_attr = attr;
    con_fg = vga_palette[attr & 0x0F];
    con_bg = vga_palette[(attr >> 4) & 0x0F];
}

/* Re-render the entire screen from the ring buffer */
static void render_from_buffer(void) {
    for (int r = 0; r < con_rows; r++) {
        int ring_line = (scroll_base - scroll_view + r + SCROLL_BUF_LINES) % SCROLL_BUF_LINES;
        for (int c = 0; c < con_cols; c++) {
            scroll_cell_t cell = scroll_buf[ring_line][c];
            uint32_t fg = vga_palette[cell.attr & 0x0F];
            uint32_t bg = vga_palette[(cell.attr >> 4) & 0x0F];
            /* Inline glyph render to avoid changing con_fg/con_bg */
            int px = c * FONT_W;
            int py = r * FONT_H;
            uint8_t *glyph = &font_data[cell.ch * FONT_H];
            for (int y = 0; y < FONT_H; y++) {
                uint32_t *scanline = (uint32_t *)(render_buf + (py + y) * fb_pitch);
                uint8_t bits = glyph[y];
                for (int x = 0; x < FONT_W; x++) {
                    scanline[px + x] = (bits & (0x80 >> x)) ? fg : bg;
                }
            }
        }
    }
}

void fb_putchar(int ch) {
    if (!fb_ok) return;
    con_dirty = 1;

    /* Any new output returns to live view */
    if (scroll_view > 0) {
        scroll_view = 0;
        render_from_buffer();
    }

    if (ch == '\n' || ch == 10) {
        con_x = 0;
        con_y++;
        if (con_y >= con_rows) {
            scroll_up();
            con_y = con_rows - 1;
        }
        return;
    }

    if (ch == '\r' || ch == 13) {
        con_x = 0;
        return;
    }

    if (ch == 8) { /* backspace */
        if (con_x > 0) {
            con_x--;
            render_char(con_x, con_y, ' ');
            int ring_line = (scroll_base + con_y) % SCROLL_BUF_LINES;
            scroll_buf[ring_line][con_x].ch = ' ';
            scroll_buf[ring_line][con_x].attr = con_attr;
        }
        return;
    }

    render_char(con_x, con_y, (uint8_t)ch);
    /* Store in ring buffer */
    {
        int ring_line = (scroll_base + con_y) % SCROLL_BUF_LINES;
        scroll_buf[ring_line][con_x].ch = (uint8_t)ch;
        scroll_buf[ring_line][con_x].attr = con_attr;
    }
    advance_cursor();
}

void fb_print(const char *str) {
    if (!fb_ok) return;
    while (*str) {
        fb_putchar((uint8_t)*str++);
    }
}

void fb_newline(void) {
    fb_putchar('\n');
}

int fb_get_cursor_row(void) { return con_y; }
int fb_get_cursor_col(void) { return con_x; }
int fb_is_ready(void)       { return fb_ok; }
int fb_get_width(void)      { return fb_width; }
int fb_get_height(void)     { return fb_height; }
int fb_get_cols(void)       { return con_cols; }
int fb_get_rows(void)       { return con_rows; }

/* ========================================================================= */
/* Scrollback navigation                                                     */
/* ========================================================================= */

void fb_page_up(void) {
    if (!fb_ok) return;
    int max_back = scroll_total;
    int limit = SCROLL_BUF_LINES - con_rows;
    if (max_back > limit) max_back = limit;
    if (max_back <= 0) return;

    scroll_view += SCROLL_STEP;
    if (scroll_view > max_back) scroll_view = max_back;
    con_dirty = 1;
    render_from_buffer();
}

void fb_page_down(void) {
    if (!fb_ok) return;
    if (scroll_view <= 0) return;

    scroll_view -= SCROLL_STEP;
    if (scroll_view < 0) scroll_view = 0;
    con_dirty = 1;
    render_from_buffer();
}

/* ========================================================================= */
/* Graphics primitives for window manager                                    */
/* ========================================================================= */

void fb_fill_rect(int x, int y, int w, int h, uint32_t color) {
    if (!fb_ok) return;
    if (x < 0) { w += x; x = 0; }
    if (y < 0) { h += y; y = 0; }
    if (x + w > (int)fb_width) w = (int)fb_width - x;
    if (y + h > (int)fb_height) h = (int)fb_height - y;
    if (w <= 0 || h <= 0) return;
    fill_rect(x, y, w, h, color);
}

void fb_draw_char_xy(int px, int py, uint8_t ch, uint32_t fg, uint32_t bg) {
    if (!fb_ok) return;
    uint8_t *glyph = &font_data[ch * FONT_H];
    for (int y = 0; y < FONT_H; y++) {
        int sy = py + y;
        if (sy < 0 || sy >= (int)fb_height) continue;
        uint32_t *scanline = (uint32_t *)(render_buf + sy * fb_pitch);
        uint8_t bits = glyph[y];
        for (int x = 0; x < FONT_W; x++) {
            int sx = px + x;
            if (sx < 0 || sx >= (int)fb_width) continue;
            scanline[sx] = (bits & (0x80 >> x)) ? fg : bg;
        }
    }
}

void fb_draw_text_xy(int px, int py, const char *text, uint32_t fg, uint32_t bg) {
    if (!fb_ok) return;
    while (*text) {
        fb_draw_char_xy(px, py, (uint8_t)*text, fg, bg);
        px += FONT_W;
        text++;
    }
}

void fb_blit(int x, int y, int w, int h, const uint32_t *pixels) {
    if (!fb_ok || !pixels) return;
    /* Clip to screen bounds */
    int sx = x, sy = y, sw = w, sh = h;
    int src_x = 0, src_y = 0;
    if (sx < 0) { src_x = -sx; sw += sx; sx = 0; }
    if (sy < 0) { src_y = -sy; sh += sy; sy = 0; }
    if (sx + sw > (int)fb_width) sw = (int)fb_width - sx;
    if (sy + sh > (int)fb_height) sh = (int)fb_height - sy;
    if (sw <= 0 || sh <= 0) return;

    /* Row-level memcpy for fast blit */
    for (int r = 0; r < sh; r++) {
        uint32_t *dst = (uint32_t *)(render_buf + (sy + r) * fb_pitch) + sx;
        const uint32_t *src = pixels + (src_y + r) * w + src_x;
        memcpy(dst, src, sw * 4);
    }
}

void fb_render_console(void) {
    if (!fb_ok) return;
    uint32_t fb_size = fb_height * fb_pitch;

    if (fb_console) {
        if (con_dirty) {
            /* Re-render console to cache */
            uint8_t *save = render_buf;
            render_buf = fb_console;
            render_from_buffer();
            render_buf = save;
            con_dirty = 0;
        }
        /* Blit cached console to current render target */
        memcpy(render_buf, fb_console, fb_size);
    } else {
        render_from_buffer();
    }
}

/* ========================================================================= */
/* Double buffering                                                          */
/* ========================================================================= */

void fb_begin_frame(void) {
    if (fb_back)
        render_buf = fb_back;
}

void fb_end_frame(void) {
    if (fb_back && render_buf == fb_back) {
        memcpy(fb_addr, fb_back, fb_height * fb_pitch);
        render_buf = fb_addr;
    }
}

uint8_t *fb_get_target(void) {
    return render_buf;
}

uint8_t  *fb_get_addr(void)  { return fb_addr; }
uint32_t  fb_get_pitch(void) { return fb_pitch; }
