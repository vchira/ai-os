/* AiOS — USB Core
   Device enumeration, address assignment, descriptor parsing.
   Drives UHCI (and future OHCI/EHCI/xHCI) host controllers. */

#include "include/usb.h"
#include "include/string.h"
#include "include/heap.h"
#include "include/stdio.h"

extern void fb_print(const char *);
extern void fb_newline(void);
extern void fb_set_color(int);

/* Host controller ops (from uhci.c, etc.) */
extern usb_hc_ops_t uhci_ops;
extern int uhci_init(void);
extern int uhci_is_ready(void);

#define MAX_HCS 4
static usb_hc_ops_t *hcs[MAX_HCS];
static int hc_count = 0;

/* Device pool */
static usb_device_t devices[USB_MAX_DEVICES];
static int next_address = 1;   /* next USB device address (1-127) */

static char pbuf[128];

/* ========================================================================= */
/* USB control helpers                                                       */
/* ========================================================================= */

static int usb_get_descriptor(usb_hc_ops_t *hc, int port, int addr, int speed,
                              int max_pkt, uint8_t type, uint8_t index,
                              void *buf, int len) {
    usb_setup_t setup;
    setup.bmRequestType = USB_DIR_IN | USB_TYPE_STANDARD | USB_RECIP_DEVICE;
    setup.bRequest = USB_REQ_GET_DESCRIPTOR;
    setup.wValue = (type << 8) | index;
    setup.wIndex = 0;
    setup.wLength = len;
    return hc->control(port, addr, speed, max_pkt, &setup, buf, len);
}

static int usb_set_address(usb_hc_ops_t *hc, int port, int speed,
                           int max_pkt, int new_addr) {
    usb_setup_t setup;
    setup.bmRequestType = USB_DIR_OUT | USB_TYPE_STANDARD | USB_RECIP_DEVICE;
    setup.bRequest = USB_REQ_SET_ADDRESS;
    setup.wValue = new_addr;
    setup.wIndex = 0;
    setup.wLength = 0;
    return hc->control(port, 0, speed, max_pkt, &setup, NULL, 0);
}

static int usb_set_configuration(usb_hc_ops_t *hc, int port, int addr,
                                 int speed, int max_pkt, int config) {
    usb_setup_t setup;
    setup.bmRequestType = USB_DIR_OUT | USB_TYPE_STANDARD | USB_RECIP_DEVICE;
    setup.bRequest = USB_REQ_SET_CONFIGURATION;
    setup.wValue = config;
    setup.wIndex = 0;
    setup.wLength = 0;
    return hc->control(port, addr, speed, max_pkt, &setup, NULL, 0);
}

/* ========================================================================= */
/* Device enumeration                                                        */
/* ========================================================================= */

/* Find a free device slot */
static usb_device_t *alloc_device(void) {
    for (int i = 0; i < USB_MAX_DEVICES; i++)
        if (!devices[i].active) return &devices[i];
    return NULL;
}

/* Parse configuration/interface/endpoint descriptors from raw data */
static void parse_config(usb_device_t *dev, const uint8_t *buf, int len) {
    int pos = 0;
    int found_if = 0;

    while (pos + 2 <= len) {
        uint8_t dlen = buf[pos];
        uint8_t dtype = buf[pos + 1];

        if (dlen < 2 || pos + dlen > len) break;

        if (dtype == USB_DT_INTERFACE && !found_if) {
            if (dlen >= 9) {
                usb_interface_desc_t *ifd = (usb_interface_desc_t *)&buf[pos];
                dev->if_class = ifd->bInterfaceClass;
                dev->if_subclass = ifd->bInterfaceSubClass;
                dev->if_protocol = ifd->bInterfaceProtocol;
                dev->if_number = ifd->bInterfaceNumber;
                found_if = 1;
            }
        } else if (dtype == USB_DT_ENDPOINT && found_if) {
            if (dlen >= 7) {
                usb_endpoint_desc_t *epd = (usb_endpoint_desc_t *)&buf[pos];
                uint8_t dir = epd->bEndpointAddress & 0x80;
                uint8_t xfer = epd->bmAttributes & 0x03;
                uint8_t epnum = epd->bEndpointAddress & 0x0F;

                if (dir && (xfer == USB_ENDPOINT_XFER_INT || xfer == USB_ENDPOINT_XFER_BULK)) {
                    /* IN endpoint */
                    if (!dev->ep_in) {
                        dev->ep_in = epd->bEndpointAddress;
                        dev->ep_in_maxpkt = epd->wMaxPacketSize;
                        dev->ep_in_interval = epd->bInterval;
                    }
                } else if (!dir && (xfer == USB_ENDPOINT_XFER_INT || xfer == USB_ENDPOINT_XFER_BULK)) {
                    /* OUT endpoint */
                    if (!dev->ep_out) {
                        dev->ep_out = epd->bEndpointAddress;
                        dev->ep_out_maxpkt = epd->wMaxPacketSize;
                    }
                }
                (void)epnum;
            }
        }

        pos += dlen;
    }
}

static int enumerate_port(usb_hc_ops_t *hc, int hc_idx, int port) {
    if (!hc->port_connected(port)) return 0;

    /* Reset port */
    int speed = hc->port_reset(port);
    if (speed < 0) return -1;

    /* Get device descriptor (first 8 bytes to learn max packet size) */
    uint8_t desc8[8];
    int ret = usb_get_descriptor(hc, port, 0, speed, 8,
                                 USB_DT_DEVICE, 0, desc8, 8);
    if (ret < 8) return -2;

    int max_pkt = desc8[7];  /* bMaxPacketSize0 */
    if (max_pkt == 0) max_pkt = 8;

    /* Assign address */
    int addr = next_address++;
    if (addr > 127) return -3;

    ret = usb_set_address(hc, port, speed, max_pkt, addr);
    if (ret < 0) return -4;

    /* Small delay after SET_ADDRESS */
    for (volatile int i = 0; i < 50000; i++) ;

    /* Get full device descriptor */
    usb_device_desc_t dev_desc;
    ret = usb_get_descriptor(hc, port, addr, speed, max_pkt,
                             USB_DT_DEVICE, 0, &dev_desc, sizeof(dev_desc));
    if (ret < (int)sizeof(dev_desc)) {
        /* Try with smaller size */
        ret = usb_get_descriptor(hc, port, addr, speed, max_pkt,
                                 USB_DT_DEVICE, 0, &dev_desc, 18);
    }

    /* Get configuration descriptor */
    uint8_t config_buf[256];
    /* First get just the header to know total length */
    usb_config_desc_t *cfg = (usb_config_desc_t *)config_buf;
    ret = usb_get_descriptor(hc, port, addr, speed, max_pkt,
                             USB_DT_CONFIGURATION, 0, config_buf, 9);
    if (ret < 9) return -5;

    int total_len = cfg->wTotalLength;
    if (total_len > (int)sizeof(config_buf)) total_len = sizeof(config_buf);

    /* Get full configuration */
    ret = usb_get_descriptor(hc, port, addr, speed, max_pkt,
                             USB_DT_CONFIGURATION, 0, config_buf, total_len);

    /* Set configuration */
    usb_set_configuration(hc, port, addr, speed, max_pkt, cfg->bConfigurationValue);

    /* Create device entry */
    usb_device_t *dev = alloc_device();
    if (!dev) return -6;

    memset(dev, 0, sizeof(*dev));
    dev->active = 1;
    dev->address = addr;
    dev->speed = speed;
    dev->port = port;
    dev->max_packet = max_pkt;
    dev->vendor_id = dev_desc.idVendor;
    dev->product_id = dev_desc.idProduct;
    dev->dev_class = dev_desc.bDeviceClass;
    dev->dev_subclass = dev_desc.bDeviceSubClass;
    dev->dev_protocol = dev_desc.bDeviceProtocol;
    dev->controller = hc_idx;

    /* Parse interfaces and endpoints */
    parse_config(dev, config_buf, ret > 0 ? ret : total_len);

    snprintf(pbuf, sizeof(pbuf),
             "[OK] USB dev %d: %04X:%04X class %02X/%02X/%02X",
             addr, dev->vendor_id, dev->product_id,
             dev->if_class, dev->if_subclass, dev->if_protocol);
    fb_print(pbuf);
    fb_newline();

    return 1;
}

/* ========================================================================= */
/* USB class driver callbacks                                                */
/* ========================================================================= */

/* Defined in usb_hid.c */
extern void usb_hid_attach(usb_device_t *dev);
extern void usb_hid_poll(void);

/* Defined in usb_storage.c */
extern void usb_msc_attach(usb_device_t *dev);

static void attach_class_drivers(void) {
    for (int i = 0; i < USB_MAX_DEVICES; i++) {
        usb_device_t *dev = &devices[i];
        if (!dev->active) continue;

        if (dev->if_class == USB_CLASS_HID) {
            usb_hid_attach(dev);
        } else if (dev->if_class == USB_CLASS_MASS_STORAGE) {
            usb_msc_attach(dev);
        }
    }
}

/* ========================================================================= */
/* Public API                                                                */
/* ========================================================================= */

void usb_init(void) {
    memset(devices, 0, sizeof(devices));
    next_address = 1;
    hc_count = 0;

    /* Try UHCI */
    if (uhci_init() == 0) {
        hcs[hc_count] = &uhci_ops;
        hc_count++;
        fb_print("[OK] UHCI USB controller initialized");
        fb_newline();
    }

    /* Enumerate all ports on all controllers */
    for (int h = 0; h < hc_count; h++) {
        int ports = hcs[h]->get_port_count();
        for (int p = 0; p < ports; p++) {
            enumerate_port(hcs[h], h, p);
        }
    }

    /* Attach class drivers */
    if (usb_get_device_count() > 0)
        attach_class_drivers();
}

void usb_poll(void) {
    /* Poll HID devices */
    usb_hid_poll();
}

usb_device_t *usb_get_devices(void) {
    return devices;
}

int usb_get_device_count(void) {
    int count = 0;
    for (int i = 0; i < USB_MAX_DEVICES; i++)
        if (devices[i].active) count++;
    return count;
}

int usb_control_msg(usb_device_t *dev, usb_setup_t *setup, void *data, int len) {
    if (!dev || !dev->active || dev->controller >= hc_count) return -1;
    return hcs[dev->controller]->control(dev->port, dev->address, dev->speed,
                                          dev->max_packet, setup, data, len);
}

int usb_interrupt_read(usb_device_t *dev, void *data, int len) {
    if (!dev || !dev->active || !dev->ep_in || dev->controller >= hc_count) return -1;
    int ep = dev->ep_in & 0x0F;
    return hcs[dev->controller]->interrupt_in(dev->port, dev->address, dev->speed,
                                               ep, dev->ep_in_maxpkt, data, len);
}

int usb_bulk_read(usb_device_t *dev, void *data, int len) {
    if (!dev || !dev->active || !dev->ep_in || dev->controller >= hc_count) return -1;
    int ep = dev->ep_in & 0x0F;
    return hcs[dev->controller]->bulk_in(dev->port, dev->address, dev->speed,
                                          ep, dev->ep_in_maxpkt, data, len);
}

int usb_bulk_write(usb_device_t *dev, const void *data, int len) {
    if (!dev || !dev->active || !dev->ep_out || dev->controller >= hc_count) return -1;
    int ep = dev->ep_out & 0x0F;
    return hcs[dev->controller]->bulk_out(dev->port, dev->address, dev->speed,
                                           ep, dev->ep_out_maxpkt, data, len);
}
