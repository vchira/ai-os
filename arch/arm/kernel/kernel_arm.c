/* AiOS — ARM Kernel Main (Raspberry Pi 3/4/5)
   Master initialization for the ARM platform. */

#include "include/types.h"

/* ARM driver prototypes */
extern void uart_init(void);
extern void uart_puts(const char *s);
extern void fb_arm_init(void);
extern void gpio_init(void);
extern void timer_arm_init(void);
extern void interrupts_arm_init(void);
extern void mailbox_init(void);
extern void arm_heap_init(void);
extern void dwc_usb_init(void);

/* Shared C code from main tree */
extern void tool_executor_init(void);
extern void context_init(void);

/* ARM IRQ dispatcher */
void arm_irq_dispatch(void) {
    /* TODO: read interrupt controller, dispatch to appropriate handler */
}

void kernel_main_arm(void) {
    /* Phase 1: UART (debug output before framebuffer) */
    uart_init();
    uart_puts("\r\n");
    uart_puts("     _    _  ___  ____  \r\n");
    uart_puts("    / \\  (_)/ _ \\/ ___| \r\n");
    uart_puts("   / _ \\ | | | | \\___ \\ \r\n");
    uart_puts("  / ___ \\| | |_| |___) |\r\n");
    uart_puts(" /_/   \\_\\_|\\___/|____/ \r\n");
    uart_puts("\r\n");
    uart_puts("AiOS v0.1 - ARM/Raspberry Pi\r\n\r\n");

    /* Phase 2: Core init */
    uart_puts("[OK] UART initialized\r\n");

    gpio_init();
    uart_puts("[OK] GPIO initialized\r\n");

    arm_heap_init();
    uart_puts("[OK] Heap initialized\r\n");

    interrupts_arm_init();
    uart_puts("[OK] Interrupts initialized\r\n");

    timer_arm_init();
    uart_puts("[OK] System timer initialized\r\n");

    /* Phase 3: Mailbox + Framebuffer */
    mailbox_init();
    uart_puts("[OK] Mailbox initialized\r\n");

    fb_arm_init();
    uart_puts("[OK] Framebuffer initialized\r\n");

    /* Phase 4: USB (DWC2 on RPi) */
    dwc_usb_init();
    uart_puts("[OK] USB initialized (DWC2)\r\n");

    /* Phase 5: Shared subsystems */
    uart_puts("[OK] Boot complete\r\n");

    /* Idle loop */
    while (1) {
        __asm__ volatile("wfe");
    }
}
