/* AiOS — BMP Image Decoder
   Supports uncompressed 24-bit and 32-bit BMPs (BI_RGB).
   BMP stores rows bottom-up by default; this decoder flips to top-down. */

#include "include/bmp.h"
#include "include/heap.h"
#include "include/string.h"

/* BMP file header fields (packed, but we read manually for portability) */
#define BM_MAGIC  0x4D42

static uint16_t read16(const uint8_t *p) {
    return p[0] | (p[1] << 8);
}

static uint32_t read32(const uint8_t *p) {
    return p[0] | (p[1] << 8) | (p[2] << 16) | (p[3] << 24);
}

uint32_t *bmp_decode(const uint8_t *data, int size, int *out_w, int *out_h) {
    if (!data || size < 54) return 0;

    /* Check BM magic */
    if (read16(data) != BM_MAGIC) return 0;

    uint32_t pixel_offset = read32(data + 10);
    /* DIB header */
    int32_t width  = (int32_t)read32(data + 18);
    int32_t height = (int32_t)read32(data + 22);
    uint16_t bpp   = read16(data + 28);
    uint32_t compr = read32(data + 30);

    /* Only support uncompressed */
    if (compr != 0) return 0;
    /* Only 24-bit or 32-bit */
    if (bpp != 24 && bpp != 32) return 0;

    int abs_h = height < 0 ? -height : height;
    int top_down = (height < 0);

    if (width <= 0 || abs_h <= 0 || width > 4096 || abs_h > 4096) return 0;

    /* BMP rows are padded to 4-byte boundaries */
    int row_bytes = (width * (bpp / 8) + 3) & ~3;
    if ((int)(pixel_offset + row_bytes * abs_h) > size) return 0;

    uint32_t *pixels = malloc(width * abs_h * 4);
    if (!pixels) return 0;

    for (int y = 0; y < abs_h; y++) {
        /* BMP default is bottom-up; flip unless top-down */
        int src_y = top_down ? y : (abs_h - 1 - y);
        const uint8_t *row = data + pixel_offset + src_y * row_bytes;
        uint32_t *dst = pixels + y * width;

        if (bpp == 24) {
            for (int x = 0; x < width; x++) {
                uint8_t b = row[x * 3 + 0];
                uint8_t g = row[x * 3 + 1];
                uint8_t r = row[x * 3 + 2];
                dst[x] = 0xFF000000 | (r << 16) | (g << 8) | b;
            }
        } else { /* 32-bit */
            for (int x = 0; x < width; x++) {
                uint8_t b = row[x * 4 + 0];
                uint8_t g = row[x * 4 + 1];
                uint8_t r = row[x * 4 + 2];
                uint8_t a = row[x * 4 + 3];
                dst[x] = (a << 24) | (r << 16) | (g << 8) | b;
            }
        }
    }

    *out_w = width;
    *out_h = abs_h;
    return pixels;
}
