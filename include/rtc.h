/* AiOS — CMOS Real-Time Clock Driver */
#ifndef AIOS_RTC_H
#define AIOS_RTC_H

#include "types.h"

typedef struct {
    uint16_t year;
    uint8_t  month;
    uint8_t  day;
    uint8_t  hour;
    uint8_t  minute;
    uint8_t  second;
} rtc_time_t;

void rtc_init(void);
void rtc_get_time(rtc_time_t *t);
int  rtc_format_datetime(char *buf, int max, rtc_time_t *t);
int  rtc_format_date(char *buf, int max, rtc_time_t *t);

/* Simple wrapper: writes "YYYY-MM-DD HH:MM:SS" into buf */
void rtc_get_datetime_str(char *buf, int max);

#endif
