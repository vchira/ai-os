/* AiOS — CMOS Real-Time Clock Driver
   Reads date/time from the MC146818 RTC via ports 0x70/0x71. */

#include "include/rtc.h"
#include "include/io.h"
#include "include/string.h"

#define CMOS_ADDR 0x70
#define CMOS_DATA 0x71

/* CMOS register addresses */
#define RTC_SECONDS  0x00
#define RTC_MINUTES  0x02
#define RTC_HOURS    0x04
#define RTC_DAY      0x07
#define RTC_MONTH    0x08
#define RTC_YEAR     0x09
#define RTC_CENTURY  0x32
#define RTC_STATUS_A 0x0A
#define RTC_STATUS_B 0x0B

static uint8_t cmos_read(uint8_t reg) {
    outb(CMOS_ADDR, reg);
    return inb(CMOS_DATA);
}

static uint8_t bcd_to_bin(uint8_t bcd) {
    return ((bcd >> 4) * 10) + (bcd & 0x0F);
}

static int rtc_is_updating(void) {
    return cmos_read(RTC_STATUS_A) & 0x80;
}

void rtc_init(void) {
    /* CMOS clock is always running — nothing to initialize. */
}

void rtc_get_time(rtc_time_t *t) {
    /* Wait for any update-in-progress to complete */
    while (rtc_is_updating());

    uint8_t second  = cmos_read(RTC_SECONDS);
    uint8_t minute  = cmos_read(RTC_MINUTES);
    uint8_t hour    = cmos_read(RTC_HOURS);
    uint8_t day     = cmos_read(RTC_DAY);
    uint8_t month   = cmos_read(RTC_MONTH);
    uint8_t year    = cmos_read(RTC_YEAR);
    uint8_t century = cmos_read(RTC_CENTURY);

    /* Read status register B to determine data format */
    uint8_t regB = cmos_read(RTC_STATUS_B);

    /* Convert from BCD if needed (bit 2 of regB = 0 means BCD) */
    if (!(regB & 0x04)) {
        second  = bcd_to_bin(second);
        minute  = bcd_to_bin(minute);
        hour    = bcd_to_bin(hour & 0x7F) | (hour & 0x80);
        day     = bcd_to_bin(day);
        month   = bcd_to_bin(month);
        year    = bcd_to_bin(year);
        century = bcd_to_bin(century);
    }

    /* Convert 12-hour to 24-hour if needed (bit 1 of regB = 0 means 12h) */
    if (!(regB & 0x02) && (hour & 0x80)) {
        hour = ((hour & 0x7F) + 12) % 24;
    }

    t->second = second;
    t->minute = minute;
    t->hour   = hour;
    t->day    = day;
    t->month  = month;
    t->year   = (century ? (uint16_t)century * 100 : 2000) + year;
}

int rtc_format_datetime(char *buf, int max, rtc_time_t *t) {
    /* "YYYY-MM-DD HH:MM:SS" = 19 chars + null */
    if (max < 20) return -1;

    buf[0]  = '0' + (t->year / 1000) % 10;
    buf[1]  = '0' + (t->year / 100)  % 10;
    buf[2]  = '0' + (t->year / 10)   % 10;
    buf[3]  = '0' + (t->year)        % 10;
    buf[4]  = '-';
    buf[5]  = '0' + (t->month / 10);
    buf[6]  = '0' + (t->month % 10);
    buf[7]  = '-';
    buf[8]  = '0' + (t->day / 10);
    buf[9]  = '0' + (t->day % 10);
    buf[10] = ' ';
    buf[11] = '0' + (t->hour / 10);
    buf[12] = '0' + (t->hour % 10);
    buf[13] = ':';
    buf[14] = '0' + (t->minute / 10);
    buf[15] = '0' + (t->minute % 10);
    buf[16] = ':';
    buf[17] = '0' + (t->second / 10);
    buf[18] = '0' + (t->second % 10);
    buf[19] = '\0';
    return 19;
}

int rtc_format_date(char *buf, int max, rtc_time_t *t) {
    /* "YYYY-MM-DD" = 10 chars + null */
    if (max < 11) return -1;

    buf[0]  = '0' + (t->year / 1000) % 10;
    buf[1]  = '0' + (t->year / 100)  % 10;
    buf[2]  = '0' + (t->year / 10)   % 10;
    buf[3]  = '0' + (t->year)        % 10;
    buf[4]  = '-';
    buf[5]  = '0' + (t->month / 10);
    buf[6]  = '0' + (t->month % 10);
    buf[7]  = '-';
    buf[8]  = '0' + (t->day / 10);
    buf[9]  = '0' + (t->day % 10);
    buf[10] = '\0';
    return 10;
}

void rtc_get_datetime_str(char *buf, int max) {
    rtc_time_t t;
    rtc_get_time(&t);
    if (rtc_format_datetime(buf, max, &t) < 0) {
        buf[0] = '?';
        buf[1] = '\0';
    }
}
