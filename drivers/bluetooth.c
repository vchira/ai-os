/* AiOS — Bluetooth Stack (HCI over USB)
   Implements:
   - HCI command/event transport over USB bulk endpoints
   - Device discovery (HCI Inquiry)
   - Remote name request
   - Connection management
   - Basic L2CAP signaling

   Bluetooth controllers appear as USB class 0xE0 (Wireless),
   subclass 0x01 (RF), protocol 0x01 (Bluetooth). */

#include "include/bluetooth.h"
#include "include/usb.h"
#include "include/string.h"
#include "include/heap.h"
#include "include/stdio.h"

extern void fb_print(const char *);
extern void fb_newline(void);

/* ========================================================================= */
/* HCI packet types and commands                                             */
/* ========================================================================= */

/* HCI packet indicators (for USB, these map to endpoints) */
#define HCI_CMD_PKT        0x01
#define HCI_ACL_PKT        0x02
#define HCI_EVT_PKT        0x04

/* HCI commands (OGF << 10 | OCF) */
#define HCI_RESET                    0x0C03
#define HCI_READ_BD_ADDR             0x1009
#define HCI_READ_LOCAL_NAME          0x0C14
#define HCI_WRITE_SCAN_ENABLE       0x0C1A
#define HCI_INQUIRY                  0x0401
#define HCI_INQUIRY_CANCEL           0x0402
#define HCI_REMOTE_NAME_REQ          0x0419
#define HCI_CREATE_CONNECTION        0x0405
#define HCI_DISCONNECT               0x0406
#define HCI_WRITE_CLASS_OF_DEVICE    0x0C24
#define HCI_SET_EVENT_FILTER         0x0C05

/* HCI events */
#define HCI_EVT_INQUIRY_COMPLETE     0x01
#define HCI_EVT_INQUIRY_RESULT       0x02
#define HCI_EVT_CONN_COMPLETE        0x03
#define HCI_EVT_CONN_REQUEST         0x04
#define HCI_EVT_DISCONN_COMPLETE     0x05
#define HCI_EVT_REMOTE_NAME          0x07
#define HCI_EVT_CMD_COMPLETE         0x0E
#define HCI_EVT_CMD_STATUS           0x0F
#define HCI_EVT_INQUIRY_RESULT_RSSI  0x22
#define HCI_EVT_EXT_INQUIRY_RESULT   0x2F

/* Scan enable values */
#define SCAN_DISABLED      0x00
#define SCAN_INQUIRY       0x01
#define SCAN_PAGE          0x02
#define SCAN_BOTH          0x03

/* HCI command header */
typedef struct __attribute__((packed)) {
    uint16_t opcode;
    uint8_t  param_len;
} hci_cmd_hdr_t;

/* HCI event header */
typedef struct __attribute__((packed)) {
    uint8_t  event;
    uint8_t  param_len;
} hci_evt_hdr_t;

/* ========================================================================= */
/* Driver state                                                              */
/* ========================================================================= */

static usb_device_t *bt_usb_dev = NULL;
static bt_device_t bt_devices[BT_MAX_DEVICES];
static bt_addr_t local_addr;
static int bt_ready = 0;
static int bt_discovering = 0;
static uint8_t cmd_buf[256] __attribute__((aligned(4)));
static uint8_t evt_buf[256] __attribute__((aligned(4)));
static char pbuf[128];

/* ========================================================================= */
/* HCI USB transport                                                         */
/* ========================================================================= */

/* Send HCI command via USB control endpoint (endpoint 0) */
static int hci_send_cmd(uint16_t opcode, const void *params, int plen) {
    if (!bt_usb_dev) return -1;

    /* HCI commands go via control transfer to endpoint 0, class request */
    hci_cmd_hdr_t *hdr = (hci_cmd_hdr_t *)cmd_buf;
    hdr->opcode = opcode;
    hdr->param_len = plen;
    if (params && plen > 0)
        memcpy(cmd_buf + 3, params, plen);

    usb_setup_t setup;
    setup.bmRequestType = USB_DIR_OUT | USB_TYPE_CLASS | USB_RECIP_DEVICE;
    setup.bRequest = 0;
    setup.wValue = 0;
    setup.wIndex = 0;
    setup.wLength = 3 + plen;

    return usb_control_msg(bt_usb_dev, &setup, cmd_buf, 3 + plen);
}

/* Read HCI event from USB interrupt endpoint */
static int hci_read_event(uint8_t *buf, int max, int timeout_ms) {
    if (!bt_usb_dev) return -1;

    for (int i = 0; i < timeout_ms; i++) {
        int ret = usb_interrupt_read(bt_usb_dev, buf, max);
        if (ret > 0) return ret;
        for (volatile int j = 0; j < 10000; j++) ;  /* ~1ms */
    }
    return -1;  /* timeout */
}

/* Wait for a specific command complete event */
static int hci_wait_cmd_complete(uint16_t opcode, uint8_t *params, int max_params) {
    for (int attempt = 0; attempt < 50; attempt++) {
        int ret = hci_read_event(evt_buf, sizeof(evt_buf), 100);
        if (ret < 4) continue;

        hci_evt_hdr_t *hdr = (hci_evt_hdr_t *)evt_buf;

        if (hdr->event == HCI_EVT_CMD_COMPLETE) {
            /* cmd complete: [num_packets(1)] [opcode(2)] [status(1)] [params...] */
            uint16_t op = evt_buf[3] | (evt_buf[4] << 8);
            if (op == opcode) {
                int plen = hdr->param_len - 3;  /* minus num_pkts + opcode */
                if (plen > 0 && params) {
                    if (plen > max_params) plen = max_params;
                    memcpy(params, &evt_buf[5], plen);
                }
                return evt_buf[5];  /* status byte */
            }
        } else if (hdr->event == HCI_EVT_CMD_STATUS) {
            uint16_t op = evt_buf[4] | (evt_buf[5] << 8);
            if (op == opcode) {
                return evt_buf[2];  /* status */
            }
        }
    }
    return -1;
}

/* ========================================================================= */
/* Device tracking                                                           */
/* ========================================================================= */

static bt_device_t *find_or_add_device(bt_addr_t *addr) {
    /* Check existing */
    for (int i = 0; i < BT_MAX_DEVICES; i++) {
        if (bt_devices[i].active &&
            memcmp(&bt_devices[i].addr, addr, 6) == 0)
            return &bt_devices[i];
    }
    /* Add new */
    for (int i = 0; i < BT_MAX_DEVICES; i++) {
        if (!bt_devices[i].active) {
            memset(&bt_devices[i], 0, sizeof(bt_device_t));
            memcpy(&bt_devices[i].addr, addr, 6);
            bt_devices[i].active = 1;
            return &bt_devices[i];
        }
    }
    return NULL;
}

/* ========================================================================= */
/* Event processing                                                          */
/* ========================================================================= */

static void process_inquiry_result(uint8_t *data, int len) {
    if (len < 1) return;
    int num_responses = data[0];

    /* Standard inquiry result: each response is 14 bytes after count */
    uint8_t *p = &data[1];
    for (int i = 0; i < num_responses && (p - data) + 14 <= len; i++) {
        bt_addr_t addr;
        memcpy(&addr, p, 6);

        bt_device_t *dev = find_or_add_device(&addr);
        if (dev) {
            /* Class of device at offset 9 (3 bytes, little-endian) */
            dev->class_of_device = p[9] | (p[10] << 8) | (p[11] << 16);

            /* Request remote name */
            uint8_t name_params[10];
            memcpy(name_params, &addr, 6);
            name_params[6] = 0x01;  /* page_scan_repetition_mode */
            name_params[7] = 0;     /* reserved */
            name_params[8] = 0;     /* clock_offset low */
            name_params[9] = 0;     /* clock_offset high */
            hci_send_cmd(HCI_REMOTE_NAME_REQ, name_params, 10);
        }
        p += 14;
    }
}

static void process_inquiry_result_rssi(uint8_t *data, int len) {
    if (len < 1) return;
    int num = data[0];

    uint8_t *p = &data[1];
    for (int i = 0; i < num && (p - data) + 15 <= len; i++) {
        bt_addr_t addr;
        memcpy(&addr, p, 6);

        bt_device_t *dev = find_or_add_device(&addr);
        if (dev) {
            dev->class_of_device = p[9] | (p[10] << 8) | (p[11] << 16);
            dev->rssi = (int8_t)p[14];

            /* Request name */
            uint8_t name_params[10];
            memcpy(name_params, &addr, 6);
            memset(&name_params[6], 0, 4);
            name_params[6] = 0x01;
            hci_send_cmd(HCI_REMOTE_NAME_REQ, name_params, 10);
        }
        p += 15;
    }
}

static void process_remote_name(uint8_t *data, int len) {
    if (len < 7) return;
    uint8_t status = data[0];
    if (status != 0) return;

    bt_addr_t addr;
    memcpy(&addr, &data[1], 6);

    bt_device_t *dev = find_or_add_device(&addr);
    if (dev) {
        int name_len = len - 7;
        if (name_len > BT_NAME_MAX - 1) name_len = BT_NAME_MAX - 1;
        memcpy(dev->name, &data[7], name_len);
        dev->name[name_len] = '\0';
        /* Trim at first null in case name is shorter */
        for (int i = 0; i < name_len; i++) {
            if (dev->name[i] == '\0') break;
        }

        snprintf(pbuf, sizeof(pbuf), "  BT: %02X:%02X:%02X:%02X:%02X:%02X  %s",
                 addr.b[5], addr.b[4], addr.b[3],
                 addr.b[2], addr.b[1], addr.b[0], dev->name);
        fb_print(pbuf);
        fb_newline();
    }
}

static void process_event(uint8_t *buf, int len) {
    if (len < 2) return;
    hci_evt_hdr_t *hdr = (hci_evt_hdr_t *)buf;
    uint8_t *params = &buf[2];
    int plen = hdr->param_len;

    switch (hdr->event) {
        case HCI_EVT_INQUIRY_RESULT:
            process_inquiry_result(params, plen);
            break;

        case HCI_EVT_INQUIRY_RESULT_RSSI:
            process_inquiry_result_rssi(params, plen);
            break;

        case HCI_EVT_REMOTE_NAME:
            process_remote_name(params, plen);
            break;

        case HCI_EVT_INQUIRY_COMPLETE:
            bt_discovering = 0;
            break;

        case HCI_EVT_CONN_COMPLETE:
            if (plen >= 11) {
                uint8_t status = params[0];
                bt_addr_t addr;
                memcpy(&addr, &params[3], 6);
                bt_device_t *dev = find_or_add_device(&addr);
                if (dev && status == 0) {
                    dev->connected = 1;
                }
            }
            break;

        case HCI_EVT_DISCONN_COMPLETE:
            /* Mark device disconnected — would need connection handle mapping */
            break;
    }
}

/* ========================================================================= */
/* Public API                                                                */
/* ========================================================================= */

void bt_init(void) {
    memset(bt_devices, 0, sizeof(bt_devices));
    bt_ready = 0;
    bt_discovering = 0;

    /* Find USB Bluetooth controller (class E0/01/01) */
    usb_device_t *devs = usb_get_devices();
    for (int i = 0; i < USB_MAX_DEVICES; i++) {
        if (!devs[i].active) continue;
        if (devs[i].if_class == USB_CLASS_WIRELESS &&
            devs[i].if_subclass == 0x01 && devs[i].if_protocol == 0x01) {
            bt_usb_dev = &devs[i];
            break;
        }
    }

    if (!bt_usb_dev) return;  /* No BT controller found */

    /* Reset HCI */
    hci_send_cmd(HCI_RESET, NULL, 0);
    uint8_t resp[8];
    if (hci_wait_cmd_complete(HCI_RESET, resp, sizeof(resp)) != 0) {
        bt_usb_dev = NULL;
        return;
    }

    /* Read local BD address */
    hci_send_cmd(HCI_READ_BD_ADDR, NULL, 0);
    if (hci_wait_cmd_complete(HCI_READ_BD_ADDR, resp, 7) == 0) {
        memcpy(&local_addr, &resp[1], 6);
    }

    /* Enable inquiry + page scan */
    uint8_t scan = SCAN_BOTH;
    hci_send_cmd(HCI_WRITE_SCAN_ENABLE, &scan, 1);
    hci_wait_cmd_complete(HCI_WRITE_SCAN_ENABLE, NULL, 0);

    bt_ready = 1;

    snprintf(pbuf, sizeof(pbuf),
             "[OK] Bluetooth initialized (%02X:%02X:%02X:%02X:%02X:%02X)",
             local_addr.b[5], local_addr.b[4], local_addr.b[3],
             local_addr.b[2], local_addr.b[1], local_addr.b[0]);
    fb_print(pbuf);
    fb_newline();
}

int bt_start_discovery(void) {
    if (!bt_ready) return -1;

    /* Clear previous results */
    for (int i = 0; i < BT_MAX_DEVICES; i++) {
        if (!bt_devices[i].connected)
            bt_devices[i].active = 0;
    }

    /* HCI Inquiry: LAP=GIAC (0x9E8B33), duration=8 (10.24s), max_responses=0 (unlimited) */
    uint8_t params[5];
    params[0] = 0x33;  /* LAP byte 0 */
    params[1] = 0x8B;  /* LAP byte 1 */
    params[2] = 0x9E;  /* LAP byte 2 (GIAC) */
    params[3] = 8;     /* inquiry length (8 * 1.28s = ~10s) */
    params[4] = 0;     /* max responses (0 = unlimited) */

    int ret = hci_send_cmd(HCI_INQUIRY, params, 5);
    if (ret < 0) return -2;

    bt_discovering = 1;

    fb_print("Scanning for Bluetooth devices...");
    fb_newline();

    return 0;
}

void bt_stop_discovery(void) {
    if (!bt_ready || !bt_discovering) return;
    hci_send_cmd(HCI_INQUIRY_CANCEL, NULL, 0);
    hci_wait_cmd_complete(HCI_INQUIRY_CANCEL, NULL, 0);
    bt_discovering = 0;
}

void bt_poll(void) {
    if (!bt_ready) return;

    /* Try to read events (non-blocking) */
    int ret = hci_read_event(evt_buf, sizeof(evt_buf), 1);
    if (ret > 0) {
        process_event(evt_buf, ret);
    }
}

bt_device_t *bt_get_devices(void) {
    return bt_devices;
}

int bt_get_device_count(void) {
    int count = 0;
    for (int i = 0; i < BT_MAX_DEVICES; i++)
        if (bt_devices[i].active) count++;
    return count;
}

int bt_connect(int dev_idx) {
    if (!bt_ready || dev_idx < 0 || dev_idx >= BT_MAX_DEVICES) return -1;
    if (!bt_devices[dev_idx].active) return -2;

    /* HCI Create Connection */
    uint8_t params[13];
    memcpy(params, &bt_devices[dev_idx].addr, 6);
    params[6] = 0x18;  /* packet type (DM1|DH1) low */
    params[7] = 0xCC;  /* packet type high */
    params[8] = 0x01;  /* page scan repetition mode */
    params[9] = 0;     /* reserved */
    params[10] = 0;    /* clock offset low */
    params[11] = 0;    /* clock offset high */
    params[12] = 0;    /* allow role switch */

    return hci_send_cmd(HCI_CREATE_CONNECTION, params, 13);
}

void bt_disconnect(int dev_idx) {
    if (!bt_ready || dev_idx < 0 || dev_idx >= BT_MAX_DEVICES) return;
    if (!bt_devices[dev_idx].connected) return;

    /* Would need connection handle — simplified version */
    bt_devices[dev_idx].connected = 0;
}

int bt_is_ready(void) { return bt_ready; }
