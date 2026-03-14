/* AiOS — ARM GPIO Driver (Raspberry Pi)
   Basic GPIO pin control for the BCM2835/2836/2837/2711 GPIO controller. */

#include "include/types.h"

#ifndef PERIPH_BASE
#define PERIPH_BASE  0x3F000000
#endif

#define GPIO_BASE    (PERIPH_BASE + 0x200000)

/* GPIO registers */
#define GPFSEL(n)    (*(volatile uint32_t *)(GPIO_BASE + 0x00 + (n) * 4))
#define GPSET(n)     (*(volatile uint32_t *)(GPIO_BASE + 0x1C + (n) * 4))
#define GPCLR(n)     (*(volatile uint32_t *)(GPIO_BASE + 0x28 + (n) * 4))
#define GPLEV(n)     (*(volatile uint32_t *)(GPIO_BASE + 0x34 + (n) * 4))

/* GPIO function select values */
#define GPIO_FUNC_INPUT   0
#define GPIO_FUNC_OUTPUT  1
#define GPIO_FUNC_ALT0    4
#define GPIO_FUNC_ALT1    5
#define GPIO_FUNC_ALT2    6
#define GPIO_FUNC_ALT3    7
#define GPIO_FUNC_ALT4    3
#define GPIO_FUNC_ALT5    2

void gpio_init(void) {
    /* Nothing to do globally — pins are configured individually */
}

void gpio_set_function(int pin, int func) {
    if (pin < 0 || pin > 53) return;
    int reg = pin / 10;
    int shift = (pin % 10) * 3;
    uint32_t val = GPFSEL(reg);
    val &= ~(7 << shift);
    val |= (func & 7) << shift;
    GPFSEL(reg) = val;
}

void gpio_set(int pin) {
    if (pin < 0 || pin > 53) return;
    GPSET(pin / 32) = 1 << (pin % 32);
}

void gpio_clear(int pin) {
    if (pin < 0 || pin > 53) return;
    GPCLR(pin / 32) = 1 << (pin % 32);
}

int gpio_read(int pin) {
    if (pin < 0 || pin > 53) return 0;
    return (GPLEV(pin / 32) >> (pin % 32)) & 1;
}
