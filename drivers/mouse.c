/* AiOS — PS/2 Mouse Driver
   Handles IRQ12, decodes 3-byte packets, tracks absolute position. */

#include "include/mouse.h"
#include "include/io.h"
#include "include/framebuffer.h"

#define PS2_DATA   0x60
#define PS2_STATUS 0x64
#define PS2_CMD    0x64

static volatile int mouse_x, mouse_y;
static volatile int mouse_buttons;
static volatile int mouse_changed;
static int mouse_cycle;
static uint8_t mouse_bytes[3];
static int mouse_installed = 0;

/* From settings.c — 0=low, 1=normal, 2=high */
extern int mouse_sensitivity;

/* Wait for 8042 input buffer to be empty (bit 1 clear) */
static void ps2_wait_write(void) {
    for (int i = 0; i < 100000; i++)
        if (!(inb(PS2_STATUS) & 0x02)) return;
}

/* Wait for 8042 output buffer to have data (bit 0 set) */
static void ps2_wait_read(void) {
    for (int i = 0; i < 100000; i++)
        if (inb(PS2_STATUS) & 0x01) return;
}

/* Send command byte to the mouse via auxiliary port */
static void mouse_write(uint8_t data) {
    ps2_wait_write();
    outb(PS2_CMD, 0xD4);   /* next byte goes to mouse */
    ps2_wait_write();
    outb(PS2_DATA, data);
}

/* Read response byte from mouse */
static uint8_t mouse_read(void) {
    ps2_wait_read();
    return inb(PS2_DATA);
}

void mouse_init(void) {
    if (!fb_is_ready()) return;

    /* Enable auxiliary port on 8042 controller */
    ps2_wait_write();
    outb(PS2_CMD, 0xA8);

    /* Read controller config byte */
    ps2_wait_write();
    outb(PS2_CMD, 0x20);
    ps2_wait_read();
    uint8_t config = inb(PS2_DATA);

    /* Enable IRQ12 (bit 1), ensure mouse clock not disabled (clear bit 5) */
    config |= 0x02;
    config &= ~0x20;

    /* Write config back */
    ps2_wait_write();
    outb(PS2_CMD, 0x60);
    ps2_wait_write();
    outb(PS2_DATA, config);

    /* Reset mouse (set defaults) */
    mouse_write(0xF6);
    mouse_read();  /* ACK */

    /* Enable data reporting */
    mouse_write(0xF4);
    mouse_read();  /* ACK */

    /* Initialize cursor position to center of screen */
    mouse_x = fb_get_width() / 2;
    mouse_y = fb_get_height() / 2;
    mouse_buttons = 0;
    mouse_cycle = 0;
    mouse_changed = 0;
    mouse_installed = 1;
}

/* IRQ12 handler — called from idt.asm irq12_handler */
void mouse_handler(void) {
    uint8_t data = inb(PS2_DATA);

    if (!mouse_installed) return;

    mouse_bytes[mouse_cycle] = data;

    if (mouse_cycle == 0) {
        /* First byte must have bit 3 set (always-1 bit) */
        if (!(data & 0x08)) return;  /* out of sync, wait for valid first byte */
    }

    mouse_cycle++;
    if (mouse_cycle >= 3) {
        mouse_cycle = 0;

        /* Discard packets with overflow bits set */
        if (mouse_bytes[0] & 0xC0) return;

        /* Extract delta movement with sign extension */
        int dx = (int)mouse_bytes[1];
        int dy = (int)mouse_bytes[2];
        if (mouse_bytes[0] & 0x10) dx -= 256;
        if (mouse_bytes[0] & 0x20) dy -= 256;

        /* Mouse acceleration: amplify larger movements, scaled by sensitivity */
        int adx = dx < 0 ? -dx : dx;
        int ady = dy < 0 ? -dy : dy;
        int amx = adx >= 6 ? 4 : (adx >= 3 ? 2 : 1);
        int amy = ady >= 6 ? 4 : (ady >= 3 ? 2 : 1);
        /* Sensitivity: 0=half speed, 1=normal, 2=double */
        if (mouse_sensitivity == 0) { amx = (amx + 1) / 2; amy = (amy + 1) / 2; }
        else if (mouse_sensitivity >= 2) { amx *= 2; amy *= 2; }
        dx *= amx;
        dy *= amy;

        /* Update position (PS/2: Y positive = up, screen: Y positive = down) */
        mouse_x += dx;
        mouse_y -= dy;

        /* Clamp to screen bounds */
        int w = fb_get_width();
        int h = fb_get_height();
        if (mouse_x < 0) mouse_x = 0;
        if (mouse_y < 0) mouse_y = 0;
        if (mouse_x >= w) mouse_x = w - 1;
        if (mouse_y >= h) mouse_y = h - 1;

        /* Update button state */
        mouse_buttons = mouse_bytes[0] & 0x07;
        mouse_changed = 1;
    }
}

int mouse_get_x(void) { return mouse_x; }
int mouse_get_y(void) { return mouse_y; }
int mouse_get_buttons(void) { return mouse_buttons; }
int mouse_is_installed(void) { return mouse_installed; }

int mouse_poll_changed(void) {
    if (mouse_changed) {
        mouse_changed = 0;
        return 1;
    }
    return 0;
}

/* Inject mouse movement from USB HID driver */
void mouse_inject(int dx, int dy, int buttons) {
    mouse_x += dx;
    mouse_y += dy;

    /* Clamp to screen bounds */
    int w = fb_get_width();
    int h = fb_get_height();
    if (mouse_x < 0) mouse_x = 0;
    if (mouse_y < 0) mouse_y = 0;
    if (mouse_x >= w) mouse_x = w - 1;
    if (mouse_y >= h) mouse_y = h - 1;

    mouse_buttons = buttons;
    mouse_changed = 1;
    if (!mouse_installed) mouse_installed = 1;
}
