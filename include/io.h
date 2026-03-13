#ifndef AIOS_IO_H
#define AIOS_IO_H

#include "types.h"

static inline void outb(uint16_t port, uint8_t val) {
    __asm__ volatile("outb %0, %1" : : "a"(val), "Nd"(port));
}

static inline uint8_t inb(uint16_t port) {
    uint8_t ret;
    __asm__ volatile("inb %1, %0" : "=a"(ret) : "Nd"(port));
    return ret;
}

static inline void outw(uint16_t port, uint16_t val) {
    __asm__ volatile("outw %0, %1" : : "a"(val), "Nd"(port));
}

static inline uint16_t inw(uint16_t port) {
    uint16_t ret;
    __asm__ volatile("inw %1, %0" : "=a"(ret) : "Nd"(port));
    return ret;
}

static inline void outl(uint16_t port, uint32_t val) {
    __asm__ volatile("outl %0, %1" : : "a"(val), "Nd"(port));
}

static inline uint32_t inl(uint16_t port) {
    uint32_t ret;
    __asm__ volatile("inl %1, %0" : "=a"(ret) : "Nd"(port));
    return ret;
}

static inline void io_wait(void) {
    outb(0x80, 0);
}

/* VGA print functions — C-callable wrappers around asm (see kernel/c_api.asm) */
extern void c_vga_print(const char *str);
extern void c_vga_print_hex(uint32_t val);
extern void c_vga_print_dec(uint32_t val);
extern void c_vga_set_color(uint8_t color);
extern void c_vga_newline(void);

/* Convenience aliases for C code */
#define vga_print     c_vga_print
#define vga_print_hex c_vga_print_hex
#define vga_print_dec c_vga_print_dec
#define vga_set_color c_vga_set_color
#define vga_newline   c_vga_newline

#endif
