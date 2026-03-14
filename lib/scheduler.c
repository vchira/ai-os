/* AiOS — Scheduler
   Manages timed events and reminders. The AI sets reminders via the
   set_reminder tool. scheduler_tick() is called from the keyboard wait
   loop and checks if any events should fire. */

#include "include/scheduler.h"
#include "include/rtc.h"
#include "include/string.h"
#include "include/io.h"
#include "include/stdio.h"

/* External VGA functions for notification display */
extern void fb_print(const char *str);
extern void fb_set_color(int attr);
extern void fb_newline(void);

static sched_event_t events[SCHED_MAX_EVENTS];
static int last_tick_second = -1;  /* avoid firing multiple times per second */

void scheduler_init(void) {
    memset(events, 0, sizeof(events));
}

/* Compare RTC time against event trigger time.
   Returns 1 if current time >= trigger time. */
static int time_reached(sched_event_t *ev, rtc_time_t *now) {
    if (now->year > ev->year) return 1;
    if (now->year < ev->year) return 0;
    if (now->month > ev->month) return 1;
    if (now->month < ev->month) return 0;
    if (now->day > ev->day) return 1;
    if (now->day < ev->day) return 0;
    if (now->hour > ev->hour) return 1;
    if (now->hour < ev->hour) return 0;
    if (now->minute > ev->minute) return 1;
    if (now->minute < ev->minute) return 0;
    if (now->second >= ev->second) return 1;
    return 0;
}

/* Display a notification on screen */
static void show_notification(const char *message) {
    fb_newline();
    fb_set_color(0x4F);  /* white on red — attention! */
    fb_print(" ! REMINDER: ");
    fb_print(message);
    fb_print(" ");
    fb_set_color(0x07);  /* reset */
    fb_newline();

    /* PC speaker beep — short alert tone */
    /* PIT channel 2: set frequency ~880Hz */
    outb(0x43, 0xB6);          /* channel 2, mode 3, lo/hi byte */
    uint16_t div = 1193180 / 880;
    outb(0x42, div & 0xFF);
    outb(0x42, (div >> 8) & 0xFF);
    /* Enable speaker */
    uint8_t tmp = inb(0x61);
    outb(0x61, tmp | 0x03);
    /* Brief delay (~100ms worth of busy-wait) */
    for (volatile int i = 0; i < 500000; i++);
    /* Disable speaker */
    outb(0x61, tmp & ~0x03);
}

void scheduler_tick(void) {
    rtc_time_t now;
    rtc_get_time(&now);

    /* Only check once per second to avoid redundant work */
    if (now.second == last_tick_second) return;
    last_tick_second = now.second;

    for (int i = 0; i < SCHED_MAX_EVENTS; i++) {
        if (!events[i].active) continue;
        if (time_reached(&events[i], &now)) {
            /* Fire the event */
            show_notification(events[i].message);
            events[i].active = 0;
        }
    }
}

int scheduler_add(int trigger_minutes, int abs_hour, int abs_minute,
                  const char *message) {
    /* Find free slot */
    int slot = -1;
    for (int i = 0; i < SCHED_MAX_EVENTS; i++) {
        if (!events[i].active) { slot = i; break; }
    }
    if (slot < 0) return -1;

    rtc_time_t now;
    rtc_get_time(&now);

    if (trigger_minutes > 0) {
        /* Relative time — add minutes to current time */
        int total_min = now.hour * 60 + now.minute + trigger_minutes;
        int carry_days = total_min / (24 * 60);
        total_min %= (24 * 60);

        events[slot].year = now.year;
        events[slot].month = now.month;
        events[slot].day = now.day + carry_days;  /* simplified — doesn't handle month overflow */
        events[slot].hour = total_min / 60;
        events[slot].minute = total_min % 60;
        events[slot].second = now.second;
    } else {
        /* Absolute time today */
        events[slot].year = now.year;
        events[slot].month = now.month;
        events[slot].day = now.day;
        events[slot].hour = abs_hour;
        events[slot].minute = abs_minute;
        events[slot].second = 0;
    }

    strncpy(events[slot].message, message, SCHED_MSG_MAX - 1);
    events[slot].message[SCHED_MSG_MAX - 1] = '\0';
    events[slot].active = 1;

    return slot;
}

int scheduler_cancel(int index) {
    if (index < 0 || index >= SCHED_MAX_EVENTS) return -1;
    if (!events[index].active) return -1;
    events[index].active = 0;
    return 0;
}

int scheduler_list(char *buf, int max) {
    int p = 0;
    int count = 0;

    for (int i = 0; i < SCHED_MAX_EVENTS; i++)
        if (events[i].active) count++;

    if (count == 0) {
        const char *s = "No pending reminders.\n";
        int len = strlen(s);
        if (len < max) { memcpy(buf, s, len); buf[len] = '\0'; return len; }
        return 0;
    }

    for (int i = 0; i < SCHED_MAX_EVENTS && p < max - 80; i++) {
        if (!events[i].active) continue;
        p += snprintf(buf + p, max - p, "  [%d] %04d-%02d-%02d %02d:%02d - %s\n",
                      i, events[i].year, events[i].month, events[i].day,
                      events[i].hour, events[i].minute, events[i].message);
    }
    buf[p] = '\0';
    return p;
}

int scheduler_count(void) {
    int count = 0;
    for (int i = 0; i < SCHED_MAX_EVENTS; i++)
        if (events[i].active) count++;
    return count;
}

sched_event_t *scheduler_get_events(void) {
    return events;
}
