#ifndef AIOS_INTTYPES_H
#define AIOS_INTTYPES_H

#include "types.h"

/* Printf format macros — not used in freestanding but lwIP references them */
#define PRId8   "d"
#define PRId16  "d"
#define PRId32  "d"
#define PRId64  "lld"
#define PRIu8   "u"
#define PRIu16  "u"
#define PRIu32  "u"
#define PRIu64  "llu"
#define PRIx8   "x"
#define PRIx16  "x"
#define PRIx32  "x"
#define PRIx64  "llx"

#endif
