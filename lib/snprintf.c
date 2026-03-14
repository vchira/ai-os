/* AiOS — minimal snprintf/vsnprintf for mbedTLS
   Supports: %s %c %d %i %u %x %X %ld %li %lu %lx %lX %p %%
   Width and zero-fill (e.g. %02X) supported.
   Uses 32-bit math only (no 64-bit division on i386). */

#include "include/types.h"
#include <stdarg.h>

static int out_char(char *buf, size_t size, int pos, char c) {
    if ((size_t)pos < size) buf[pos] = c;
    return pos + 1;
}

static int out_str(char *buf, size_t size, int pos, const char *s) {
    while (*s) pos = out_char(buf, size, pos, *s++);
    return pos;
}

static int out_uint(char *buf, size_t size, int pos,
                    unsigned long val, int base, int upper,
                    int width, int zero_pad) {
    char tmp[16];
    const char *digits = upper ? "0123456789ABCDEF" : "0123456789abcdef";
    int i = 0;
    if (val == 0) tmp[i++] = '0';
    while (val > 0) { tmp[i++] = digits[val % base]; val /= base; }
    while (i < width) tmp[i++] = zero_pad ? '0' : ' ';
    while (--i >= 0) pos = out_char(buf, size, pos, tmp[i]);
    return pos;
}

int vsnprintf(char *buf, size_t size, const char *fmt, va_list ap) {
    int pos = 0;
    if (!fmt) { if (size > 0) buf[0] = '\0'; return 0; }

    while (*fmt) {
        if (*fmt != '%') { pos = out_char(buf, size, pos, *fmt++); continue; }
        fmt++;  /* skip '%' */

        /* flags */
        int zero_pad = 0, width = 0, is_long = 0;
        if (*fmt == '0') { zero_pad = 1; fmt++; }
        while (*fmt >= '0' && *fmt <= '9') { width = width * 10 + (*fmt - '0'); fmt++; }
        if (*fmt == 'l') { is_long = 1; fmt++; }
        if (*fmt == 'l') { is_long = 2; fmt++; }  /* ll — treated as long on 32-bit */

        switch (*fmt) {
        case '%': pos = out_char(buf, size, pos, '%'); break;
        case 'c': pos = out_char(buf, size, pos, (char)va_arg(ap, int)); break;
        case 's': {
            const char *s = va_arg(ap, const char *);
            pos = out_str(buf, size, pos, s ? s : "(null)");
            break;
        }
        case 'd': case 'i': {
            long val;
            if (is_long >= 1) val = va_arg(ap, long);
            else val = va_arg(ap, int);
            if (val < 0) { pos = out_char(buf, size, pos, '-'); val = -val; if (width > 0) width--; }
            pos = out_uint(buf, size, pos, (unsigned long)val, 10, 0, width, zero_pad);
            break;
        }
        case 'u': {
            unsigned long val;
            if (is_long >= 1) val = va_arg(ap, unsigned long);
            else val = va_arg(ap, unsigned int);
            pos = out_uint(buf, size, pos, val, 10, 0, width, zero_pad);
            break;
        }
        case 'x': case 'X': {
            unsigned long val;
            if (is_long >= 1) val = va_arg(ap, unsigned long);
            else val = va_arg(ap, unsigned int);
            pos = out_uint(buf, size, pos, val, 16, *fmt == 'X', width, zero_pad);
            break;
        }
        case 'p': {
            pos = out_str(buf, size, pos, "0x");
            unsigned long val = (unsigned long)va_arg(ap, void *);
            pos = out_uint(buf, size, pos, val, 16, 0, 8, 1);
            break;
        }
        case '\0': goto done;
        default: pos = out_char(buf, size, pos, *fmt); break;
        }
        fmt++;
    }
done:
    if (size > 0) buf[pos < (int)size ? pos : (int)size - 1] = '\0';
    return pos;
}

int snprintf(char *buf, size_t size, const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    int ret = vsnprintf(buf, size, fmt, ap);
    va_end(ap);
    return ret;
}
