/* AiOS — ARM VideoCore Mailbox Driver (Raspberry Pi)
   The mailbox is the communication channel between ARM and VideoCore GPU.
   Used for framebuffer setup, power management, clock configuration, etc.

   Mailbox 0 = GPU → ARM (read)
   Mailbox 1 = ARM → GPU (write) */

#include "include/types.h"
#include "include/string.h"

#ifndef PERIPH_BASE
#define PERIPH_BASE  0x3F000000
#endif

#define MBOX_BASE    (PERIPH_BASE + 0xB880)

#define MBOX_READ    (*(volatile uint32_t *)(MBOX_BASE + 0x00))
#define MBOX_STATUS  (*(volatile uint32_t *)(MBOX_BASE + 0x18))
#define MBOX_WRITE   (*(volatile uint32_t *)(MBOX_BASE + 0x20))

#define MBOX_FULL    0x80000000
#define MBOX_EMPTY   0x40000000

/* Property channel (channel 8) */
#define MBOX_CHANNEL_PROP  8

/* Property tags */
#define MBOX_TAG_GETSERIAL    0x00010004
#define MBOX_TAG_GETCLKRATE   0x00030002
#define MBOX_TAG_SETFBPHYS    0x00048003
#define MBOX_TAG_SETFBVIRT    0x00048004
#define MBOX_TAG_SETFBDEPTH   0x00048005
#define MBOX_TAG_SETFBPOFF    0x00048009
#define MBOX_TAG_GETFBPITCH   0x00040008
#define MBOX_TAG_ALLOCFB      0x00040001
#define MBOX_TAG_LAST         0

/* 16-byte aligned mailbox buffer */
static volatile uint32_t __attribute__((aligned(16))) mbox_buf[36];

void mailbox_init(void) {
    /* Nothing needed at init */
}

/* Send a message on the property channel and wait for response */
int mailbox_call(void) {
    /* Combine buffer address with channel (must be 16-byte aligned) */
    uint32_t msg = ((uint32_t)(uintptr_t)&mbox_buf & ~0xF) | MBOX_CHANNEL_PROP;

    /* Wait for mailbox to not be full */
    while (MBOX_STATUS & MBOX_FULL) ;

    /* Write message */
    MBOX_WRITE = msg;

    /* Wait for response */
    while (1) {
        while (MBOX_STATUS & MBOX_EMPTY) ;
        uint32_t resp = MBOX_READ;
        if (resp == msg) {
            /* Check response code */
            return (mbox_buf[1] == 0x80000000) ? 0 : -1;
        }
    }
}

/* Get ARM memory clock rate (for UART baud rate calculation) */
uint32_t mailbox_get_clock_rate(uint32_t clock_id) {
    mbox_buf[0] = 9 * 4;           /* buffer size */
    mbox_buf[1] = 0;               /* request code */
    mbox_buf[2] = MBOX_TAG_GETCLKRATE;
    mbox_buf[3] = 8;               /* value buffer size */
    mbox_buf[4] = 0;               /* request/response indicator */
    mbox_buf[5] = clock_id;        /* clock ID */
    mbox_buf[6] = 0;               /* rate (response) */
    mbox_buf[7] = MBOX_TAG_LAST;
    mbox_buf[8] = 0;

    if (mailbox_call() == 0)
        return mbox_buf[6];
    return 0;
}

/* Get board serial number */
uint64_t mailbox_get_serial(void) {
    mbox_buf[0] = 8 * 4;
    mbox_buf[1] = 0;
    mbox_buf[2] = MBOX_TAG_GETSERIAL;
    mbox_buf[3] = 8;
    mbox_buf[4] = 0;
    mbox_buf[5] = 0;
    mbox_buf[6] = 0;
    mbox_buf[7] = MBOX_TAG_LAST;

    if (mailbox_call() == 0)
        return ((uint64_t)mbox_buf[6] << 32) | mbox_buf[5];
    return 0;
}

/* Allocate framebuffer via GPU mailbox */
uint32_t mailbox_alloc_framebuffer(int width, int height, int depth,
                                    uint32_t *pitch, uint32_t *fb_size) {
    int i = 0;
    mbox_buf[i++] = 35 * 4;        /* buffer size */
    mbox_buf[i++] = 0;             /* request code */

    /* Set physical size */
    mbox_buf[i++] = MBOX_TAG_SETFBPHYS;
    mbox_buf[i++] = 8;
    mbox_buf[i++] = 0;
    mbox_buf[i++] = width;
    mbox_buf[i++] = height;

    /* Set virtual size (same as physical) */
    mbox_buf[i++] = MBOX_TAG_SETFBVIRT;
    mbox_buf[i++] = 8;
    mbox_buf[i++] = 0;
    mbox_buf[i++] = width;
    mbox_buf[i++] = height;

    /* Set depth */
    mbox_buf[i++] = MBOX_TAG_SETFBDEPTH;
    mbox_buf[i++] = 4;
    mbox_buf[i++] = 0;
    mbox_buf[i++] = depth;

    /* Set pixel order (0 = BGR, 1 = RGB) */
    mbox_buf[i++] = MBOX_TAG_SETFBPOFF;
    mbox_buf[i++] = 4;
    mbox_buf[i++] = 0;
    mbox_buf[i++] = 1;             /* RGB */

    /* Allocate framebuffer */
    mbox_buf[i++] = MBOX_TAG_ALLOCFB;
    mbox_buf[i++] = 8;
    mbox_buf[i++] = 0;
    mbox_buf[i++] = 4096;          /* alignment */
    mbox_buf[i++] = 0;

    /* Get pitch */
    mbox_buf[i++] = MBOX_TAG_GETFBPITCH;
    mbox_buf[i++] = 4;
    mbox_buf[i++] = 0;
    mbox_buf[i++] = 0;

    mbox_buf[i++] = MBOX_TAG_LAST;

    if (mailbox_call() != 0) return 0;

    /* Parse response — find ALLOCFB tag */
    /* The framebuffer address is in the ALLOCFB response (convert from bus address) */
    *fb_size = mbox_buf[24];
    *pitch = mbox_buf[28];

    /* Bus address → ARM address: clear upper 2 bits */
    uint32_t fb_addr = mbox_buf[23] & 0x3FFFFFFF;
    return fb_addr;
}
