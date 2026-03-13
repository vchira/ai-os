#ifndef AIOS_STDIO_H
#define AIOS_STDIO_H

/* Stub — we have no stdio, but some libraries reference it */
#include "types.h"

#define EOF (-1)

/* snprintf stub — returns 0, doesn't actually format */
static inline int snprintf(char *buf, size_t n, const char *fmt, ...) {
    (void)buf; (void)n; (void)fmt;
    if (n > 0) buf[0] = '\0';
    return 0;
}

#endif
