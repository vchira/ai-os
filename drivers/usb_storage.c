/* AiOS — USB Mass Storage Class Driver (Bulk-Only Transport / BBB)
   Supports USB flash drives with SCSI transparent command set.
   Uses Bulk-Only Transport (BOT) per USB MSC BBB spec.

   Protocol:
   1. Command Block Wrapper (CBW) → Bulk OUT
   2. Data → Bulk IN or Bulk OUT
   3. Command Status Wrapper (CSW) ← Bulk IN */

#include "include/usb.h"
#include "include/string.h"
#include "include/heap.h"
#include "include/stdio.h"

extern void fb_print(const char *);
extern void fb_newline(void);

/* CBW signature */
#define CBW_SIGNATURE  0x43425355
/* CSW signature */
#define CSW_SIGNATURE  0x53425355

/* CBW — Command Block Wrapper */
typedef struct __attribute__((packed)) {
    uint32_t dCBWSignature;
    uint32_t dCBWTag;
    uint32_t dCBWDataTransferLength;
    uint8_t  bmCBWFlags;         /* bit 7: 0=OUT, 1=IN */
    uint8_t  bCBWLUN;
    uint8_t  bCBWCBLength;       /* 1-16 */
    uint8_t  CBWCB[16];          /* command block */
} usb_cbw_t;

/* CSW — Command Status Wrapper */
typedef struct __attribute__((packed)) {
    uint32_t dCSWSignature;
    uint32_t dCSWTag;
    uint32_t dCSWDataResidue;
    uint8_t  bCSWStatus;         /* 0=pass, 1=fail, 2=phase error */
} usb_csw_t;

/* SCSI commands */
#define SCSI_TEST_UNIT_READY   0x00
#define SCSI_REQUEST_SENSE     0x03
#define SCSI_INQUIRY           0x12
#define SCSI_READ_CAPACITY_10  0x25
#define SCSI_READ_10           0x28
#define SCSI_WRITE_10          0x2A

/* Maximum tracked storage devices */
#define MAX_MSC  4

typedef struct {
    usb_device_t *dev;
    uint32_t tag;             /* incrementing CBW tag */
    uint32_t block_count;     /* total blocks */
    uint32_t block_size;      /* bytes per block (usually 512) */
    int ready;
} msc_device_t;

static msc_device_t msc_devs[MAX_MSC];
static int msc_count = 0;

static char pbuf[128];

/* ========================================================================= */
/* BOT transfer helpers                                                      */
/* ========================================================================= */

static int msc_bot_transfer(msc_device_t *msc, uint8_t *cdb, int cdb_len,
                            int direction, void *data, int data_len) {
    usb_cbw_t cbw;
    usb_csw_t csw;

    memset(&cbw, 0, sizeof(cbw));
    cbw.dCBWSignature = CBW_SIGNATURE;
    cbw.dCBWTag = msc->tag++;
    cbw.dCBWDataTransferLength = data_len;
    cbw.bmCBWFlags = direction ? 0x80 : 0x00;  /* IN or OUT */
    cbw.bCBWLUN = 0;
    cbw.bCBWCBLength = cdb_len;
    memcpy(cbw.CBWCB, cdb, cdb_len);

    /* Send CBW */
    int ret = usb_bulk_write(msc->dev, &cbw, sizeof(cbw));
    if (ret < 0) return -1;

    /* Data phase */
    if (data_len > 0) {
        if (direction) {
            ret = usb_bulk_read(msc->dev, data, data_len);
        } else {
            ret = usb_bulk_write(msc->dev, data, data_len);
        }
        if (ret < 0) return -2;
    }

    /* Read CSW */
    memset(&csw, 0, sizeof(csw));
    ret = usb_bulk_read(msc->dev, &csw, sizeof(csw));
    if (ret < (int)sizeof(csw)) return -3;

    if (csw.dCSWSignature != CSW_SIGNATURE) return -4;
    if (csw.bCSWStatus != 0) return -5;

    return data_len - csw.dCSWDataResidue;
}

/* ========================================================================= */
/* SCSI commands                                                             */
/* ========================================================================= */

static int msc_test_unit_ready(msc_device_t *msc) {
    uint8_t cdb[6] = { SCSI_TEST_UNIT_READY, 0, 0, 0, 0, 0 };
    return msc_bot_transfer(msc, cdb, 6, 0, NULL, 0);
}

static int msc_inquiry(msc_device_t *msc, void *buf, int len) {
    uint8_t cdb[6] = { SCSI_INQUIRY, 0, 0, 0, (uint8_t)len, 0 };
    return msc_bot_transfer(msc, cdb, 6, 1, buf, len);
}

static int msc_read_capacity(msc_device_t *msc) {
    uint8_t data[8];
    uint8_t cdb[10] = { SCSI_READ_CAPACITY_10, 0, 0, 0, 0, 0, 0, 0, 0, 0 };

    int ret = msc_bot_transfer(msc, cdb, 10, 1, data, 8);
    if (ret < 8) return -1;

    /* Big-endian */
    msc->block_count = (data[0] << 24) | (data[1] << 16) |
                       (data[2] << 8) | data[3];
    msc->block_count++;  /* last LBA + 1 */
    msc->block_size = (data[4] << 24) | (data[5] << 16) |
                      (data[6] << 8) | data[7];

    return 0;
}

/* Read sectors from USB storage */
int usb_msc_read(int dev_idx, uint32_t lba, uint32_t count, void *buf) {
    if (dev_idx < 0 || dev_idx >= msc_count) return -1;
    msc_device_t *msc = &msc_devs[dev_idx];
    if (!msc->ready) return -2;

    uint8_t cdb[10];
    memset(cdb, 0, sizeof(cdb));
    cdb[0] = SCSI_READ_10;
    cdb[2] = (lba >> 24) & 0xFF;
    cdb[3] = (lba >> 16) & 0xFF;
    cdb[4] = (lba >> 8) & 0xFF;
    cdb[5] = lba & 0xFF;
    cdb[7] = (count >> 8) & 0xFF;
    cdb[8] = count & 0xFF;

    int data_len = count * msc->block_size;
    return msc_bot_transfer(msc, cdb, 10, 1, buf, data_len);
}

/* Write sectors to USB storage */
int usb_msc_write(int dev_idx, uint32_t lba, uint32_t count, const void *buf) {
    if (dev_idx < 0 || dev_idx >= msc_count) return -1;
    msc_device_t *msc = &msc_devs[dev_idx];
    if (!msc->ready) return -2;

    uint8_t cdb[10];
    memset(cdb, 0, sizeof(cdb));
    cdb[0] = SCSI_WRITE_10;
    cdb[2] = (lba >> 24) & 0xFF;
    cdb[3] = (lba >> 16) & 0xFF;
    cdb[4] = (lba >> 8) & 0xFF;
    cdb[5] = lba & 0xFF;
    cdb[7] = (count >> 8) & 0xFF;
    cdb[8] = count & 0xFF;

    int data_len = count * msc->block_size;
    return msc_bot_transfer(msc, cdb, 10, 0, (void *)buf, data_len);
}

/* ========================================================================= */
/* Attach and query                                                          */
/* ========================================================================= */

void usb_msc_attach(usb_device_t *dev) {
    if (msc_count >= MAX_MSC) return;
    if (dev->if_class != USB_CLASS_MASS_STORAGE) return;

    msc_device_t *msc = &msc_devs[msc_count];
    memset(msc, 0, sizeof(*msc));
    msc->dev = dev;
    msc->tag = 1;

    /* Wait for device to be ready */
    for (int i = 0; i < 10; i++) {
        if (msc_test_unit_ready(msc) >= 0) break;
        for (volatile int j = 0; j < 100000; j++) ;
    }

    /* INQUIRY */
    uint8_t inq[36];
    if (msc_inquiry(msc, inq, 36) >= 36) {
        char vendor[9], product[17];
        memcpy(vendor, &inq[8], 8); vendor[8] = '\0';
        memcpy(product, &inq[16], 16); product[16] = '\0';
        snprintf(pbuf, sizeof(pbuf), "[OK] USB storage: %s %s", vendor, product);
        fb_print(pbuf);
        fb_newline();
    }

    /* Read capacity */
    if (msc_read_capacity(msc) == 0) {
        uint32_t mb = (msc->block_count / 2048);  /* blocks to MB */
        snprintf(pbuf, sizeof(pbuf), "     %u MB (%u x %u byte sectors)",
                 mb, msc->block_count, msc->block_size);
        fb_print(pbuf);
        fb_newline();
        msc->ready = 1;
    }

    msc_count++;
}

int usb_msc_get_count(void) { return msc_count; }
int usb_msc_is_ready(int idx) {
    return (idx >= 0 && idx < msc_count && msc_devs[idx].ready);
}
