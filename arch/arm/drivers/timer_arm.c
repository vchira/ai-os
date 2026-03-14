/* AiOS — ARM System Timer Driver (Raspberry Pi)
   Uses the BCM2835 System Timer (not the ARM Timer).
   The system timer runs at 1MHz and has 4 compare channels (0-3).
   Channels 0 and 2 are used by the GPU; we use channel 1. */

#include "include/types.h"

#ifndef PERIPH_BASE
#define PERIPH_BASE  0x3F000000
#endif

#define TIMER_BASE   (PERIPH_BASE + 0x3000)

#define TIMER_CS     (*(volatile uint32_t *)(TIMER_BASE + 0x00))
#define TIMER_CLO    (*(volatile uint32_t *)(TIMER_BASE + 0x04))
#define TIMER_CHI    (*(volatile uint32_t *)(TIMER_BASE + 0x08))
#define TIMER_C1     (*(volatile uint32_t *)(TIMER_BASE + 0x10))
#define TIMER_C3     (*(volatile uint32_t *)(TIMER_BASE + 0x18))

/* Timer interval: 10ms (100 Hz) = 10000 microseconds */
#define TIMER_INTERVAL  10000

static volatile uint32_t ticks = 0;

void timer_arm_init(void) {
    /* Set compare for channel 1 */
    TIMER_C1 = TIMER_CLO + TIMER_INTERVAL;
    ticks = 0;
}

void timer_arm_handler(void) {
    /* Acknowledge timer match */
    TIMER_CS = (1 << 1);
    /* Set next compare */
    TIMER_C1 = TIMER_CLO + TIMER_INTERVAL;
    ticks++;
}

uint32_t timer_arm_get_ticks(void) {
    return ticks;
}

/* Busy-wait delay in microseconds */
void delay_us(uint32_t us) {
    uint32_t start = TIMER_CLO;
    while (TIMER_CLO - start < us) ;
}

/* Busy-wait delay in milliseconds */
void delay_ms_arm(uint32_t ms) {
    delay_us(ms * 1000);
}
