/* AiOS — Kernel Updater
   Downloads a new kernel binary via HTTPS and writes it to the FAT16
   partition on disk, replacing /boot/aios.bin. */

#include "include/updater.h"
#include "include/tls_client.h"
#include "include/ata.h"
#include "include/string.h"
#include "include/heap.h"
#include "include/stdio.h"

extern void fb_print(const char *str);
extern void fb_newline(void);
extern void fb_set_color(int attr);

/* FAT16 partition starts at this LBA (must match tools/make-disk.sh) */
#define PART_START_LBA  4096
#define MAX_KERNEL_SIZE (512 * 1024)

/* Parse https://host/path from URL string */
static int parse_url(const char *url, char *host, int host_max,
                     char *path, int path_max) {
    if (memcmp(url, "https://", 8) != 0) return -1;
    const char *p = url + 8;

    int i = 0;
    while (*p && *p != '/' && i < host_max - 1)
        host[i++] = *p++;
    host[i] = '\0';
    if (i == 0) return -1;

    if (*p == '/') {
        int j = 0;
        while (*p && j < path_max - 1)
            path[j++] = *p++;
        path[j] = '\0';
    } else {
        path[0] = '/';
        path[1] = '\0';
    }
    return 0;
}

/* Write kernel binary to the FAT16 partition's /boot/aios.bin file.
   Reads the BPB, locates the file in the root directory, and overwrites
   its data sectors. Assumes clusters are contiguous (true for a fresh FS). */
static int fat16_write_kernel(const uint8_t *data, int size) {
    uint8_t sect[512];

    /* Read BPB from partition start */
    if (ata_read_sectors(PART_START_LBA, 1, sect) < 0) return -1;

    /* Parse BPB fields */
    uint16_t bytes_per_sect = *(uint16_t *)(sect + 11);
    uint8_t  sects_per_clust = sect[13];
    uint16_t reserved = *(uint16_t *)(sect + 14);
    uint8_t  num_fats = sect[16];
    uint16_t root_entries = *(uint16_t *)(sect + 17);
    uint16_t fat_size = *(uint16_t *)(sect + 22);

    if (bytes_per_sect != 512 || sects_per_clust == 0) return -2;

    /* Calculate filesystem layout */
    uint32_t root_dir_lba = PART_START_LBA + reserved + (uint32_t)num_fats * fat_size;
    uint32_t root_dir_sects = ((uint32_t)root_entries * 32 + 511) / 512;
    uint32_t data_start_lba = root_dir_lba + root_dir_sects;

    /* Search root directory for the kernel file.
       FAT stores /boot/aios.bin using VFAT long name entries or as a
       subdirectory entry. We need to traverse into /boot/ first. */

    /* Find "BOOT" subdirectory in root */
    uint16_t boot_cluster = 0;
    for (uint32_t s = 0; s < root_dir_sects; s++) {
        if (ata_read_sectors(root_dir_lba + s, 1, sect) < 0) return -3;
        for (int i = 0; i < 512; i += 32) {
            if (sect[i] == 0x00) goto not_found;
            if (sect[i] == 0xE5) continue;
            if (memcmp(sect + i, "BOOT       ", 11) == 0 && (sect[i + 11] & 0x10)) {
                boot_cluster = *(uint16_t *)(sect + i + 26);
                goto found_boot;
            }
        }
    }
not_found:
    return -4;

found_boot:;
    /* Read /boot/ directory to find AIOS.BIN */
    uint32_t boot_dir_lba = data_start_lba + (uint32_t)(boot_cluster - 2) * sects_per_clust;
    int found = 0;
    uint16_t first_cluster = 0;
    uint32_t dir_sect_lba = 0;
    int dir_entry_off = 0;

    for (int s = 0; s < sects_per_clust; s++) {
        if (ata_read_sectors(boot_dir_lba + s, 1, sect) < 0) return -3;
        for (int i = 0; i < 512; i += 32) {
            if (sect[i] == 0x00) break;
            if (sect[i] == 0xE5) continue;
            if (memcmp(sect + i, "AIOS    BIN", 11) == 0) {
                first_cluster = *(uint16_t *)(sect + i + 26);
                dir_sect_lba = boot_dir_lba + s;
                dir_entry_off = i;
                found = 1;
                goto found_file;
            }
        }
    }
    return -4;

found_file:
    if (!found || first_cluster < 2) return -4;

    /* Calculate first data sector of the kernel file */
    uint32_t kernel_lba = data_start_lba + (uint32_t)(first_cluster - 2) * sects_per_clust;

    /* Write kernel data (assumes contiguous clusters) */
    int remaining = size;
    const uint8_t *p = data;
    uint32_t lba = kernel_lba;

    while (remaining > 0) {
        int full_sectors = remaining / 512;
        if (full_sectors > 0) {
            int chunk = full_sectors > 255 ? 255 : full_sectors;
            if (ata_write_sectors(lba, (uint8_t)chunk, p) < 0) return -5;
            lba += chunk;
            p += chunk * 512;
            remaining -= chunk * 512;
        }

        /* Handle last partial sector with zero padding */
        if (remaining > 0 && remaining < 512) {
            uint8_t pad[512];
            memcpy(pad, p, remaining);
            memset(pad + remaining, 0, 512 - remaining);
            if (ata_write_sectors(lba, 1, pad) < 0) return -5;
            remaining = 0;
        }
    }

    /* Update file size in directory entry */
    if (ata_read_sectors(dir_sect_lba, 1, sect) < 0) return -6;
    *(uint32_t *)(sect + dir_entry_off + 28) = (uint32_t)size;
    if (ata_write_sectors(dir_sect_lba, 1, sect) < 0) return -7;

    return 0;
}

int update_kernel(const char *url) {
    char msg[120];

    if (!ata_is_ready()) {
        fb_set_color(0x0C);
        fb_print("[update] No disk available\n");
        fb_set_color(0x07);
        return -1;
    }

    fb_set_color(0x0E);
    fb_print("[update] Downloading: ");
    fb_print(url);
    fb_newline();
    fb_set_color(0x07);

    char host[128], path[256];
    if (parse_url(url, host, sizeof(host), path, sizeof(path)) < 0) {
        fb_set_color(0x0C);
        fb_print("[update] Invalid URL (must start with https://)\n");
        fb_set_color(0x07);
        return -1;
    }

    /* Allocate download buffer */
    int buf_size = MAX_KERNEL_SIZE + 4096;
    char *buf = malloc(buf_size);
    if (!buf) {
        fb_set_color(0x0C);
        fb_print("[update] Out of memory\n");
        fb_set_color(0x07);
        return -1;
    }

    int ret = https_get(host, path, "", buf, buf_size);
    if (ret <= 0) {
        fb_set_color(0x0C);
        snprintf(msg, sizeof(msg), "[update] Download failed (err=%d)\n", ret);
        fb_print(msg);
        fb_set_color(0x07);
        free(buf);
        return -1;
    }

    snprintf(msg, sizeof(msg), "[update] Downloaded %d bytes\n", ret);
    fb_print(msg);

    if (ret > MAX_KERNEL_SIZE) {
        fb_set_color(0x0C);
        fb_print("[update] Kernel too large (max 512KB)\n");
        fb_set_color(0x07);
        free(buf);
        return -1;
    }

    /* Validate multiboot magic (0x1BADB002) in first 8KB */
    int valid = 0;
    for (int i = 0; i < ret - 3 && i < 8192; i += 4) {
        if (*(uint32_t *)(buf + i) == 0x1BADB002) {
            valid = 1;
            break;
        }
    }
    if (!valid) {
        fb_set_color(0x0C);
        fb_print("[update] Invalid kernel (no multiboot header)\n");
        fb_set_color(0x07);
        free(buf);
        return -1;
    }

    /* Write to disk */
    fb_print("[update] Writing to disk...\n");
    int wr = fat16_write_kernel((const uint8_t *)buf, ret);
    free(buf);

    if (wr < 0) {
        fb_set_color(0x0C);
        snprintf(msg, sizeof(msg), "[update] Disk write failed (err=%d)\n", wr);
        fb_print(msg);
        fb_set_color(0x07);
        return -1;
    }

    fb_set_color(0x0A);
    fb_print("[update] Kernel updated! Reboot to apply.\n");
    fb_set_color(0x07);
    return 0;
}
