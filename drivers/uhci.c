/* AiOS — UHCI (USB 1.1) Host Controller Driver
   Universal Host Controller Interface — Intel's USB 1.1 controller.
   Used by QEMU's default USB controller (-device usb-uhci).

   UHCI uses a frame list (1024 entries, each a pointer to a TD/QH chain)
   that the HC processes at 1ms intervals. We use synchronous transfers
   by polling the TD completion bits.

   PCI class 0C / subclass 03 / prog-if 00 = UHCI. */

#include "include/usb.h"
#include "include/pci.h"
#include "include/io.h"
#include "include/string.h"
#include "include/heap.h"
#include "include/stdio.h"

/* UHCI I/O registers (offsets from BAR4) */
#define UHCI_CMD        0x00   /* Command register */
#define UHCI_STS        0x02   /* Status register */
#define UHCI_INTR       0x04   /* Interrupt enable */
#define UHCI_FRNUM      0x06   /* Frame number */
#define UHCI_FLBASE     0x08   /* Frame list base address (physical) */
#define UHCI_SOF        0x0C   /* Start-of-frame modify */
#define UHCI_PORTSC1    0x10   /* Port 1 status/control */
#define UHCI_PORTSC2    0x12   /* Port 2 status/control */

/* Command register bits */
#define UHCI_CMD_RS     0x01   /* Run/Stop */
#define UHCI_CMD_HCRESET 0x02  /* Host Controller Reset */
#define UHCI_CMD_GRESET 0x04   /* Global Reset */
#define UHCI_CMD_MAXP   0x80   /* Max Packet (1=64 bytes, 0=32) */

/* Status register bits */
#define UHCI_STS_INT    0x01   /* USB Interrupt */
#define UHCI_STS_ERR    0x02   /* USB Error Interrupt */
#define UHCI_STS_RD     0x04   /* Resume Detect */
#define UHCI_STS_HSE    0x08   /* Host System Error */
#define UHCI_STS_HCPE   0x10   /* HC Process Error */
#define UHCI_STS_HCH    0x20   /* HC Halted */

/* Port status/control bits */
#define UHCI_PORT_CCS   0x0001 /* Current Connect Status */
#define UHCI_PORT_CSC   0x0002 /* Connect Status Change */
#define UHCI_PORT_PED   0x0004 /* Port Enable/Disable */
#define UHCI_PORT_PEDC  0x0008 /* Port Enable/Disable Change */
#define UHCI_PORT_LSS   0x0030 /* Line Status (D+/D-) */
#define UHCI_PORT_RD    0x0040 /* Resume Detect */
#define UHCI_PORT_LSDA  0x0100 /* Low Speed Device Attached */
#define UHCI_PORT_PR    0x0200 /* Port Reset */
#define UHCI_PORT_SUSP  0x1000 /* Suspend */

/* Transfer Descriptor (TD) — 32 bytes, 16-byte aligned */
typedef struct __attribute__((packed, aligned(16))) {
    uint32_t link;           /* pointer to next TD/QH, bit 0=T, bit 1=QH, bit 2=Vf */
    uint32_t control;        /* status, error count, etc. */
    uint32_t token;          /* PID, device addr, endpoint, data toggle, max len */
    uint32_t buffer;         /* data buffer physical address */
    /* Software fields (UHCI ignores these) */
    uint32_t _reserved[4];
} uhci_td_t;

/* Queue Head (QH) — 8 bytes, 16-byte aligned */
typedef struct __attribute__((packed, aligned(16))) {
    uint32_t head;           /* horizontal link */
    uint32_t element;        /* vertical link (first TD) */
    uint32_t _pad[2];
} uhci_qh_t;

/* TD link pointer flags */
#define TD_LINK_T    0x01    /* Terminate */
#define TD_LINK_QH   0x02    /* Points to QH (vs TD) */
#define TD_LINK_VF   0x04    /* Depth-first (vs breadth-first) */

/* TD control/status bits */
#define TD_CTRL_SPD    (1 << 29)   /* Short Packet Detect */
#define TD_CTRL_CERR(n) (((n) & 3) << 27) /* Error count */
#define TD_CTRL_LS     (1 << 26)   /* Low Speed */
#define TD_CTRL_ISO    (1 << 25)   /* Isochronous */
#define TD_CTRL_IOC    (1 << 24)   /* Interrupt on Complete */
#define TD_CTRL_ACTIVE (1 << 23)   /* Active */
#define TD_CTRL_STALL  (1 << 22)   /* Stalled */
#define TD_CTRL_DBERR  (1 << 21)   /* Data Buffer Error */
#define TD_CTRL_BABBLE (1 << 20)   /* Babble Detected */
#define TD_CTRL_NAK    (1 << 19)   /* NAK Received */
#define TD_CTRL_CRCTMO (1 << 18)   /* CRC/Timeout Error */
#define TD_CTRL_BITSTF (1 << 17)   /* Bitstuff Error */

/* TD token fields */
#define TD_TOKEN_PID(pid)     ((pid) & 0xFF)
#define TD_TOKEN_ADDR(addr)   (((addr) & 0x7F) << 8)
#define TD_TOKEN_EP(ep)       (((ep) & 0xF) << 15)
#define TD_TOKEN_TOGGLE(t)    (((t) & 1) << 19)
#define TD_TOKEN_MAXLEN(len)  ((((len) - 1) & 0x7FF) << 21)

/* PID values */
#define PID_SETUP  0x2D
#define PID_IN     0x69
#define PID_OUT    0xE1

/* Maximum TDs per transfer */
#define MAX_TDS  16

/* Driver state */
static uint16_t uhci_base;         /* I/O base address */
static uint32_t *frame_list;       /* 4KB-aligned frame list */
static uhci_td_t *td_pool;        /* Pre-allocated TD pool */
static uhci_qh_t *qh_pool;        /* Pre-allocated QH pool */
static int uhci_ready = 0;

extern void fb_print(const char *);
extern void fb_newline(void);

/* Polling delay — busy wait ~ms milliseconds */
static void delay_ms(int ms) {
    for (volatile int i = 0; i < ms * 10000; i++) ;
}

/* ========================================================================= */
/* Register access                                                           */
/* ========================================================================= */

static uint16_t uhci_read16(uint16_t reg) {
    return inw(uhci_base + reg);
}

static void uhci_write16(uint16_t reg, uint16_t val) {
    outw(uhci_base + reg, val);
}

static uint32_t uhci_read32(uint16_t reg) {
    return inl(uhci_base + reg);
}

static void uhci_write32(uint16_t reg, uint32_t val) {
    outl(uhci_base + reg, val);
}

/* ========================================================================= */
/* Initialization                                                            */
/* ========================================================================= */

int uhci_init(void) {
    pci_device_t pci;

    /* Find UHCI controller: class 0C (serial bus), subclass 03 (USB),
       prog-if 00 (UHCI) */
    if (!pci_find_class(0x0C, 0x03, 0x00, 0, 0, 0, &pci)) {
        return -1;  /* No UHCI controller found */
    }

    /* UHCI uses BAR4 (I/O space) */
    uhci_base = pci.bar[4] & 0xFFE0;
    if (uhci_base == 0) {
        /* Some systems use BAR0 */
        uhci_base = pci.bar[0] & 0xFFE0;
        if (uhci_base == 0) return -2;
    }

    /* Enable bus mastering and I/O space */
    pci_enable_bus_mastering(&pci);
    uint16_t cmd = pci_read16(pci.bus, pci.dev, pci.func, 0x04);
    cmd |= 0x01;  /* I/O space enable */
    pci_write16(pci.bus, pci.dev, pci.func, 0x04, cmd);

    /* Global reset */
    uhci_write16(UHCI_CMD, UHCI_CMD_GRESET);
    delay_ms(50);
    uhci_write16(UHCI_CMD, 0);
    delay_ms(10);

    /* Host controller reset */
    uhci_write16(UHCI_CMD, UHCI_CMD_HCRESET);
    for (int i = 0; i < 100; i++) {
        delay_ms(1);
        if (!(uhci_read16(UHCI_CMD) & UHCI_CMD_HCRESET)) break;
    }

    /* Allocate frame list (4KB aligned, 1024 entries) */
    frame_list = malloc(4096 + 4096);
    if (!frame_list) return -3;
    frame_list = (uint32_t *)(((uintptr_t)frame_list + 4095) & ~4095);

    /* Allocate TD and QH pools (16-byte aligned) */
    td_pool = malloc(sizeof(uhci_td_t) * MAX_TDS + 16);
    if (!td_pool) return -4;
    td_pool = (uhci_td_t *)(((uintptr_t)td_pool + 15) & ~15);

    qh_pool = malloc(sizeof(uhci_qh_t) * 4 + 16);
    if (!qh_pool) return -5;
    qh_pool = (uhci_qh_t *)(((uintptr_t)qh_pool + 15) & ~15);

    /* Initialize frame list — all entries point to terminate */
    for (int i = 0; i < 1024; i++)
        frame_list[i] = TD_LINK_T;

    /* Set up the HC */
    uhci_write16(UHCI_INTR, 0);                       /* Disable interrupts */
    uhci_write16(UHCI_FRNUM, 0);                       /* Reset frame number */
    uhci_write32(UHCI_FLBASE, (uint32_t)frame_list);   /* Set frame list base */
    uhci_write16(UHCI_STS, 0xFFFF);                    /* Clear status */

    /* Start the controller */
    uhci_write16(UHCI_CMD, UHCI_CMD_RS | UHCI_CMD_MAXP);

    uhci_ready = 1;
    return 0;
}

/* ========================================================================= */
/* Port operations                                                           */
/* ========================================================================= */

static int uhci_get_port_count(void) {
    return 2;  /* UHCI always has exactly 2 root ports */
}

static int uhci_port_connected(int port) {
    if (port < 0 || port >= 2) return 0;
    uint16_t reg = (port == 0) ? UHCI_PORTSC1 : UHCI_PORTSC2;
    return (uhci_read16(reg) & UHCI_PORT_CCS) ? 1 : 0;
}

static int uhci_port_reset(int port) {
    if (port < 0 || port >= 2) return -1;
    uint16_t reg = (port == 0) ? UHCI_PORTSC1 : UHCI_PORTSC2;

    /* Assert port reset */
    uint16_t val = uhci_read16(reg);
    uhci_write16(reg, val | UHCI_PORT_PR);
    delay_ms(50);

    /* De-assert reset */
    val = uhci_read16(reg);
    uhci_write16(reg, val & ~UHCI_PORT_PR);
    delay_ms(10);

    /* Clear change bits and enable port */
    for (int i = 0; i < 10; i++) {
        delay_ms(10);
        val = uhci_read16(reg);
        if (val & UHCI_PORT_CCS) {
            /* Enable port, clear change bits */
            uhci_write16(reg, val | UHCI_PORT_PED | UHCI_PORT_CSC | UHCI_PORT_PEDC);
            delay_ms(10);
            val = uhci_read16(reg);
            if (val & UHCI_PORT_PED) {
                /* Detect speed */
                return (val & UHCI_PORT_LSDA) ? USB_SPEED_LOW : USB_SPEED_FULL;
            }
        }
    }
    return -1;  /* Reset failed */
}

/* ========================================================================= */
/* Transfer execution — synchronous polling                                  */
/* ========================================================================= */

/* Build and execute a TD chain. Returns actual bytes transferred or <0 on error. */
static int uhci_execute_tds(uhci_td_t *tds, int count) {
    if (count == 0) return 0;

    /* Link TDs */
    for (int i = 0; i < count - 1; i++)
        tds[i].link = (uint32_t)&tds[i + 1] | TD_LINK_VF;
    tds[count - 1].link = TD_LINK_T;

    /* Set up a QH pointing to the first TD */
    uhci_qh_t *qh = &qh_pool[0];
    qh->head = TD_LINK_T;
    qh->element = (uint32_t)&tds[0];

    /* Insert QH into frame list at current frame */
    uint16_t frame = uhci_read16(UHCI_FRNUM) & 0x3FF;
    uint16_t target = (frame + 2) & 0x3FF;
    frame_list[target] = (uint32_t)qh | TD_LINK_QH;

    /* Poll for completion */
    int total = 0;
    for (int timeout = 0; timeout < 500; timeout++) {
        delay_ms(1);

        int all_done = 1;
        int error = 0;
        total = 0;

        for (int i = 0; i < count; i++) {
            uint32_t ctrl = tds[i].control;
            if (ctrl & TD_CTRL_ACTIVE) {
                all_done = 0;
                break;
            }
            if (ctrl & (TD_CTRL_STALL | TD_CTRL_DBERR | TD_CTRL_BABBLE | TD_CTRL_CRCTMO | TD_CTRL_BITSTF)) {
                error = 1;
                break;
            }
            /* Actual length is in bits 0-10 of control, +1 (0x7FF = no data) */
            int actlen = (ctrl & 0x7FF);
            if (actlen != 0x7FF) {
                total += actlen + 1;
            }

            /* Short packet — stop early */
            uint32_t token = tds[i].token;
            uint8_t pid = token & 0xFF;
            if (pid == PID_IN) {
                int maxlen = ((token >> 21) & 0x7FF) + 1;
                if (actlen + 1 < maxlen) break;
            }
        }

        if (error) {
            frame_list[target] = TD_LINK_T;
            return -1;
        }
        if (all_done) {
            frame_list[target] = TD_LINK_T;
            return total;
        }
    }

    /* Timeout — remove from schedule */
    frame_list[target] = TD_LINK_T;
    return -2;
}

/* ========================================================================= */
/* Control transfer                                                          */
/* ========================================================================= */

static int uhci_control(int port, int addr, int speed, int max_pkt,
                        usb_setup_t *setup, void *data, int len) {
    int td_count = 0;
    int toggle = 0;
    uhci_td_t *tds = td_pool;
    memset(tds, 0, sizeof(uhci_td_t) * MAX_TDS);

    uint32_t ls_flag = (speed == USB_SPEED_LOW) ? TD_CTRL_LS : 0;

    /* SETUP TD */
    tds[td_count].control = TD_CTRL_ACTIVE | TD_CTRL_CERR(3) | ls_flag;
    tds[td_count].token = TD_TOKEN_PID(PID_SETUP) | TD_TOKEN_ADDR(addr) |
                          TD_TOKEN_EP(0) | TD_TOKEN_TOGGLE(0) |
                          TD_TOKEN_MAXLEN(8);
    tds[td_count].buffer = (uint32_t)setup;
    td_count++;
    toggle = 1;

    /* DATA TDs */
    int remaining = len;
    uint8_t *buf = (uint8_t *)data;
    uint8_t pid = (setup->bmRequestType & USB_DIR_IN) ? PID_IN : PID_OUT;

    while (remaining > 0 && td_count < MAX_TDS - 1) {
        int chunk = remaining > max_pkt ? max_pkt : remaining;
        tds[td_count].control = TD_CTRL_ACTIVE | TD_CTRL_CERR(3) |
                                TD_CTRL_SPD | ls_flag;
        tds[td_count].token = TD_TOKEN_PID(pid) | TD_TOKEN_ADDR(addr) |
                              TD_TOKEN_EP(0) | TD_TOKEN_TOGGLE(toggle) |
                              TD_TOKEN_MAXLEN(chunk);
        tds[td_count].buffer = (uint32_t)buf;
        td_count++;
        buf += chunk;
        remaining -= chunk;
        toggle ^= 1;
    }

    /* STATUS TD (opposite direction from data) */
    uint8_t status_pid = (pid == PID_IN) ? PID_OUT : PID_IN;
    if (len == 0) status_pid = PID_IN;
    tds[td_count].control = TD_CTRL_ACTIVE | TD_CTRL_CERR(3) |
                            TD_CTRL_IOC | ls_flag;
    tds[td_count].token = TD_TOKEN_PID(status_pid) | TD_TOKEN_ADDR(addr) |
                          TD_TOKEN_EP(0) | TD_TOKEN_TOGGLE(1) |
                          TD_TOKEN_MAXLEN(0);
    tds[td_count].buffer = 0;
    /* For zero-length status, UHCI uses maxlen = 0x7FF (null packet) */
    tds[td_count].token = (tds[td_count].token & ~(0x7FF << 21)) | (0x7FF << 21);
    td_count++;

    return uhci_execute_tds(tds, td_count);
}

/* ========================================================================= */
/* Interrupt IN transfer                                                     */
/* ========================================================================= */

static int uhci_interrupt_in(int port, int addr, int speed, int ep,
                             int max_pkt, void *data, int len) {
    uhci_td_t *tds = td_pool;
    memset(tds, 0, sizeof(uhci_td_t));

    uint32_t ls_flag = (speed == USB_SPEED_LOW) ? TD_CTRL_LS : 0;

    int chunk = len > max_pkt ? max_pkt : len;
    tds[0].control = TD_CTRL_ACTIVE | TD_CTRL_CERR(3) |
                     TD_CTRL_SPD | TD_CTRL_IOC | ls_flag;
    tds[0].token = TD_TOKEN_PID(PID_IN) | TD_TOKEN_ADDR(addr) |
                   TD_TOKEN_EP(ep) | TD_TOKEN_TOGGLE(0) |
                   TD_TOKEN_MAXLEN(chunk);
    tds[0].buffer = (uint32_t)data;

    return uhci_execute_tds(tds, 1);
}

/* ========================================================================= */
/* Bulk IN/OUT transfers                                                     */
/* ========================================================================= */

static int uhci_bulk_in(int port, int addr, int speed, int ep,
                        int max_pkt, void *data, int len) {
    uhci_td_t *tds = td_pool;
    int td_count = 0;
    int toggle = 0;
    uint8_t *buf = (uint8_t *)data;
    int remaining = len;

    memset(tds, 0, sizeof(uhci_td_t) * MAX_TDS);

    while (remaining > 0 && td_count < MAX_TDS) {
        int chunk = remaining > max_pkt ? max_pkt : remaining;
        tds[td_count].control = TD_CTRL_ACTIVE | TD_CTRL_CERR(3) | TD_CTRL_SPD;
        tds[td_count].token = TD_TOKEN_PID(PID_IN) | TD_TOKEN_ADDR(addr) |
                              TD_TOKEN_EP(ep) | TD_TOKEN_TOGGLE(toggle) |
                              TD_TOKEN_MAXLEN(chunk);
        tds[td_count].buffer = (uint32_t)buf;
        td_count++;
        buf += chunk;
        remaining -= chunk;
        toggle ^= 1;
    }
    if (td_count > 0)
        tds[td_count - 1].control |= TD_CTRL_IOC;

    return uhci_execute_tds(tds, td_count);
}

static int uhci_bulk_out(int port, int addr, int speed, int ep,
                         int max_pkt, const void *data, int len) {
    uhci_td_t *tds = td_pool;
    int td_count = 0;
    int toggle = 0;
    const uint8_t *buf = (const uint8_t *)data;
    int remaining = len;

    memset(tds, 0, sizeof(uhci_td_t) * MAX_TDS);

    while (remaining > 0 && td_count < MAX_TDS) {
        int chunk = remaining > max_pkt ? max_pkt : remaining;
        tds[td_count].control = TD_CTRL_ACTIVE | TD_CTRL_CERR(3);
        tds[td_count].token = TD_TOKEN_PID(PID_OUT) | TD_TOKEN_ADDR(addr) |
                              TD_TOKEN_EP(ep) | TD_TOKEN_TOGGLE(toggle) |
                              TD_TOKEN_MAXLEN(chunk);
        tds[td_count].buffer = (uint32_t)buf;
        td_count++;
        buf += chunk;
        remaining -= chunk;
        toggle ^= 1;
    }
    if (td_count > 0)
        tds[td_count - 1].control |= TD_CTRL_IOC;

    return uhci_execute_tds(tds, td_count);
}

/* ========================================================================= */
/* HC operations table                                                       */
/* ========================================================================= */

usb_hc_ops_t uhci_ops = {
    .control       = uhci_control,
    .interrupt_in  = uhci_interrupt_in,
    .bulk_in       = uhci_bulk_in,
    .bulk_out      = uhci_bulk_out,
    .get_port_count = uhci_get_port_count,
    .port_connected = uhci_port_connected,
    .port_reset     = uhci_port_reset,
};

int uhci_is_ready(void) { return uhci_ready; }
