/* AiOS — Bluetooth Stack
   HCI over USB + L2CAP + device discovery + HID profile. */
#ifndef AIOS_BLUETOOTH_H
#define AIOS_BLUETOOTH_H

#include "include/types.h"

/* Bluetooth address */
typedef struct {
    uint8_t b[6];
} __attribute__((packed)) bt_addr_t;

/* Discovered device */
#define BT_MAX_DEVICES  16
#define BT_NAME_MAX     32

typedef struct {
    bt_addr_t addr;
    char name[BT_NAME_MAX];
    uint32_t class_of_device;  /* major/minor device class */
    int8_t rssi;
    int active;
    int connected;
} bt_device_t;

/* Major device classes */
#define BT_COD_MAJOR_COMPUTER    0x01
#define BT_COD_MAJOR_PHONE       0x02
#define BT_COD_MAJOR_AUDIO       0x04
#define BT_COD_MAJOR_PERIPHERAL  0x05
#define BT_COD_MAJOR_IMAGING     0x06

/* Initialize Bluetooth subsystem (finds USB BT controller) */
void bt_init(void);

/* Start device discovery (inquiry). Non-blocking; results arrive via polling. */
int bt_start_discovery(void);

/* Stop discovery */
void bt_stop_discovery(void);

/* Poll for events (call periodically) */
void bt_poll(void);

/* Get discovered devices */
bt_device_t *bt_get_devices(void);
int bt_get_device_count(void);

/* Connect to a device by index */
int bt_connect(int dev_idx);

/* Disconnect a device */
void bt_disconnect(int dev_idx);

/* Check if BT controller is present */
int bt_is_ready(void);

#endif
