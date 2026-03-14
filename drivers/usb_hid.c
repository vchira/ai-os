/* AiOS — USB HID (Human Interface Device) Class Driver
   Supports boot protocol keyboards and mice.
   Boot protocol uses fixed report formats — no HID report descriptor parsing needed. */

#include "include/usb.h"
#include "include/string.h"
#include "include/stdio.h"

extern void fb_print(const char *);
extern void fb_newline(void);

/* Maximum tracked HID devices */
#define MAX_HID  4

typedef struct {
    usb_device_t *dev;
    int type;       /* USB_HID_PROTO_KEYBOARD or USB_HID_PROTO_MOUSE */
    uint8_t prev_keys[6]; /* previous key state for keyboard */
    uint8_t prev_buttons; /* previous button state for mouse */
} hid_device_t;

static hid_device_t hid_devs[MAX_HID];
static int hid_count = 0;

static char pbuf[128];

/* Keyboard scancode-to-ASCII map (USB HID usage IDs, boot protocol) */
static const char hid_key_lower[128] = {
    0, 0, 0, 0,                                    /* 00-03: reserved */
    'a','b','c','d','e','f','g','h','i','j','k','l','m', /* 04-10 */
    'n','o','p','q','r','s','t','u','v','w','x','y','z', /* 11-1D */
    '1','2','3','4','5','6','7','8','9','0',             /* 1E-27 */
    '\n', 27, 8, '\t', ' ',                              /* 28-2C: enter,esc,bksp,tab,space */
    '-','=','[',']','\\',                                /* 2D-31 */
    0, ';','\'','`',',','.','/',                         /* 32-38 */
    0, /* 39: caps lock */
    0,0,0,0,0,0,0,0,0,0, /* 3A-43: F1-F10 */
    0,0, /* 44-45: F11,F12 */
    0,0,0,0,0,0,0,0,0, /* 46-4E: misc */
    0,0, /* 4F-50: right,left arrows */
    0,0, /* 51-52: down,up arrows */
};

static const char hid_key_upper[128] = {
    0, 0, 0, 0,
    'A','B','C','D','E','F','G','H','I','J','K','L','M',
    'N','O','P','Q','R','S','T','U','V','W','X','Y','Z',
    '!','@','#','$','%','^','&','*','(',')',
    '\n', 27, 8, '\t', ' ',
    '_','+','{','}','|',
    0, ':','"','~','<','>','?',
};

/* Buffer for injecting USB keyboard input into the PS/2 keyboard buffer */
extern void keyboard_inject_char(char c);

/* Set boot protocol on HID device */
static void hid_set_boot_protocol(usb_device_t *dev) {
    usb_setup_t setup;
    setup.bmRequestType = USB_DIR_OUT | USB_TYPE_CLASS | USB_RECIP_INTERFACE;
    setup.bRequest = 0x0B;  /* SET_PROTOCOL */
    setup.wValue = 0;       /* 0 = boot protocol */
    setup.wIndex = dev->if_number;
    setup.wLength = 0;
    usb_control_msg(dev, &setup, NULL, 0);
}

/* Set idle rate to 0 (only report on change) */
static void hid_set_idle(usb_device_t *dev) {
    usb_setup_t setup;
    setup.bmRequestType = USB_DIR_OUT | USB_TYPE_CLASS | USB_RECIP_INTERFACE;
    setup.bRequest = 0x0A;  /* SET_IDLE */
    setup.wValue = 0;       /* infinite idle */
    setup.wIndex = dev->if_number;
    setup.wLength = 0;
    usb_control_msg(dev, &setup, NULL, 0);
}

void usb_hid_attach(usb_device_t *dev) {
    if (hid_count >= MAX_HID) return;
    if (dev->if_class != USB_CLASS_HID) return;
    if (dev->if_subclass != USB_HID_SUBCLASS_BOOT) return;

    hid_device_t *hid = &hid_devs[hid_count];
    memset(hid, 0, sizeof(*hid));
    hid->dev = dev;
    hid->type = dev->if_protocol;

    /* Set boot protocol */
    hid_set_boot_protocol(dev);
    hid_set_idle(dev);

    const char *type_str = "unknown";
    if (hid->type == USB_HID_PROTO_KEYBOARD) type_str = "keyboard";
    else if (hid->type == USB_HID_PROTO_MOUSE) type_str = "mouse";

    snprintf(pbuf, sizeof(pbuf), "[OK] USB HID %s attached (addr %d)",
             type_str, dev->address);
    fb_print(pbuf);
    fb_newline();

    hid_count++;
}

/* Poll keyboard — boot protocol report format:
   byte 0: modifier keys (bit0=LCtrl, bit1=LShift, etc.)
   byte 1: reserved
   bytes 2-7: key codes (up to 6 simultaneous keys) */
static void poll_keyboard(hid_device_t *hid) {
    uint8_t report[8];
    int ret = usb_interrupt_read(hid->dev, report, 8);
    if (ret < 8) return;

    uint8_t mods = report[0];
    int shift = (mods & 0x22);  /* LShift or RShift */

    /* Check for new key presses */
    for (int i = 2; i < 8; i++) {
        uint8_t key = report[i];
        if (key == 0 || key >= 128) continue;

        /* Check if this key was already pressed */
        int was_pressed = 0;
        for (int j = 0; j < 6; j++) {
            if (hid->prev_keys[j] == key) { was_pressed = 1; break; }
        }
        if (was_pressed) continue;

        /* New key press — convert to ASCII and inject */
        char ch = shift ? hid_key_upper[key] : hid_key_lower[key];
        if (ch) keyboard_inject_char(ch);
    }

    /* Save current state */
    memcpy(hid->prev_keys, &report[2], 6);
}

/* Poll mouse — boot protocol report format:
   byte 0: buttons (bit0=left, bit1=right, bit2=middle)
   byte 1: X movement (signed)
   byte 2: Y movement (signed) */
static void poll_mouse(hid_device_t *hid) {
    uint8_t report[4];
    int ret = usb_interrupt_read(hid->dev, report, 3);
    if (ret < 3) return;

    /* We have PS/2 mouse handling already — inject USB mouse data into
       the same mouse state. */
    extern int mouse_is_installed(void);
    extern void mouse_inject(int dx, int dy, int buttons);

    if (mouse_is_installed()) {
        int8_t dx = (int8_t)report[1];
        int8_t dy = (int8_t)report[2];
        mouse_inject(dx, -dy, report[0] & 0x07);
    }
}

void usb_hid_poll(void) {
    for (int i = 0; i < hid_count; i++) {
        if (!hid_devs[i].dev || !hid_devs[i].dev->active) continue;
        if (hid_devs[i].type == USB_HID_PROTO_KEYBOARD)
            poll_keyboard(&hid_devs[i]);
        else if (hid_devs[i].type == USB_HID_PROTO_MOUSE)
            poll_mouse(&hid_devs[i]);
    }
}
