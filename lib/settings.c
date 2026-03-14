/* AiOS — Persistent OS Settings
   Key-value store for OS configuration that persists across reboots.
   Stored on disk at a dedicated LBA range, separate from AI memory. */

#include "include/settings.h"
#include "include/string.h"
#include "include/stdio.h"
#include "include/heap.h"
#include "include/ata.h"
#include "include/theme.h"
#include "include/mouse.h"

/* External OS functions we configure */
extern void keyboard_set_layout(int id);
extern int  keyboard_get_layout(void);

/* ========================================================================= */
/* Settings store                                                            */
/* ========================================================================= */

typedef struct {
    int  active;
    char key[SETTING_KEY_MAX];
    char value[SETTING_VALUE_MAX];
} setting_entry_t;

static setting_entry_t settings[SETTINGS_MAX];

/* Mouse sensitivity stored here, applied in mouse_handler via extern */
int mouse_sensitivity = 1;  /* 0=low, 1=normal, 2=high */

/* ========================================================================= */
/* Disk persistence — stored at LBA 2120 (after AI memory at 2048-2115)      */
/* ========================================================================= */

#define SETTINGS_LBA_BASE  2120
#define SETTINGS_MAGIC     0xA105CFCF   /* "AiOS ConFiG" */

#define SETTINGS_STORE_SIZE  (sizeof(settings))
#define SETTINGS_SECTORS     (((SETTINGS_STORE_SIZE) + 511) / 512)

typedef struct {
    uint32_t magic;
    uint32_t count;
} settings_header_t;

void settings_save(void) {
    if (!ata_is_ready()) return;

    /* Write header */
    uint8_t sector[512];
    memset(sector, 0, 512);
    settings_header_t *hdr = (settings_header_t *)sector;
    hdr->magic = SETTINGS_MAGIC;
    hdr->count = SETTINGS_MAX;
    ata_write_sectors(SETTINGS_LBA_BASE, 1, sector);

    /* Write settings array */
    int total_sectors = SETTINGS_SECTORS;
    uint8_t *buf = malloc(total_sectors * 512);
    if (!buf) return;
    memset(buf, 0, total_sectors * 512);
    memcpy(buf, settings, SETTINGS_STORE_SIZE);
    ata_write_sectors(SETTINGS_LBA_BASE + 1, total_sectors, buf);
    free(buf);
}

static void settings_load(void) {
    if (!ata_is_ready()) return;

    uint8_t sector[512];
    if (ata_read_sectors(SETTINGS_LBA_BASE, 1, sector) < 0) return;
    settings_header_t *hdr = (settings_header_t *)sector;
    if (hdr->magic != SETTINGS_MAGIC) return;

    int total_sectors = SETTINGS_SECTORS;
    uint8_t *buf = malloc(total_sectors * 512);
    if (!buf) return;
    if (ata_read_sectors(SETTINGS_LBA_BASE + 1, total_sectors, buf) == 0) {
        memcpy(settings, buf, SETTINGS_STORE_SIZE);
    }
    free(buf);
}

/* ========================================================================= */
/* Public API                                                                */
/* ========================================================================= */

void settings_init(void) {
    memset(settings, 0, sizeof(settings));
    settings_load();
    settings_apply();
}

const char *setting_get(const char *key) {
    for (int i = 0; i < SETTINGS_MAX; i++) {
        if (settings[i].active && strcmp(settings[i].key, key) == 0)
            return settings[i].value;
    }
    return 0;
}

int setting_get_int(const char *key, int def) {
    const char *v = setting_get(key);
    if (!v) return def;

    int neg = 0, val = 0;
    const char *p = v;
    if (*p == '-') { neg = 1; p++; }
    while (*p >= '0' && *p <= '9') {
        val = val * 10 + (*p - '0');
        p++;
    }
    return neg ? -val : val;
}

int setting_set(const char *key, const char *value) {
    /* Update existing */
    for (int i = 0; i < SETTINGS_MAX; i++) {
        if (settings[i].active && strcmp(settings[i].key, key) == 0) {
            strncpy(settings[i].value, value, SETTING_VALUE_MAX - 1);
            settings[i].value[SETTING_VALUE_MAX - 1] = '\0';
            return 0;
        }
    }
    /* Find free slot */
    for (int i = 0; i < SETTINGS_MAX; i++) {
        if (!settings[i].active) {
            settings[i].active = 1;
            strncpy(settings[i].key, key, SETTING_KEY_MAX - 1);
            settings[i].key[SETTING_KEY_MAX - 1] = '\0';
            strncpy(settings[i].value, value, SETTING_VALUE_MAX - 1);
            settings[i].value[SETTING_VALUE_MAX - 1] = '\0';
            return 0;
        }
    }
    return -1; /* full */
}

int setting_set_int(const char *key, int value) {
    char buf[20];
    snprintf(buf, sizeof(buf), "%d", value);
    return setting_set(key, buf);
}

int setting_delete(const char *key) {
    for (int i = 0; i < SETTINGS_MAX; i++) {
        if (settings[i].active && strcmp(settings[i].key, key) == 0) {
            settings[i].active = 0;
            return 0;
        }
    }
    return -1;
}

void settings_apply(void) {
    /* Theme */
    int theme_id = setting_get_int("theme", -1);
    if (theme_id >= 0 && theme_id < theme_count())
        theme_set(theme_id);

    /* Keyboard layout */
    int kbd_layout = setting_get_int("keyboard_layout", -1);
    if (kbd_layout >= 0)
        keyboard_set_layout(kbd_layout);

    /* Mouse sensitivity */
    mouse_sensitivity = setting_get_int("mouse_sensitivity", 1);
}

int settings_dump(char *buf, int max) {
    int p = 0;
    int count = 0;

    for (int i = 0; i < SETTINGS_MAX; i++)
        if (settings[i].active) count++;

    if (count == 0) {
        const char *s = "No settings configured.\n";
        int l = strlen(s);
        if (l < max) { memcpy(buf, s, l); buf[l] = '\0'; return l; }
        return 0;
    }

    for (int i = 0; i < SETTINGS_MAX && p < max - 100; i++) {
        if (!settings[i].active) continue;
        /* "  key = value\n" */
        buf[p++] = ' '; buf[p++] = ' ';
        int kl = strlen(settings[i].key);
        memcpy(buf + p, settings[i].key, kl); p += kl;
        buf[p++] = ' '; buf[p++] = '='; buf[p++] = ' ';
        int vl = strlen(settings[i].value);
        memcpy(buf + p, settings[i].value, vl); p += vl;
        buf[p++] = '\n';
    }
    buf[p] = '\0';
    return p;
}
