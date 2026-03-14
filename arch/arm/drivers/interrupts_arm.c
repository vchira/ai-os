/* AiOS — ARM Interrupt Controller Driver (Raspberry Pi)
   The BCM2835 has a simple interrupt controller (not GIC).
   RPi 4 uses GIC-400 but we target the simpler BCM2835/2836 controller
   first for RPi 3 compatibility. */

#include "include/types.h"

#ifndef PERIPH_BASE
#define PERIPH_BASE  0x3F000000
#endif

#define IRQ_BASE     (PERIPH_BASE + 0xB200)

#define IRQ_PENDING1    (*(volatile uint32_t *)(IRQ_BASE + 0x04))
#define IRQ_PENDING2    (*(volatile uint32_t *)(IRQ_BASE + 0x08))
#define IRQ_ENABLE1     (*(volatile uint32_t *)(IRQ_BASE + 0x10))
#define IRQ_ENABLE2     (*(volatile uint32_t *)(IRQ_BASE + 0x14))
#define IRQ_DISABLE1    (*(volatile uint32_t *)(IRQ_BASE + 0x1C))
#define IRQ_DISABLE2    (*(volatile uint32_t *)(IRQ_BASE + 0x20))

/* IRQ numbers */
#define IRQ_TIMER1   1    /* System Timer Compare 1 */
#define IRQ_TIMER3   3    /* System Timer Compare 3 */
#define IRQ_USB      9    /* USB (DWC2) */
#define IRQ_AUX     29    /* Auxiliary (mini UART, SPI1, SPI2) */

extern void timer_arm_handler(void);

void interrupts_arm_init(void) {
    /* Disable all IRQs first */
    IRQ_DISABLE1 = 0xFFFFFFFF;
    IRQ_DISABLE2 = 0xFFFFFFFF;

    /* Enable system timer 1 interrupt */
    IRQ_ENABLE1 = (1 << IRQ_TIMER1);

    /* Enable USB interrupt */
    IRQ_ENABLE1 = (1 << IRQ_USB);
}

/* Called from irq_handler in start.S */
void arm_irq_dispatch(void) {
    uint32_t pending = IRQ_PENDING1;

    /* System Timer Compare 1 */
    if (pending & (1 << IRQ_TIMER1)) {
        timer_arm_handler();
    }
}

void irq_enable(int irq) {
    if (irq < 32)
        IRQ_ENABLE1 = (1 << irq);
    else
        IRQ_ENABLE2 = (1 << (irq - 32));
}

void irq_disable(int irq) {
    if (irq < 32)
        IRQ_DISABLE1 = (1 << irq);
    else
        IRQ_DISABLE2 = (1 << (irq - 32));
}
