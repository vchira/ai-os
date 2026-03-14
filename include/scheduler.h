/* AiOS — Scheduler: timed events and reminders */
#ifndef AIOS_SCHEDULER_H
#define AIOS_SCHEDULER_H

#include "types.h"

#define SCHED_MAX_EVENTS  16
#define SCHED_MSG_MAX     128

typedef struct {
    int      active;
    uint16_t year, month, day, hour, minute, second;  /* trigger time (RTC) */
    char     message[SCHED_MSG_MAX];
} sched_event_t;

/* Initialize scheduler (loads persisted events from disk). */
void scheduler_init(void);

/* Called frequently (from keyboard wait loop). Checks RTC and fires events. */
void scheduler_tick(void);

/* Add a reminder. trigger_minutes = minutes from now (0 = use absolute time).
   Returns event index, or -1 on error. */
int scheduler_add(int trigger_minutes, int abs_hour, int abs_minute,
                  const char *message);

/* Cancel an event by index. Returns 0 on success. */
int scheduler_cancel(int index);

/* List pending events into buf. Returns length written. */
int scheduler_list(char *buf, int max);

/* Get number of active events. */
int scheduler_count(void);

/* Persistence — called by tool_executor's persist_save/load */
sched_event_t *scheduler_get_events(void);

#endif
