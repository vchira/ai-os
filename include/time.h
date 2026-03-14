/* AiOS freestanding time.h */
#ifndef AIOS_TIME_H
#define AIOS_TIME_H

#include "types.h"

typedef int32_t time_t;

struct tm {
    int tm_sec;
    int tm_min;
    int tm_hour;
    int tm_mday;
    int tm_mon;
    int tm_year;
    int tm_wday;
    int tm_yday;
    int tm_isdst;
};

/* Stubs — mbedTLS x509 uses these for cert date validation,
   but we skip cert verification so they're rarely called. */
time_t time(time_t *t);
struct tm *gmtime(const time_t *t);

#endif
