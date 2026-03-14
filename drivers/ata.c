/* AiOS — ATA PIO Driver
   Minimal 28-bit LBA PIO driver for the primary IDE controller.
   Used to persist AI memory and dynamic tools across reboots. */

#include "include/ata.h"
#include "include/io.h"
#include "include/string.h"

/* Primary ATA I/O ports */
#define ATA_DATA        0x1F0
#define ATA_ERROR       0x1F1
#define ATA_SECT_COUNT  0x1F2
#define ATA_LBA_LO      0x1F3
#define ATA_LBA_MID     0x1F4
#define ATA_LBA_HI      0x1F5
#define ATA_DRIVE_HEAD  0x1F6
#define ATA_STATUS      0x1F7
#define ATA_COMMAND     0x1F7
#define ATA_ALT_STATUS  0x3F6

/* Status bits */
#define ATA_SR_BSY   0x80
#define ATA_SR_DRDY  0x40
#define ATA_SR_DRQ   0x08
#define ATA_SR_ERR   0x01

/* Commands */
#define ATA_CMD_READ   0x20
#define ATA_CMD_WRITE  0x30
#define ATA_CMD_FLUSH  0xE7
#define ATA_CMD_IDENTIFY 0xEC

static int ata_ready = 0;

/* Wait for BSY to clear. Returns 0 on success, -1 on timeout. */
static int ata_wait_bsy(void) {
    for (int i = 0; i < 100000; i++) {
        uint8_t s = inb(ATA_STATUS);
        if (!(s & ATA_SR_BSY)) return 0;
    }
    return -1;
}

/* Wait for DRQ to set (data ready). Returns 0 on success. */
static int ata_wait_drq(void) {
    for (int i = 0; i < 100000; i++) {
        uint8_t s = inb(ATA_STATUS);
        if (s & ATA_SR_ERR) return -1;
        if (s & ATA_SR_DRQ) return 0;
    }
    return -1;
}

/* Soft delay — 400ns by reading alt status 4 times */
static void ata_delay(void) {
    inb(ATA_ALT_STATUS);
    inb(ATA_ALT_STATUS);
    inb(ATA_ALT_STATUS);
    inb(ATA_ALT_STATUS);
}

int ata_init(void) {
    /* Select drive 0 (master) with LBA mode */
    outb(ATA_DRIVE_HEAD, 0xE0);
    ata_delay();

    /* Send IDENTIFY command */
    outb(ATA_SECT_COUNT, 0);
    outb(ATA_LBA_LO, 0);
    outb(ATA_LBA_MID, 0);
    outb(ATA_LBA_HI, 0);
    outb(ATA_COMMAND, ATA_CMD_IDENTIFY);
    ata_delay();

    uint8_t status = inb(ATA_STATUS);
    if (status == 0) {
        /* No drive present */
        ata_ready = 0;
        return -1;
    }

    /* Wait for BSY to clear */
    if (ata_wait_bsy() < 0) {
        ata_ready = 0;
        return -1;
    }

    /* Check if it's an ATA drive (not ATAPI) */
    if (inb(ATA_LBA_MID) != 0 || inb(ATA_LBA_HI) != 0) {
        ata_ready = 0;
        return -1;
    }

    /* Wait for DRQ or ERR */
    if (ata_wait_drq() < 0) {
        ata_ready = 0;
        return -1;
    }

    /* Read and discard IDENTIFY data (256 words) */
    for (int i = 0; i < 256; i++)
        inw(ATA_DATA);

    ata_ready = 1;
    return 0;
}

int ata_is_ready(void) {
    return ata_ready;
}

int ata_read_sectors(uint32_t lba, uint8_t count, void *buf) {
    if (!ata_ready || count == 0) return -1;

    if (ata_wait_bsy() < 0) return -1;

    /* Select drive 0, LBA mode, top 4 bits of LBA */
    outb(ATA_DRIVE_HEAD, 0xE0 | ((lba >> 24) & 0x0F));
    outb(ATA_SECT_COUNT, count);
    outb(ATA_LBA_LO, lba & 0xFF);
    outb(ATA_LBA_MID, (lba >> 8) & 0xFF);
    outb(ATA_LBA_HI, (lba >> 16) & 0xFF);
    outb(ATA_COMMAND, ATA_CMD_READ);

    uint16_t *p = (uint16_t *)buf;
    for (int s = 0; s < count; s++) {
        ata_delay();
        if (ata_wait_bsy() < 0) return -1;
        if (ata_wait_drq() < 0) return -1;

        /* Read 256 words (512 bytes) */
        for (int i = 0; i < 256; i++)
            *p++ = inw(ATA_DATA);
    }

    return 0;
}

int ata_write_sectors(uint32_t lba, uint8_t count, const void *buf) {
    if (!ata_ready || count == 0) return -1;

    if (ata_wait_bsy() < 0) return -1;

    /* Select drive 0, LBA mode, top 4 bits of LBA */
    outb(ATA_DRIVE_HEAD, 0xE0 | ((lba >> 24) & 0x0F));
    outb(ATA_SECT_COUNT, count);
    outb(ATA_LBA_LO, lba & 0xFF);
    outb(ATA_LBA_MID, (lba >> 8) & 0xFF);
    outb(ATA_LBA_HI, (lba >> 16) & 0xFF);
    outb(ATA_COMMAND, ATA_CMD_WRITE);

    const uint16_t *p = (const uint16_t *)buf;
    for (int s = 0; s < count; s++) {
        ata_delay();
        if (ata_wait_bsy() < 0) return -1;
        if (ata_wait_drq() < 0) return -1;

        /* Write 256 words (512 bytes) */
        for (int i = 0; i < 256; i++)
            outw(ATA_DATA, *p++);
    }

    /* Flush cache */
    outb(ATA_COMMAND, ATA_CMD_FLUSH);
    if (ata_wait_bsy() < 0) return -1;

    return 0;
}
