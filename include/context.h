#ifndef AIOS_CONTEXT_H
#define AIOS_CONTEXT_H

#include "types.h"

/* ============================================================================
 * AiOS Context Frame — the kernel's "ground truth" snapshot
 * The AI reads this to understand the system state before acting.
 * Only the kernel writes to this (via context_update / INTENT_SETCTX).
 * ========================================================================= */

/* Context Frame structure (matches kernel/context.asm layout) */
typedef struct __attribute__((packed)) {
    uint8_t  user_state;       /* 0x00: 0=Idle, 1=Active, 2=Busy */
    uint8_t  activity;         /* 0x01: 0=Desktop, 1=Shell, 2=AI_Prompt */
    uint8_t  privacy_level;    /* 0x02: 0=Open, 1=Standard, 2=Restricted */
    uint8_t  power_state;      /* 0x03: 0=Battery_Low, 1=Normal, 2=Wall */
    uint8_t  output_mode;      /* 0x04: 0=Text, 1=Minimal, 2=Silent */
    uint8_t  ai_tier;          /* 0x05: 0=Local_SLM, 1=Local_Pro, 2=Remote */
    uint8_t  kbd_layout;       /* 0x06: Keyboard layout ID */
    uint8_t  net_status;       /* 0x07: 0=Down, 1=DHCP_Pending, 2=Up */
    uint32_t uptime_secs;      /* 0x08: Seconds since boot */
    uint32_t total_memory_kb;  /* 0x0C: Total RAM in KB */
    uint32_t free_heap_bytes;  /* 0x10: Free heap space */
    uint32_t intent_count;     /* 0x14: Total intents processed */
    uint8_t  last_intent_id;   /* 0x18: Last intent syscall number */
    uint8_t  last_intent_err;  /* 0x19: 0=OK, else error code */
    uint8_t  reserved[6];      /* 0x1A-0x1F */
} context_frame_t;

/* External reference to the kernel's context frame (in context.asm) */
extern context_frame_t context_frame;

/* Context field update from C (calls context_update in asm) */
extern void context_update(void);

/* ============================================================================
 * Intent Syscall IDs — the AI's "vocabulary" for interacting with the kernel
 * ========================================================================= */
#define INTENT_RENDER     0x01  /* Display text/data */
#define INTENT_NOTIFY     0x02  /* Alert the user (urgency-aware) */
#define INTENT_RECALL     0x03  /* Query stored knowledge (future) */
#define INTENT_MEMORIZE   0x04  /* Store data persistently (future) */
#define INTENT_COMM       0x05  /* Send external message (future) */
#define INTENT_QUERY      0x06  /* Ask remote AI model */
#define INTENT_SETCTX     0x07  /* Modify context field */
#define INTENT_GETCTX     0x08  /* Read context frame */
#define INTENT_SETLAYOUT  0x09  /* Switch keyboard layout */

/* Render types */
#define RENDER_PLAIN  0
#define RENDER_INFO   1
#define RENDER_ERROR  2

/* Notify urgency levels */
#define NOTIFY_LOW    0
#define NOTIFY_NORMAL 1
#define NOTIFY_HIGH   2

/* Output modes */
#define OUTPUT_TEXT    0
#define OUTPUT_MINIMAL 1
#define OUTPUT_SILENT  2

/* AI tiers */
#define AI_LOCAL_SLM  0
#define AI_LOCAL_PRO  1
#define AI_REMOTE     2

/* Privacy levels */
#define PRIVACY_OPEN       0
#define PRIVACY_STANDARD   1
#define PRIVACY_RESTRICTED 2

/* ============================================================================
 * C inline wrappers for INT 0x80 syscalls
 * ========================================================================= */

/* Generic intent syscall */
static inline int intent_call(int id, int arg1, int arg2, int arg3) {
    int ret;
    __asm__ volatile(
        "int $0x80"
        : "=a"(ret)
        : "a"(id), "b"(arg1), "c"(arg2), "d"(arg3)
        : "memory"
    );
    return ret;
}

/* INTENT_RENDER: Display text */
static inline int intent_render(const char *text, int type) {
    return intent_call(INTENT_RENDER, (int)text, type, 0);
}

/* INTENT_NOTIFY: Alert user */
static inline int intent_notify(int urgency, const char *message) {
    return intent_call(INTENT_NOTIFY, urgency, 0, (int)message);
}

/* INTENT_QUERY: Ask remote AI */
static inline int intent_query(const char *question, char *response, int max_len) {
    return intent_call(INTENT_QUERY, (int)question, (int)response, max_len);
}

/* INTENT_SETCTX: Set context field */
static inline int intent_setctx(int offset, int value) {
    return intent_call(INTENT_SETCTX, offset, value, 0);
}

/* INTENT_GETCTX: Read context frame */
static inline int intent_getctx(context_frame_t *buf) {
    return intent_call(INTENT_GETCTX, (int)buf, 0, 0);
}

/* INTENT_SETLAYOUT: Switch keyboard layout */
static inline int intent_setlayout(int layout_id) {
    return intent_call(INTENT_SETLAYOUT, layout_id, 0, 0);
}

#endif /* AIOS_CONTEXT_H */
