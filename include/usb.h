/* AiOS — USB Stack Core
   USB 1.1/2.0 host controller abstraction, device enumeration, descriptors. */
#ifndef AIOS_USB_H
#define AIOS_USB_H

#include "include/types.h"

/* USB speed constants */
#define USB_SPEED_LOW   0
#define USB_SPEED_FULL  1
#define USB_SPEED_HIGH  2

/* Standard request types */
#define USB_DIR_OUT         0x00
#define USB_DIR_IN          0x80
#define USB_TYPE_STANDARD   0x00
#define USB_TYPE_CLASS      0x20
#define USB_TYPE_VENDOR     0x40
#define USB_RECIP_DEVICE    0x00
#define USB_RECIP_INTERFACE 0x01
#define USB_RECIP_ENDPOINT  0x02

/* Standard requests */
#define USB_REQ_GET_STATUS        0
#define USB_REQ_CLEAR_FEATURE     1
#define USB_REQ_SET_FEATURE       3
#define USB_REQ_SET_ADDRESS       5
#define USB_REQ_GET_DESCRIPTOR    6
#define USB_REQ_SET_DESCRIPTOR    7
#define USB_REQ_GET_CONFIGURATION 8
#define USB_REQ_SET_CONFIGURATION 9
#define USB_REQ_GET_INTERFACE    10
#define USB_REQ_SET_INTERFACE    11

/* Descriptor types */
#define USB_DT_DEVICE         1
#define USB_DT_CONFIGURATION  2
#define USB_DT_STRING         3
#define USB_DT_INTERFACE      4
#define USB_DT_ENDPOINT       5
#define USB_DT_HID           0x21
#define USB_DT_HID_REPORT    0x22

/* USB class codes */
#define USB_CLASS_HID          3
#define USB_CLASS_MASS_STORAGE 8
#define USB_CLASS_HUB          9
#define USB_CLASS_WIRELESS    0xE0

/* HID subclass/protocol */
#define USB_HID_SUBCLASS_BOOT 1
#define USB_HID_PROTO_KEYBOARD 1
#define USB_HID_PROTO_MOUSE    2

/* Mass storage subclass/protocol */
#define USB_MSC_SUBCLASS_SCSI  6
#define USB_MSC_PROTO_BBB     0x50  /* Bulk-Only (BBB) */

/* Transfer types */
#define USB_ENDPOINT_XFER_CONTROL  0
#define USB_ENDPOINT_XFER_ISOC     1
#define USB_ENDPOINT_XFER_BULK     2
#define USB_ENDPOINT_XFER_INT      3

/* Setup packet */
typedef struct __attribute__((packed)) {
    uint8_t  bmRequestType;
    uint8_t  bRequest;
    uint16_t wValue;
    uint16_t wIndex;
    uint16_t wLength;
} usb_setup_t;

/* Device descriptor */
typedef struct __attribute__((packed)) {
    uint8_t  bLength;
    uint8_t  bDescriptorType;
    uint16_t bcdUSB;
    uint8_t  bDeviceClass;
    uint8_t  bDeviceSubClass;
    uint8_t  bDeviceProtocol;
    uint8_t  bMaxPacketSize0;
    uint16_t idVendor;
    uint16_t idProduct;
    uint16_t bcdDevice;
    uint8_t  iManufacturer;
    uint8_t  iProduct;
    uint8_t  iSerialNumber;
    uint8_t  bNumConfigurations;
} usb_device_desc_t;

/* Configuration descriptor */
typedef struct __attribute__((packed)) {
    uint8_t  bLength;
    uint8_t  bDescriptorType;
    uint16_t wTotalLength;
    uint8_t  bNumInterfaces;
    uint8_t  bConfigurationValue;
    uint8_t  iConfiguration;
    uint8_t  bmAttributes;
    uint8_t  bMaxPower;
} usb_config_desc_t;

/* Interface descriptor */
typedef struct __attribute__((packed)) {
    uint8_t bLength;
    uint8_t bDescriptorType;
    uint8_t bInterfaceNumber;
    uint8_t bAlternateSetting;
    uint8_t bNumEndpoints;
    uint8_t bInterfaceClass;
    uint8_t bInterfaceSubClass;
    uint8_t bInterfaceProtocol;
    uint8_t iInterface;
} usb_interface_desc_t;

/* Endpoint descriptor */
typedef struct __attribute__((packed)) {
    uint8_t  bLength;
    uint8_t  bDescriptorType;
    uint8_t  bEndpointAddress;
    uint8_t  bmAttributes;
    uint16_t wMaxPacketSize;
    uint8_t  bInterval;
} usb_endpoint_desc_t;

/* Maximum devices we track */
#define USB_MAX_DEVICES 16

/* USB device state */
typedef struct {
    int      active;
    uint8_t  address;        /* USB device address (1-127) */
    uint8_t  speed;          /* USB_SPEED_* */
    uint8_t  port;           /* root port (0-based) */
    uint8_t  max_packet;     /* EP0 max packet size */
    uint16_t vendor_id;
    uint16_t product_id;
    uint8_t  dev_class;
    uint8_t  dev_subclass;
    uint8_t  dev_protocol;
    int      controller;     /* which HC owns this device */

    /* Interface info (first interface) */
    uint8_t  if_class;
    uint8_t  if_subclass;
    uint8_t  if_protocol;
    uint8_t  if_number;

    /* Endpoints */
    uint8_t  ep_in;          /* interrupt/bulk IN endpoint address */
    uint8_t  ep_out;         /* interrupt/bulk OUT endpoint address */
    uint16_t ep_in_maxpkt;
    uint16_t ep_out_maxpkt;
    uint8_t  ep_in_interval; /* polling interval for interrupt EP */
} usb_device_t;

/* Host controller operations */
typedef struct {
    /* Control transfer. Returns bytes transferred or negative on error. */
    int (*control)(int port, int addr, int speed, int max_pkt,
                   usb_setup_t *setup, void *data, int len);
    /* Interrupt IN transfer. Returns bytes read or negative on error. */
    int (*interrupt_in)(int port, int addr, int speed, int ep,
                        int max_pkt, void *data, int len);
    /* Bulk IN transfer. Returns bytes read or negative on error. */
    int (*bulk_in)(int port, int addr, int speed, int ep,
                   int max_pkt, void *data, int len);
    /* Bulk OUT transfer. Returns bytes written or negative on error. */
    int (*bulk_out)(int port, int addr, int speed, int ep,
                    int max_pkt, const void *data, int len);
    /* Get number of root ports */
    int (*get_port_count)(void);
    /* Check if device connected on port */
    int (*port_connected)(int port);
    /* Reset port, returns speed */
    int (*port_reset)(int port);
} usb_hc_ops_t;

/* Initialize USB subsystem (scans PCI for HCs, enumerates devices) */
void usb_init(void);

/* Poll USB for new devices and status changes */
void usb_poll(void);

/* Get device list */
usb_device_t *usb_get_devices(void);
int usb_get_device_count(void);

/* Perform a control transfer on a device */
int usb_control_msg(usb_device_t *dev, usb_setup_t *setup, void *data, int len);

/* Interrupt IN transfer */
int usb_interrupt_read(usb_device_t *dev, void *data, int len);

/* Bulk transfers */
int usb_bulk_read(usb_device_t *dev, void *data, int len);
int usb_bulk_write(usb_device_t *dev, const void *data, int len);

#endif
