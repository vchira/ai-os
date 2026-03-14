/* AiOS freestanding stdio.h */
#ifndef AIOS_STDIO_H
#define AIOS_STDIO_H

#include "types.h"
#include <stdarg.h>

#define EOF (-1)

typedef void FILE;
#define stdout ((FILE *)1)
#define stderr ((FILE *)2)

int snprintf(char *buf, size_t size, const char *fmt, ...);
int vsnprintf(char *buf, size_t size, const char *fmt, va_list ap);

/* No-op stubs */
static inline int printf(const char *fmt, ...) { (void)fmt; return 0; }
static inline int fprintf(FILE *stream, const char *fmt, ...) {
    (void)stream; (void)fmt; return 0;
}
static inline void setbuf(FILE *stream, char *buf) {
    (void)stream; (void)buf;
}

#endif
