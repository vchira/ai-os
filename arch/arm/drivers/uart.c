/* AiOS — ARM Mini UART Driver (Raspberry Pi)
   Uses the BCM2835/2836/2837 mini UART (auxiliary UART)
   which is simpler than PL011 and connected to GPIO 14/15.

   Peripheral base addresses:
   - RPi 1:     0x20000000
   - RPi 2/3:   0x3F000000
   - RPi 4/5:   0xFE000000

   We detect at runtime or default to RPi 3/4. */

#include "include/types.h"

/* Detect RPi version at compile time or use RPi 3 default */
#ifndef PERIPH_BASE
#define PERIPH_BASE  0x3F000000   /* RPi 2/3 */
#endif

#define GPIO_BASE    (PERIPH_BASE + 0x200000)
#define AUX_BASE     (PERIPH_BASE + 0x215000)

/* GPIO registers */
#define GPFSEL1      (*(volatile uint32_t *)(GPIO_BASE + 0x04))
#define GPPUD        (*(volatile uint32_t *)(GPIO_BASE + 0x94))
#define GPPUDCLK0    (*(volatile uint32_t *)(GPIO_BASE + 0x98))

/* Auxiliary peripherals */
#define AUX_ENABLES  (*(volatile uint32_t *)(AUX_BASE + 0x04))
#define AUX_MU_IO    (*(volatile uint32_t *)(AUX_BASE + 0x40))
#define AUX_MU_IER   (*(volatile uint32_t *)(AUX_BASE + 0x44))
#define AUX_MU_IIR   (*(volatile uint32_t *)(AUX_BASE + 0x48))
#define AUX_MU_LCR   (*(volatile uint32_t *)(AUX_BASE + 0x4C))
#define AUX_MU_MCR   (*(volatile uint32_t *)(AUX_BASE + 0x50))
#define AUX_MU_LSR   (*(volatile uint32_t *)(AUX_BASE + 0x54))
#define AUX_MU_CNTL  (*(volatile uint32_t *)(AUX_BASE + 0x60))
#define AUX_MU_BAUD  (*(volatile uint32_t *)(AUX_BASE + 0x68))

static void delay(int count) {
    for (volatile int i = 0; i < count; i++) ;
}

void uart_init(void) {
    /* Enable mini UART */
    AUX_ENABLES = 1;

    /* Disable TX/RX while configuring */
    AUX_MU_CNTL = 0;

    /* Disable interrupts */
    AUX_MU_IER = 0;

    /* 8-bit mode */
    AUX_MU_LCR = 3;

    /* RTS high */
    AUX_MU_MCR = 0;

    /* Clear FIFOs */
    AUX_MU_IIR = 0xC6;

    /* 115200 baud @ 250MHz system clock
       baudrate = system_clock / (8 * (AUX_MU_BAUD + 1))
       AUX_MU_BAUD = 250000000 / (8 * 115200) - 1 = 270 */
    AUX_MU_BAUD = 270;

    /* Set GPIO 14/15 to ALT5 (mini UART) */
    uint32_t sel = GPFSEL1;
    sel &= ~(7 << 12);   /* GPIO 14: clear bits 14:12 */
    sel |= (2 << 12);    /* GPIO 14: ALT5 */
    sel &= ~(7 << 15);   /* GPIO 15: clear bits 17:15 */
    sel |= (2 << 15);    /* GPIO 15: ALT5 */
    GPFSEL1 = sel;

    /* Disable pull-up/down on GPIO 14/15 */
    GPPUD = 0;
    delay(150);
    GPPUDCLK0 = (1 << 14) | (1 << 15);
    delay(150);
    GPPUDCLK0 = 0;

    /* Enable TX and RX */
    AUX_MU_CNTL = 3;
}

void uart_putc(char c) {
    /* Wait for space in TX FIFO */
    while (!(AUX_MU_LSR & 0x20)) ;
    AUX_MU_IO = c;
}

char uart_getc(void) {
    /* Wait for data in RX FIFO */
    while (!(AUX_MU_LSR & 0x01)) ;
    return AUX_MU_IO & 0xFF;
}

void uart_puts(const char *s) {
    while (*s) {
        if (*s == '\n') uart_putc('\r');
        uart_putc(*s++);
    }
}

int uart_has_data(void) {
    return (AUX_MU_LSR & 0x01) ? 1 : 0;
}
