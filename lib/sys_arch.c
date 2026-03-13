/* lwIP sys_arch — minimal implementation for NO_SYS=1 on AiOS */

#include "lib/lwip/src/include/lwip/sys.h"

/* Timer ticks from our PIT timer (asm) */
extern unsigned int timer_get_ticks(void);

/* sys_now: return milliseconds since boot (PIT runs at 100Hz) */
u32_t sys_now(void) {
    return timer_get_ticks() * 10; /* 100Hz ticks -> ms */
}

/* Simple pseudo-random number generator */
static unsigned int rand_state = 12345;

unsigned int aios_rand(void) {
    rand_state = rand_state * 1103515245 + 12345;
    return (rand_state >> 16) & 0x7FFF;
}
