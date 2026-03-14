/* AiOS — PS/2 Mouse Driver */
#ifndef AIOS_MOUSE_H
#define AIOS_MOUSE_H

/* Initialize PS/2 mouse on the 8042 controller. Call after IDT setup. */
void mouse_init(void);

/* IRQ12 handler — called from assembly stub in idt.asm */
void mouse_handler(void);

/* Query current mouse state */
int mouse_get_x(void);
int mouse_get_y(void);
int mouse_get_buttons(void);  /* bit 0=left, 1=right, 2=middle */
int mouse_is_installed(void);

/* Check if mouse state changed since last call (resets flag) */
int mouse_poll_changed(void);

/* Inject mouse delta from USB HID (for USB mouse support) */
void mouse_inject(int dx, int dy, int buttons);

#endif
