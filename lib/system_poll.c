/* AiOS — System Poll
   Central periodic work dispatcher called from the idle loop.
   Consolidates network polling, scheduler checks, and any future
   periodic tasks into one place. */

#include "include/scheduler.h"

extern void net_poll(void);
extern void usb_poll(void);
extern void bt_poll(void);
extern int scheduler_due;  /* set by timer interrupt every second */

static int poll_counter = 0;

void system_poll(void) {
    /* Always poll network (TCP timers, DHCP, ARP) */
    net_poll();

    /* Poll USB HID + Bluetooth every ~10 ticks to avoid hogging CPU */
    if (++poll_counter >= 10) {
        poll_counter = 0;
        usb_poll();
        bt_poll();
    }

    /* Check scheduler only when the timer interrupt flags it (1/sec) */
    if (scheduler_due) {
        scheduler_due = 0;
        scheduler_tick();
    }
}
