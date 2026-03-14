/* AiOS — ATA PIO Driver (primary IDE controller) */
#ifndef AIOS_ATA_H
#define AIOS_ATA_H

#include "types.h"

/* Initialize ATA — detect primary drive. Returns 0 on success. */
int ata_init(void);

/* Returns 1 if a drive was detected. */
int ata_is_ready(void);

/* Read `count` sectors starting at LBA `lba` into `buf`.
   buf must hold count * 512 bytes. Returns 0 on success. */
int ata_read_sectors(uint32_t lba, uint8_t count, void *buf);

/* Write `count` sectors starting at LBA `lba` from `buf`.
   Returns 0 on success. */
int ata_write_sectors(uint32_t lba, uint8_t count, const void *buf);

#endif
