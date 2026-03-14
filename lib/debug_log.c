/* AiOS — Debug Log
   Ring buffer that captures all debug output.
   View with /debug command or ask the AI to show debug log. */

#include "include/debug_log.h"
#include "include/string.h"
#include "include/window.h"
#include "include/stdio.h"

/* 8KB ring buffer */
#define DBG_LOG_SIZE  8192

static char log_buf[DBG_LOG_SIZE];
static int  log_pos = 0;    /* next write position */
static int  log_len = 0;    /* total bytes used (capped at DBG_LOG_SIZE) */

void dbg_log(const char *msg) {
    if (!msg) return;

    while (*msg) {
        log_buf[log_pos] = *msg++;
        log_pos = (log_pos + 1) % DBG_LOG_SIZE;
        if (log_len < DBG_LOG_SIZE)
            log_len++;
    }
}

const char *dbg_get_log(int *out_len) {
    if (out_len) *out_len = log_len;
    return log_buf;
}

void dbg_clear(void) {
    log_pos = 0;
    log_len = 0;
}

void dbg_show(void) {
    if (log_len == 0) {
        /* Nothing to show — just open an empty window */
        int id = win_create("Debug Log", 40, 20, 700, 500,
                            WIN_CLOSABLE | WIN_RESIZABLE | WIN_SCROLLABLE | WIN_VISIBLE);
        if (id >= 0)
            win_append_text(id, "(debug log is empty)\n");
        return;
    }

    /* Linearize the ring buffer into a temporary buffer */
    char linear[DBG_LOG_SIZE + 1];
    int out = 0;

    if (log_len < DBG_LOG_SIZE) {
        /* Buffer hasn't wrapped — data is 0..log_pos */
        memcpy(linear, log_buf, log_len);
        out = log_len;
    } else {
        /* Buffer wrapped — oldest data starts at log_pos */
        int tail = DBG_LOG_SIZE - log_pos;
        memcpy(linear, log_buf + log_pos, tail);
        memcpy(linear + tail, log_buf, log_pos);
        out = DBG_LOG_SIZE;
    }
    linear[out] = '\0';

    int id = win_create("Debug Log", 40, 20, 700, 500,
                        WIN_CLOSABLE | WIN_RESIZABLE | WIN_SCROLLABLE | WIN_VISIBLE);
    if (id >= 0)
        win_append_text(id, linear);
}
