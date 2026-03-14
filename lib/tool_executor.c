/* AiOS — Tool Executor
   Dispatches LLM tool calls to OS primitives.

   Built-in tools:
     memorize, recall, forget — AI memory store
     get_datetime — hardware clock
     set_api_key — runtime API key configuration
     create_tool — AI creates new tools at runtime
     delete_tool — remove a dynamic tool
     list_tools  — list all available tools
     http_request — general-purpose HTTPS GET/POST

   Dynamic tools:
     Created by the AI via create_tool. Stored as name + description +
     implementation prompt. When called, the implementation is sent to
     the LLM with the tool input, and the LLM's response is the result.
*/

#include "include/tool_executor.h"
#include "include/rtc.h"
#include "include/string.h"
#include "include/heap.h"
#include "include/tls_client.h"
#include "include/stdio.h"
#include "include/ata.h"
#include "include/scheduler.h"
#include "include/window.h"
#include "include/widget.h"
#include "include/theme.h"
#include "include/bmp.h"
#include "include/debug_log.h"
#include "include/settings.h"

/* ========================================================================= */
/* AI Memory Store — persisted to disk via ATA                               */
/* ========================================================================= */

#define MEM_SLOTS    64
#define KEY_MAX      48
#define VALUE_MAX    256

typedef struct {
    int  active;
    char key[KEY_MAX];
    char value[VALUE_MAX];
} mem_entry_t;

static mem_entry_t mem_store[MEM_SLOTS];

/* ========================================================================= */
/* Dynamic Tool Registry                                                     */
/* ========================================================================= */

#define DYN_TOOL_SLOTS   16
#define TOOL_NAME_MAX    32
#define TOOL_DESC_MAX    128
#define TOOL_IMPL_MAX    512

typedef struct {
    int  active;
    char name[TOOL_NAME_MAX];
    char description[TOOL_DESC_MAX];
    char implementation[TOOL_IMPL_MAX];  /* prompt the LLM executes */
} dyn_tool_t;

static dyn_tool_t dyn_tools[DYN_TOOL_SLOTS];

/* LLM callback for dynamic tool execution — set by llm_provider */
static int (*dyn_tool_llm_cb)(const char *prompt, char *response, int max_len) = 0;

void tool_set_llm_callback(int (*cb)(const char *prompt, char *response, int max_len)) {
    dyn_tool_llm_cb = cb;
}

/* ========================================================================= */
/* Helpers                                                                   */
/* ========================================================================= */

static int str_append(char *buf, int pos, const char *s) {
    while (*s) buf[pos++] = *s++;
    return pos;
}

static int int_append(char *buf, int pos, int val) {
    if (val < 0) { buf[pos++] = '-'; val = -val; }
    if (val == 0) { buf[pos++] = '0'; return pos; }
    char tmp[12];
    int len = 0;
    while (val > 0) { tmp[len++] = '0' + (val % 10); val /= 10; }
    while (len > 0) buf[pos++] = tmp[--len];
    return pos;
}

/* Extract a JSON string value: {"key":"value"} → value */
static int json_get_string(const char *json, const char *key, char *out, int max) {
    char pattern[80];
    int plen = 0;
    pattern[plen++] = '"';
    const char *k = key;
    while (*k && plen < 76) pattern[plen++] = *k++;
    pattern[plen++] = '"';
    pattern[plen] = '\0';

    const char *p = strstr(json, pattern);
    if (!p) return -1;
    p += plen;

    while (*p == ':' || *p == ' ' || *p == '\t') p++;
    if (*p != '"') return -1;
    p++;

    int i = 0;
    while (*p && *p != '"' && i < max - 1) {
        if (*p == '\\' && *(p + 1)) {
            p++;
            switch (*p) {
                case 'n':  out[i++] = '\n'; break;
                case 't':  out[i++] = '\t'; break;
                case '"':  out[i++] = '"';  break;
                case '\\': out[i++] = '\\'; break;
                default:   out[i++] = *p;   break;
            }
        } else {
            out[i++] = *p;
        }
        p++;
    }
    out[i] = '\0';
    return i;
}

/* ========================================================================= */
/* Tool: get_datetime                                                        */
/* ========================================================================= */

static int tool_get_datetime(char *out, int max) {
    rtc_time_t t;
    rtc_get_time(&t);

    char dt[20];
    rtc_format_datetime(dt, sizeof(dt), &t);

    int p = 0;
    p = str_append(out, p, "{\"datetime\":\"");
    p = str_append(out, p, dt);
    p = str_append(out, p, "\",\"year\":");
    p = int_append(out, p, t.year);
    p = str_append(out, p, ",\"month\":");
    p = int_append(out, p, t.month);
    p = str_append(out, p, ",\"day\":");
    p = int_append(out, p, t.day);
    p = str_append(out, p, ",\"hour\":");
    p = int_append(out, p, t.hour);
    p = str_append(out, p, ",\"minute\":");
    p = int_append(out, p, t.minute);
    p = str_append(out, p, ",\"second\":");
    p = int_append(out, p, t.second);
    p = str_append(out, p, "}");
    out[p] = '\0';
    return p;
}

/* ========================================================================= */
/* Tool: memorize — store key→value                                          */
/* ========================================================================= */

static int tool_memorize(const char *input, char *out, int max) {
    char key[KEY_MAX];
    char value[VALUE_MAX];

    if (json_get_string(input, "key", key, KEY_MAX) < 0 || key[0] == '\0') {
        int p = str_append(out, 0, "{\"error\":\"missing or empty field: key\"}");
        out[p] = '\0';
        return p;
    }
    if (json_get_string(input, "value", value, VALUE_MAX) < 0 || value[0] == '\0') {
        int p = str_append(out, 0, "{\"error\":\"missing or empty field: value\"}");
        out[p] = '\0';
        return p;
    }

    /* Check if key already exists — overwrite */
    for (int i = 0; i < MEM_SLOTS; i++) {
        if (mem_store[i].active && strcmp(mem_store[i].key, key) == 0) {
            strncpy(mem_store[i].value, value, VALUE_MAX - 1);
            mem_store[i].value[VALUE_MAX - 1] = '\0';
            int p = 0;
            p = str_append(out, p, "{\"status\":\"updated\",\"key\":\"");
            p = str_append(out, p, key);
            p = str_append(out, p, "\"}");
            out[p] = '\0';
            return p;
        }
    }

    /* Find free slot */
    for (int i = 0; i < MEM_SLOTS; i++) {
        if (!mem_store[i].active) {
            mem_store[i].active = 1;
            strncpy(mem_store[i].key, key, KEY_MAX - 1);
            mem_store[i].key[KEY_MAX - 1] = '\0';
            strncpy(mem_store[i].value, value, VALUE_MAX - 1);
            mem_store[i].value[VALUE_MAX - 1] = '\0';
            int p = 0;
            p = str_append(out, p, "{\"status\":\"stored\",\"key\":\"");
            p = str_append(out, p, key);
            p = str_append(out, p, "\"}");
            out[p] = '\0';
            return p;
        }
    }

    int p = str_append(out, 0, "{\"error\":\"memory full (64 slots)\"}");
    out[p] = '\0';
    return p;
}

/* ========================================================================= */
/* Tool: recall — retrieve by key, or list all if key="*"                    */
/* ========================================================================= */

static int tool_recall(const char *input, char *out, int max) {
    char key[KEY_MAX];

    int list_all = (json_get_string(input, "key", key, KEY_MAX) < 0)
                   || (key[0] == '\0')
                   || (strcmp(key, "*") == 0);

    if (list_all) {
        /* No key / empty / wildcard — list all entries */
        int p = 0;
        p = str_append(out, p, "{\"entries\":[");
        int first = 1;
        for (int i = 0; i < MEM_SLOTS && p < max - 128; i++) {
            if (!mem_store[i].active) continue;
            if (!first) out[p++] = ',';
            first = 0;
            p = str_append(out, p, "{\"key\":\"");
            p = str_append(out, p, mem_store[i].key);
            p = str_append(out, p, "\",\"value\":\"");
            p = str_append(out, p, mem_store[i].value);
            p = str_append(out, p, "\"}");
        }
        p = str_append(out, p, "]}");
        out[p] = '\0';
        return p;
    }

    /* Look up specific key */
    for (int i = 0; i < MEM_SLOTS; i++) {
        if (mem_store[i].active && strcmp(mem_store[i].key, key) == 0) {
            int p = 0;
            p = str_append(out, p, "{\"key\":\"");
            p = str_append(out, p, mem_store[i].key);
            p = str_append(out, p, "\",\"value\":\"");
            p = str_append(out, p, mem_store[i].value);
            p = str_append(out, p, "\"}");
            out[p] = '\0';
            return p;
        }
    }

    int p = 0;
    p = str_append(out, p, "{\"error\":\"not found\",\"key\":\"");
    p = str_append(out, p, key);
    p = str_append(out, p, "\"}");
    out[p] = '\0';
    return p;
}

/* ========================================================================= */
/* Tool: forget — delete by key                                              */
/* ========================================================================= */

static int tool_forget(const char *input, char *out, int max) {
    char key[KEY_MAX];

    if (json_get_string(input, "key", key, KEY_MAX) < 0 || key[0] == '\0') {
        int p = str_append(out, 0, "{\"error\":\"missing or empty field: key\"}");
        out[p] = '\0';
        return p;
    }

    for (int i = 0; i < MEM_SLOTS; i++) {
        if (mem_store[i].active && strcmp(mem_store[i].key, key) == 0) {
            mem_store[i].active = 0;
            int p = 0;
            p = str_append(out, p, "{\"status\":\"forgotten\",\"key\":\"");
            p = str_append(out, p, key);
            p = str_append(out, p, "\"}");
            out[p] = '\0';
            return p;
        }
    }

    int p = 0;
    p = str_append(out, p, "{\"error\":\"not found\",\"key\":\"");
    p = str_append(out, p, key);
    p = str_append(out, p, "\"}");
    out[p] = '\0';
    return p;
}

/* ========================================================================= */
/* Tool: set_api_key — AI can set API keys at runtime                        */
/* ========================================================================= */

extern int llm_set_api_key(int provider_id, const char *key);

static int tool_set_api_key(const char *input, char *out, int max) {
    char provider[32];
    char key[128];

    if (json_get_string(input, "provider", provider, 32) < 0) {
        int p = str_append(out, 0, "{\"error\":\"missing field: provider (claude or openai)\"}");
        out[p] = '\0';
        return p;
    }
    if (json_get_string(input, "key", key, 128) < 0 || key[0] == '\0') {
        int p = str_append(out, 0, "{\"error\":\"missing or empty field: key\"}");
        out[p] = '\0';
        return p;
    }

    int provider_id = -1;
    if (strcmp(provider, "claude") == 0) provider_id = 0;
    else if (strcmp(provider, "openai") == 0) provider_id = 1;
    else {
        int p = str_append(out, 0, "{\"error\":\"unknown provider. Use claude or openai\"}");
        out[p] = '\0';
        return p;
    }

    int ret = llm_set_api_key(provider_id, key);
    if (ret == 0) {
        int p = 0;
        p = str_append(out, p, "{\"status\":\"key set\",\"provider\":\"");
        p = str_append(out, p, provider);
        p = str_append(out, p, "\"}");
        out[p] = '\0';
        return p;
    }

    int p = str_append(out, 0, "{\"error\":\"failed to set key\"}");
    out[p] = '\0';
    return p;
}

/* ========================================================================= */
/* Tool: create_tool — AI defines a new tool at runtime                      */
/* ========================================================================= */

static int tool_create_tool(const char *input, char *out, int max) {
    char name[TOOL_NAME_MAX];
    char desc[TOOL_DESC_MAX];
    char impl[TOOL_IMPL_MAX];

    if (json_get_string(input, "name", name, TOOL_NAME_MAX) < 0 || name[0] == '\0') {
        int p = str_append(out, 0, "{\"error\":\"missing field: name\"}");
        out[p] = '\0';
        return p;
    }
    if (json_get_string(input, "description", desc, TOOL_DESC_MAX) < 0) {
        int p = str_append(out, 0, "{\"error\":\"missing field: description\"}");
        out[p] = '\0';
        return p;
    }
    if (json_get_string(input, "implementation", impl, TOOL_IMPL_MAX) < 0 || impl[0] == '\0') {
        int p = str_append(out, 0, "{\"error\":\"missing field: implementation\"}");
        out[p] = '\0';
        return p;
    }

    /* Reject reserved built-in names */
    if (strcmp(name, "get_datetime") == 0 || strcmp(name, "memorize") == 0 ||
        strcmp(name, "recall") == 0 || strcmp(name, "forget") == 0 ||
        strcmp(name, "set_api_key") == 0 || strcmp(name, "create_tool") == 0 ||
        strcmp(name, "delete_tool") == 0 || strcmp(name, "list_tools") == 0 ||
        strcmp(name, "http_request") == 0 ||
        strcmp(name, "display_text") == 0 || strcmp(name, "display_image") == 0 ||
        strcmp(name, "ask_input") == 0 || strcmp(name, "ask_confirm") == 0 ||
        strcmp(name, "ask_choice") == 0 || strcmp(name, "show_notification") == 0 ||
        strcmp(name, "set_theme") == 0 || strcmp(name, "show_debug_log") == 0 ||
        strcmp(name, "configure") == 0) {
        int p = str_append(out, 0, "{\"error\":\"cannot override built-in tool\"}");
        out[p] = '\0';
        return p;
    }

    /* Check if already exists — overwrite */
    for (int i = 0; i < DYN_TOOL_SLOTS; i++) {
        if (dyn_tools[i].active && strcmp(dyn_tools[i].name, name) == 0) {
            strncpy(dyn_tools[i].description, desc, TOOL_DESC_MAX - 1);
            dyn_tools[i].description[TOOL_DESC_MAX - 1] = '\0';
            strncpy(dyn_tools[i].implementation, impl, TOOL_IMPL_MAX - 1);
            dyn_tools[i].implementation[TOOL_IMPL_MAX - 1] = '\0';
            int p = 0;
            p = str_append(out, p, "{\"status\":\"updated\",\"tool\":\"");
            p = str_append(out, p, name);
            p = str_append(out, p, "\"}");
            out[p] = '\0';
            return p;
        }
    }

    /* Find free slot */
    for (int i = 0; i < DYN_TOOL_SLOTS; i++) {
        if (!dyn_tools[i].active) {
            dyn_tools[i].active = 1;
            strncpy(dyn_tools[i].name, name, TOOL_NAME_MAX - 1);
            dyn_tools[i].name[TOOL_NAME_MAX - 1] = '\0';
            strncpy(dyn_tools[i].description, desc, TOOL_DESC_MAX - 1);
            dyn_tools[i].description[TOOL_DESC_MAX - 1] = '\0';
            strncpy(dyn_tools[i].implementation, impl, TOOL_IMPL_MAX - 1);
            dyn_tools[i].implementation[TOOL_IMPL_MAX - 1] = '\0';
            int p = 0;
            p = str_append(out, p, "{\"status\":\"created\",\"tool\":\"");
            p = str_append(out, p, name);
            p = str_append(out, p, "\"}");
            out[p] = '\0';
            return p;
        }
    }

    int p = str_append(out, 0, "{\"error\":\"tool registry full (16 slots)\"}");
    out[p] = '\0';
    return p;
}

/* ========================================================================= */
/* Tool: delete_tool — remove a dynamic tool                                 */
/* ========================================================================= */

static int tool_delete_tool(const char *input, char *out, int max) {
    char name[TOOL_NAME_MAX];

    if (json_get_string(input, "name", name, TOOL_NAME_MAX) < 0 || name[0] == '\0') {
        int p = str_append(out, 0, "{\"error\":\"missing field: name\"}");
        out[p] = '\0';
        return p;
    }

    for (int i = 0; i < DYN_TOOL_SLOTS; i++) {
        if (dyn_tools[i].active && strcmp(dyn_tools[i].name, name) == 0) {
            dyn_tools[i].active = 0;
            int p = 0;
            p = str_append(out, p, "{\"status\":\"deleted\",\"tool\":\"");
            p = str_append(out, p, name);
            p = str_append(out, p, "\"}");
            out[p] = '\0';
            return p;
        }
    }

    int p = 0;
    p = str_append(out, p, "{\"error\":\"not found\",\"tool\":\"");
    p = str_append(out, p, name);
    p = str_append(out, p, "\"}");
    out[p] = '\0';
    return p;
}

/* ========================================================================= */
/* Tool: list_tools — list all built-in + dynamic tools                      */
/* ========================================================================= */

static int tool_list_tools(char *out, int max) {
    int p = 0;
    p = str_append(out, p, "{\"builtin\":[");
    p = str_append(out, p, "\"get_datetime\",\"memorize\",\"recall\",\"forget\",");
    p = str_append(out, p, "\"set_api_key\",\"create_tool\",\"delete_tool\",");
    p = str_append(out, p, "\"list_tools\",\"http_request\",");
    p = str_append(out, p, "\"set_reminder\",\"cancel_reminder\",\"list_reminders\",");
    p = str_append(out, p, "\"display_text\",\"display_image\",");
    p = str_append(out, p, "\"ask_input\",\"ask_confirm\",\"ask_choice\",");
    p = str_append(out, p, "\"show_notification\",\"set_theme\",\"show_debug_log\",\"configure\"");
    p = str_append(out, p, "],\"dynamic\":[");

    int first = 1;
    for (int i = 0; i < DYN_TOOL_SLOTS && p < max - 256; i++) {
        if (!dyn_tools[i].active) continue;
        if (!first) out[p++] = ',';
        first = 0;
        p = str_append(out, p, "{\"name\":\"");
        p = str_append(out, p, dyn_tools[i].name);
        p = str_append(out, p, "\",\"description\":\"");
        p = str_append(out, p, dyn_tools[i].description);
        p = str_append(out, p, "\"}");
    }
    p = str_append(out, p, "]}");
    out[p] = '\0';
    return p;
}

/* ========================================================================= */
/* Tool: http_request — general-purpose HTTPS GET/POST                       */
/* ========================================================================= */

static int tool_http_request(const char *input, char *out, int max) {
    char host[128];
    char path[256];
    char method[8];
    char body[1024];
    char hdrs[256];

    if (json_get_string(input, "host", host, sizeof(host)) < 0 || host[0] == '\0') {
        int p = str_append(out, 0, "{\"error\":\"missing field: host\"}");
        out[p] = '\0';
        return p;
    }
    if (json_get_string(input, "path", path, sizeof(path)) < 0 || path[0] == '\0') {
        strncpy(path, "/", sizeof(path));
    }
    if (json_get_string(input, "method", method, sizeof(method)) < 0) {
        strncpy(method, "GET", sizeof(method));
    }
    if (json_get_string(input, "body", body, sizeof(body)) < 0) {
        body[0] = '\0';
    }
    if (json_get_string(input, "headers", hdrs, sizeof(hdrs)) < 0) {
        hdrs[0] = '\0';
    }

    /* Build headers string with CRLF */
    char full_hdrs[512];
    int hp = 0;
    if (hdrs[0]) {
        memcpy(full_hdrs + hp, hdrs, strlen(hdrs));
        hp += strlen(hdrs);
        /* Ensure ends with \r\n */
        if (hp >= 2 && full_hdrs[hp-2] != '\r') {
            full_hdrs[hp++] = '\r';
            full_hdrs[hp++] = '\n';
        }
    }
    full_hdrs[hp] = '\0';

    /* Allocate response buffer */
    int resp_max = max > 4096 ? 4096 : max;
    char *resp_buf = malloc(resp_max);
    if (!resp_buf) {
        int p = str_append(out, 0, "{\"error\":\"out of memory\"}");
        out[p] = '\0';
        return p;
    }

    int ret;
    if (strcmp(method, "POST") == 0 || strcmp(method, "post") == 0) {
        ret = https_post(host, path, full_hdrs, body, resp_buf, resp_max);
    } else {
        /* GET — pass empty body */
        ret = https_post(host, path, full_hdrs, "", resp_buf, resp_max);
    }

    if (ret <= 0) {
        free(resp_buf);
        int p = 0;
        p = str_append(out, p, "{\"error\":\"request failed\",\"code\":");
        p = int_append(out, p, ret);
        p = str_append(out, p, "}");
        out[p] = '\0';
        return p;
    }

    /* Truncate response to fit output buffer */
    int copy_len = ret;
    if (copy_len > max - 32) copy_len = max - 32;

    int p = 0;
    p = str_append(out, p, "{\"status\":\"ok\",\"length\":");
    p = int_append(out, p, ret);
    p = str_append(out, p, ",\"body\":\"");
    /* Simple copy — escape quotes and backslashes */
    for (int i = 0; i < copy_len && p < max - 4; i++) {
        char c = resp_buf[i];
        if (c == '"' || c == '\\') out[p++] = '\\';
        else if (c == '\n') { out[p++] = '\\'; out[p++] = 'n'; continue; }
        else if (c == '\r') continue;
        else if (c < 0x20) continue;  /* skip control chars */
        out[p++] = c;
    }
    p = str_append(out, p, "\"}");
    out[p] = '\0';
    free(resp_buf);
    return p;
}

/* ========================================================================= */
/* Tool: set_reminder — schedule a notification                              */
/* ========================================================================= */

static int json_get_int(const char *json, const char *key) {
    char val[16];
    if (json_get_string(json, key, val, sizeof(val)) < 0) return -1;
    int n = 0;
    const char *p = val;
    while (*p >= '0' && *p <= '9') { n = n * 10 + (*p - '0'); p++; }
    return (p == val) ? -1 : n;
}

static int tool_set_reminder(const char *input, char *out, int max) {
    char message[SCHED_MSG_MAX];

    if (json_get_string(input, "message", message, SCHED_MSG_MAX) < 0 || message[0] == '\0') {
        int p = str_append(out, 0, "{\"error\":\"missing field: message\"}");
        out[p] = '\0';
        return p;
    }

    int minutes = json_get_int(input, "minutes");
    int hour = json_get_int(input, "hour");
    int minute = json_get_int(input, "minute");

    if (minutes <= 0 && hour < 0) {
        int p = str_append(out, 0, "{\"error\":\"provide minutes (relative) or hour+minute (absolute)\"}");
        out[p] = '\0';
        return p;
    }

    int slot = scheduler_add(minutes > 0 ? minutes : 0, hour, minute >= 0 ? minute : 0, message);
    if (slot < 0) {
        int p = str_append(out, 0, "{\"error\":\"scheduler full (16 events)\"}");
        out[p] = '\0';
        return p;
    }

    int p = 0;
    p = str_append(out, p, "{\"status\":\"reminder set\",\"id\":");
    p = int_append(out, p, slot);
    if (minutes > 0) {
        p = str_append(out, p, ",\"in_minutes\":");
        p = int_append(out, p, minutes);
    } else {
        p = str_append(out, p, ",\"at\":\"");
        p = int_append(out, p, hour);
        p = str_append(out, p, ":");
        if (minute >= 0 && minute < 10) out[p++] = '0';
        p = int_append(out, p, minute >= 0 ? minute : 0);
        p = str_append(out, p, "\"");
    }
    p = str_append(out, p, "}");
    out[p] = '\0';
    return p;
}

/* ========================================================================= */
/* Tool: cancel_reminder — cancel a pending reminder                         */
/* ========================================================================= */

static int tool_cancel_reminder(const char *input, char *out, int max) {
    int id = json_get_int(input, "id");
    if (id < 0) {
        int p = str_append(out, 0, "{\"error\":\"missing field: id\"}");
        out[p] = '\0';
        return p;
    }

    if (scheduler_cancel(id) == 0) {
        int p = 0;
        p = str_append(out, p, "{\"status\":\"cancelled\",\"id\":");
        p = int_append(out, p, id);
        p = str_append(out, p, "}");
        out[p] = '\0';
        return p;
    }

    int p = str_append(out, 0, "{\"error\":\"reminder not found\"}");
    out[p] = '\0';
    return p;
}

/* ========================================================================= */
/* Tool: list_reminders — show pending reminders                             */
/* ========================================================================= */

static int tool_list_reminders(char *out, int max) {
    return scheduler_list(out, max);
}

/* ========================================================================= */
/* Tool: display_text — open a window with text content                      */
/* ========================================================================= */

static int tool_display_text(const char *input, char *out, int max) {
    char title[64];
    char content[4096];

    if (json_get_string(input, "title", title, sizeof(title)) < 0)
        strncpy(title, "Text", sizeof(title));
    if (json_get_string(input, "content", content, sizeof(content)) < 0 ||
        content[0] == '\0') {
        int p = str_append(out, 0, "{\"error\":\"missing field: content\"}");
        out[p] = '\0';
        return p;
    }

    int id = win_create(title, 80, 60, 560, 400,
                        WIN_CLOSABLE | WIN_RESIZABLE | WIN_SCROLLABLE);
    if (id < 0) {
        int p = str_append(out, 0, "{\"error\":\"no free window slots\"}");
        out[p] = '\0';
        return p;
    }

    win_append_text(id, content);

    int p = 0;
    p = str_append(out, p, "{\"status\":\"displayed\",\"window_id\":");
    p = int_append(out, p, id);
    p = str_append(out, p, "}");
    out[p] = '\0';
    return p;
}

/* ========================================================================= */
/* Tool: display_image — fetch and display a BMP image from URL              */
/* ========================================================================= */

static int parse_image_url(const char *url, char *host, int hmax,
                           char *path, int pmax) {
    const char *p = url;
    if (strncmp(p, "https://", 8) == 0) p += 8;
    else if (strncmp(p, "http://", 7) == 0) p += 7;

    int i = 0;
    while (*p && *p != '/' && i < hmax - 1) host[i++] = *p++;
    host[i] = '\0';

    if (*p == '/') {
        int j = 0;
        while (*p && j < pmax - 1) path[j++] = *p++;
        path[j] = '\0';
    } else {
        path[0] = '/'; path[1] = '\0';
    }
    return (i > 0) ? 0 : -1;
}

static int tool_display_image(const char *input, char *out, int max) {
    char url[256];
    char title[64];
    char host[128];
    char path[256];

    if (json_get_string(input, "url", url, sizeof(url)) < 0 || url[0] == '\0') {
        int p = str_append(out, 0, "{\"error\":\"missing field: url\"}");
        out[p] = '\0';
        return p;
    }
    if (json_get_string(input, "title", title, sizeof(title)) < 0)
        strncpy(title, "Image", sizeof(title));

    if (parse_image_url(url, host, sizeof(host), path, sizeof(path)) < 0) {
        int p = str_append(out, 0, "{\"error\":\"invalid URL\"}");
        out[p] = '\0';
        return p;
    }

    /* Fetch image data */
    int buf_size = 512 * 1024;  /* 512KB max */
    char *buf = malloc(buf_size);
    if (!buf) {
        int p = str_append(out, 0, "{\"error\":\"out of memory\"}");
        out[p] = '\0';
        return p;
    }

    int ret = https_get(host, path, "", buf, buf_size);
    if (ret <= 0) {
        free(buf);
        int p = 0;
        p = str_append(out, p, "{\"error\":\"fetch failed\",\"code\":");
        p = int_append(out, p, ret);
        p = str_append(out, p, "}");
        out[p] = '\0';
        return p;
    }

    /* Skip HTTP headers — find \r\n\r\n */
    char *body = buf;
    int body_len = ret;
    char *hdr_end = strstr(buf, "\r\n\r\n");
    if (hdr_end) {
        body = hdr_end + 4;
        body_len = ret - (body - buf);
    }

    /* Decode BMP */
    int img_w, img_h;
    uint32_t *pixels = bmp_decode((const uint8_t *)body, body_len, &img_w, &img_h);
    free(buf);

    if (!pixels) {
        int p = str_append(out, 0, "{\"error\":\"BMP decode failed (only 24/32-bit uncompressed supported)\"}");
        out[p] = '\0';
        return p;
    }

    /* Create window sized to image (plus chrome) */
    int win_w = img_w + 4 + 14;   /* borders + scrollbar */
    int win_h = img_h + 24 + 4;   /* title + borders */
    if (win_w > 800) win_w = 800;
    if (win_h > 600) win_h = 600;

    int id = win_create(title, 50, 30, win_w, win_h,
                        WIN_CLOSABLE | WIN_RESIZABLE);
    if (id < 0) {
        free(pixels);
        int p = str_append(out, 0, "{\"error\":\"no free window slots\"}");
        out[p] = '\0';
        return p;
    }

    win_set_image(id, pixels, img_w, img_h);
    free(pixels);

    int p = 0;
    p = str_append(out, p, "{\"status\":\"displayed\",\"window_id\":");
    p = int_append(out, p, id);
    p = str_append(out, p, ",\"width\":");
    p = int_append(out, p, img_w);
    p = str_append(out, p, ",\"height\":");
    p = int_append(out, p, img_h);
    p = str_append(out, p, "}");
    out[p] = '\0';
    return p;
}

/* ========================================================================= */
/* Tool: ask_input — show input dialog, return user's text                   */
/* ========================================================================= */

static int tool_ask_input(const char *input, char *out, int max) {
    char title[64], prompt[128], placeholder[64];

    if (json_get_string(input, "prompt", prompt, sizeof(prompt)) < 0)
        strncpy(prompt, "Enter value:", sizeof(prompt));
    if (json_get_string(input, "title", title, sizeof(title)) < 0)
        strncpy(title, "Input", sizeof(title));
    if (json_get_string(input, "placeholder", placeholder, sizeof(placeholder)) < 0)
        placeholder[0] = '\0';

    char value[128];
    value[0] = '\0';
    int result = dialog_input(title, prompt, placeholder, value, sizeof(value));

    int p = 0;
    if (result == DIALOG_OK) {
        p = str_append(out, p, "{\"status\":\"ok\",\"value\":\"");
        /* Escape user input for JSON */
        for (int i = 0; value[i] && p < max - 10; i++) {
            if (value[i] == '"')       { out[p++] = '\\'; out[p++] = '"'; }
            else if (value[i] == '\\') { out[p++] = '\\'; out[p++] = '\\'; }
            else if (value[i] == '\n') { out[p++] = '\\'; out[p++] = 'n'; }
            else                       { out[p++] = value[i]; }
        }
        p = str_append(out, p, "\"}");
    } else {
        p = str_append(out, p, "{\"status\":\"cancelled\"}");
    }
    out[p] = '\0';
    return p;
}

/* ========================================================================= */
/* Tool: ask_confirm — show yes/no dialog                                    */
/* ========================================================================= */

static int tool_ask_confirm(const char *input, char *out, int max) {
    char title[64], prompt[128], yes_label[32], no_label[32];

    if (json_get_string(input, "prompt", prompt, sizeof(prompt)) < 0)
        strncpy(prompt, "Are you sure?", sizeof(prompt));
    if (json_get_string(input, "title", title, sizeof(title)) < 0)
        strncpy(title, "Confirm", sizeof(title));
    if (json_get_string(input, "yes_label", yes_label, sizeof(yes_label)) < 0)
        strncpy(yes_label, "Yes", sizeof(yes_label));
    if (json_get_string(input, "no_label", no_label, sizeof(no_label)) < 0)
        strncpy(no_label, "No", sizeof(no_label));

    int result = dialog_confirm(title, prompt, yes_label, no_label);

    int p = 0;
    if (result == DIALOG_OK)
        p = str_append(out, p, "{\"result\":\"yes\"}");
    else
        p = str_append(out, p, "{\"result\":\"no\"}");
    out[p] = '\0';
    return p;
}

/* ========================================================================= */
/* Tool: ask_choice — show radio-button selection dialog                     */
/* ========================================================================= */

static int tool_ask_choice(const char *input, char *out, int max) {
    char title[64], prompt[128], choices[256];

    if (json_get_string(input, "prompt", prompt, sizeof(prompt)) < 0)
        strncpy(prompt, "Select an option:", sizeof(prompt));
    if (json_get_string(input, "title", title, sizeof(title)) < 0)
        strncpy(title, "Choose", sizeof(title));
    if (json_get_string(input, "choices", choices, sizeof(choices)) < 0) {
        int p = str_append(out, 0, "{\"error\":\"missing field: choices (comma-separated)\"}");
        out[p] = '\0';
        return p;
    }

    int def = json_get_int(input, "default");
    if (def < 0) def = 0;

    int result = dialog_choice(title, prompt, choices, def);

    int p = 0;
    if (result >= 0) {
        p = str_append(out, p, "{\"status\":\"ok\",\"index\":");
        p = int_append(out, p, result);
        /* Also include the selected text */
        char items[8][48];
        int nitems = 0;
        int ci = 0;
        for (int i = 0; choices[i] && nitems < 8; i++) {
            if (choices[i] == ',') {
                items[nitems][ci] = '\0'; nitems++; ci = 0;
            } else if (ci < 47) {
                items[nitems][ci++] = choices[i];
            }
        }
        if (ci > 0) { items[nitems][ci] = '\0'; nitems++; }
        if (result < nitems) {
            p = str_append(out, p, ",\"value\":\"");
            p = str_append(out, p, items[result]);
            p = str_append(out, p, "\"");
        }
        p = str_append(out, p, "}");
    } else {
        p = str_append(out, p, "{\"status\":\"cancelled\"}");
    }
    out[p] = '\0';
    return p;
}

/* ========================================================================= */
/* Tool: show_notification — brief popup message                             */
/* ========================================================================= */

static int tool_show_notification(const char *input, char *out, int max) {
    char title[64], message[128], type_str[16];

    if (json_get_string(input, "message", message, sizeof(message)) < 0) {
        int p = str_append(out, 0, "{\"error\":\"missing field: message\"}");
        out[p] = '\0';
        return p;
    }
    if (json_get_string(input, "title", title, sizeof(title)) < 0)
        strncpy(title, "Notice", sizeof(title));

    int type = 0; /* info */
    if (json_get_string(input, "type", type_str, sizeof(type_str)) >= 0) {
        if (strcmp(type_str, "success") == 0) type = 1;
        else if (strcmp(type_str, "warning") == 0) type = 2;
        else if (strcmp(type_str, "error") == 0) type = 3;
    }

    dialog_notify(title, message, type);

    int p = str_append(out, 0, "{\"status\":\"ok\"}");
    out[p] = '\0';
    return p;
}

/* ========================================================================= */
/* Tool: set_theme — change UI color theme                                   */
/* ========================================================================= */

static int tool_set_theme(const char *input, char *out, int max) {
    char name[32];
    int p = 0;

    if (json_get_string(input, "theme", name, sizeof(name)) < 0) {
        /* List available themes */
        p = str_append(out, p, "{\"themes\":[");
        for (int i = 0; i < theme_count(); i++) {
            if (i > 0) out[p++] = ',';
            out[p++] = '"';
            p = str_append(out, p, theme_name(i));
            out[p++] = '"';
        }
        p = str_append(out, p, "],\"current\":\"");
        p = str_append(out, p, theme_name(theme_current_id()));
        p = str_append(out, p, "\"}");
        out[p] = '\0';
        return p;
    }

    /* Find theme by name (case-insensitive compare) */
    int found = -1;
    for (int i = 0; i < theme_count(); i++) {
        const char *tn = theme_name(i);
        /* Simple case-insensitive compare */
        int match = 1;
        for (int j = 0; tn[j] || name[j]; j++) {
            char a = tn[j], b = name[j];
            if (a >= 'A' && a <= 'Z') a += 32;
            if (b >= 'A' && b <= 'Z') b += 32;
            if (a != b) { match = 0; break; }
        }
        if (match) { found = i; break; }
    }

    if (found >= 0) {
        theme_set(found);
        p = str_append(out, p, "{\"status\":\"ok\",\"theme\":\"");
        p = str_append(out, p, theme_name(found));
        p = str_append(out, p, "\"}");
    } else {
        p = str_append(out, p, "{\"error\":\"unknown theme: ");
        p = str_append(out, p, name);
        p = str_append(out, p, "\"}");
    }
    out[p] = '\0';
    return p;
}

/* ========================================================================= */
/* Tool: configure — get/set OS settings                                     */
/* ========================================================================= */

static int tool_configure(const char *input, char *out, int max) {
    char action[16], key[32], value[64];
    int p = 0;

    if (json_get_string(input, "action", action, sizeof(action)) < 0) {
        /* Default: list all settings */
        p = str_append(out, p, "{\"settings\":{");
        char dump[1024];
        int dl = settings_dump(dump, sizeof(dump));
        if (dl > 0) {
            /* Parse the dump into JSON key-value pairs */
            int first = 1;
            char *line = dump;
            while (*line) {
                /* Skip leading spaces */
                while (*line == ' ') line++;
                if (*line == '\0' || *line == '\n') { if (*line) line++; continue; }

                /* Find '=' separator */
                char *eq = line;
                while (*eq && *eq != '=') eq++;
                if (!*eq) break;

                /* Extract key (trim trailing spaces) */
                char *kend = eq - 1;
                while (kend > line && *kend == ' ') kend--;

                /* Extract value (trim leading spaces) */
                char *vstart = eq + 1;
                while (*vstart == ' ') vstart++;
                char *vend = vstart;
                while (*vend && *vend != '\n') vend++;

                if (!first) out[p++] = ',';
                first = 0;
                out[p++] = '"';
                int kl = kend - line + 1;
                if (kl > 0) { memcpy(out + p, line, kl); p += kl; }
                out[p++] = '"'; out[p++] = ':'; out[p++] = '"';
                int vl = vend - vstart;
                if (vl > 0) { memcpy(out + p, vstart, vl); p += vl; }
                out[p++] = '"';

                line = vend;
                if (*line == '\n') line++;
            }
        }
        p = str_append(out, p, "},\"available_keys\":[");
        p = str_append(out, p, "\"theme\",\"keyboard_layout\",\"mouse_sensitivity\"");
        p = str_append(out, p, "]}");
        out[p] = '\0';
        return p;
    }

    if (strcmp(action, "get") == 0) {
        if (json_get_string(input, "key", key, sizeof(key)) < 0) {
            p = str_append(out, 0, "{\"error\":\"missing field: key\"}");
            out[p] = '\0';
            return p;
        }
        const char *v = setting_get(key);
        if (v) {
            p = str_append(out, p, "{\"key\":\"");
            p = str_append(out, p, key);
            p = str_append(out, p, "\",\"value\":\"");
            p = str_append(out, p, v);
            p = str_append(out, p, "\"}");
        } else {
            p = str_append(out, p, "{\"key\":\"");
            p = str_append(out, p, key);
            p = str_append(out, p, "\",\"value\":null}");
        }
        out[p] = '\0';
        return p;
    }

    if (strcmp(action, "set") == 0) {
        if (json_get_string(input, "key", key, sizeof(key)) < 0) {
            p = str_append(out, 0, "{\"error\":\"missing field: key\"}");
            out[p] = '\0';
            return p;
        }
        if (json_get_string(input, "value", value, sizeof(value)) < 0) {
            p = str_append(out, 0, "{\"error\":\"missing field: value\"}");
            out[p] = '\0';
            return p;
        }
        if (setting_set(key, value) < 0) {
            p = str_append(out, 0, "{\"error\":\"settings full\"}");
            out[p] = '\0';
            return p;
        }
        settings_apply();
        settings_save();
        p = str_append(out, p, "{\"status\":\"ok\",\"key\":\"");
        p = str_append(out, p, key);
        p = str_append(out, p, "\",\"value\":\"");
        p = str_append(out, p, value);
        p = str_append(out, p, "\"}");
        out[p] = '\0';
        return p;
    }

    if (strcmp(action, "delete") == 0) {
        if (json_get_string(input, "key", key, sizeof(key)) < 0) {
            p = str_append(out, 0, "{\"error\":\"missing field: key\"}");
            out[p] = '\0';
            return p;
        }
        setting_delete(key);
        settings_apply();
        settings_save();
        p = str_append(out, p, "{\"status\":\"deleted\",\"key\":\"");
        p = str_append(out, p, key);
        p = str_append(out, p, "\"}");
        out[p] = '\0';
        return p;
    }

    p = str_append(out, 0, "{\"error\":\"unknown action. Use: get, set, delete, or omit for list\"}");
    out[p] = '\0';
    return p;
}

/* ========================================================================= */
/* Tool: show_debug_log — display debug log in a window                      */
/* ========================================================================= */

static int tool_show_debug_log(const char *input, char *out, int max) {
    char action[16];
    if (json_get_string(input, "action", action, sizeof(action)) >= 0 &&
        strcmp(action, "clear") == 0) {
        dbg_clear();
        int p = str_append(out, 0, "{\"status\":\"cleared\"}");
        out[p] = '\0';
        return p;
    }

    /* Show the debug log in a window */
    dbg_show();

    int log_len = 0;
    dbg_get_log(&log_len);
    int p = 0;
    p = str_append(out, p, "{\"status\":\"displayed\",\"log_bytes\":");
    p = int_append(out, p, log_len);
    p = str_append(out, p, "}");
    out[p] = '\0';
    return p;
}

/* ========================================================================= */
/* Dynamic tool execution — send implementation+input to LLM                 */
/* ========================================================================= */

static int execute_dynamic_tool(dyn_tool_t *tool, const char *input,
                                 char *out, int max) {
    if (!dyn_tool_llm_cb) {
        int p = str_append(out, 0, "{\"error\":\"no LLM callback set for dynamic tools\"}");
        out[p] = '\0';
        return p;
    }

    /* Build prompt: implementation instructions + user input */
    char *prompt = malloc(1024);
    if (!prompt) {
        int p = str_append(out, 0, "{\"error\":\"out of memory\"}");
        out[p] = '\0';
        return p;
    }

    int pp = 0;
    pp = str_append(prompt, pp, "You are executing a tool called '");
    pp = str_append(prompt, pp, tool->name);
    pp = str_append(prompt, pp, "'.\nDescription: ");
    pp = str_append(prompt, pp, tool->description);
    pp = str_append(prompt, pp, "\nImplementation instructions: ");
    pp = str_append(prompt, pp, tool->implementation);
    pp = str_append(prompt, pp, "\n\nInput: ");
    pp = str_append(prompt, pp, input);
    pp = str_append(prompt, pp, "\n\nExecute these instructions and return the result. "
                    "Respond with ONLY the result, no extra explanation.");
    prompt[pp] = '\0';

    int ret = dyn_tool_llm_cb(prompt, out, max);
    free(prompt);
    return ret;
}

/* ========================================================================= */
/* Disk Persistence — save/load memory + tools to ATA sectors               */
/* ========================================================================= */

/*
   On-disk layout (starting at LBA 2048 — 1MB offset, safe from boot):
     Sector 0:   Header (magic + counts)
     Sector 1-N: mem_store entries  (each entry = 308 bytes, ~1.6 per sector)
     Sector N+1: dyn_tools entries  (each entry = 676 bytes, ~0.75 per sector)

   We just write the arrays raw — they're packed C structs.
   mem_store:  64 entries  → 39 sectors
   dyn_tools:  16 entries  → 22 sectors
   sched:      16 entries  → 6 sectors
   Total: 1 + 39 + 22 + 6 = 68 sectors (~34 KB)
*/

#define PERSIST_LBA_BASE   2048
#define PERSIST_MAGIC      0xA105DA7A   /* "AiOS DATA" */

typedef struct {
    uint32_t magic;
    uint32_t mem_count;
    uint32_t tool_count;
    uint32_t sched_count;
} persist_header_t;

/* Sector math helpers */
#define SECTORS_FOR(bytes) (((bytes) + 511) / 512)
#define MEM_STORE_SIZE   (sizeof(mem_store))
#define DYN_TOOLS_SIZE   (sizeof(dyn_tools))
#define SCHED_SIZE       (SCHED_MAX_EVENTS * sizeof(sched_event_t))
#define MEM_SECTORS      SECTORS_FOR(MEM_STORE_SIZE)
#define TOOL_SECTORS     SECTORS_FOR(DYN_TOOLS_SIZE)
#define SCHED_SECTORS    SECTORS_FOR(SCHED_SIZE)

static void persist_save(void) {
    if (!ata_is_ready()) return;

    /* Write header */
    uint8_t sector[512];
    memset(sector, 0, 512);
    persist_header_t *hdr = (persist_header_t *)sector;
    hdr->magic = PERSIST_MAGIC;
    hdr->mem_count = MEM_SLOTS;
    hdr->tool_count = DYN_TOOL_SLOTS;
    hdr->sched_count = SCHED_MAX_EVENTS;
    ata_write_sectors(PERSIST_LBA_BASE, 1, sector);

    uint32_t lba = PERSIST_LBA_BASE + 1;

    /* Write mem_store */
    uint8_t *pad_buf = malloc(MEM_SECTORS * 512);
    if (!pad_buf) return;
    memset(pad_buf, 0, MEM_SECTORS * 512);
    memcpy(pad_buf, mem_store, MEM_STORE_SIZE);
    ata_write_sectors(lba, MEM_SECTORS, pad_buf);
    free(pad_buf);
    lba += MEM_SECTORS;

    /* Write dyn_tools */
    pad_buf = malloc(TOOL_SECTORS * 512);
    if (!pad_buf) return;
    memset(pad_buf, 0, TOOL_SECTORS * 512);
    memcpy(pad_buf, dyn_tools, DYN_TOOLS_SIZE);
    ata_write_sectors(lba, TOOL_SECTORS, pad_buf);
    free(pad_buf);
    lba += TOOL_SECTORS;

    /* Write scheduler events */
    sched_event_t *sched = scheduler_get_events();
    pad_buf = malloc(SCHED_SECTORS * 512);
    if (!pad_buf) return;
    memset(pad_buf, 0, SCHED_SECTORS * 512);
    memcpy(pad_buf, sched, SCHED_SIZE);
    ata_write_sectors(lba, SCHED_SECTORS, pad_buf);
    free(pad_buf);
}

static void persist_load(void) {
    if (!ata_is_ready()) return;

    /* Read header */
    uint8_t sector[512];
    if (ata_read_sectors(PERSIST_LBA_BASE, 1, sector) < 0) return;
    persist_header_t *hdr = (persist_header_t *)sector;
    if (hdr->magic != PERSIST_MAGIC) return;

    uint32_t lba = PERSIST_LBA_BASE + 1;

    /* Read mem_store */
    uint8_t *pad_buf = malloc(MEM_SECTORS * 512);
    if (!pad_buf) return;
    if (ata_read_sectors(lba, MEM_SECTORS, pad_buf) == 0) {
        memcpy(mem_store, pad_buf, MEM_STORE_SIZE);
    }
    free(pad_buf);
    lba += MEM_SECTORS;

    /* Read dyn_tools */
    pad_buf = malloc(TOOL_SECTORS * 512);
    if (!pad_buf) return;
    if (ata_read_sectors(lba, TOOL_SECTORS, pad_buf) == 0) {
        memcpy(dyn_tools, pad_buf, DYN_TOOLS_SIZE);
    }
    free(pad_buf);
    lba += TOOL_SECTORS;

    /* Read scheduler events */
    if (hdr->sched_count > 0) {
        sched_event_t *sched = scheduler_get_events();
        pad_buf = malloc(SCHED_SECTORS * 512);
        if (!pad_buf) return;
        if (ata_read_sectors(lba, SCHED_SECTORS, pad_buf) == 0) {
            memcpy(sched, pad_buf, SCHED_SIZE);
        }
        free(pad_buf);
    }
}

/* ========================================================================= */
/* Public API                                                                */
/* ========================================================================= */

void tool_executor_init(void) {
    memset(mem_store, 0, sizeof(mem_store));
    memset(dyn_tools, 0, sizeof(dyn_tools));
    persist_load();  /* restore from disk if available */
}

int tool_execute(const char *name, const char *input_json, char *out, int max) {
    int ret;
    int needs_save = 0;

    /* Built-in tools */
    if (strcmp(name, "get_datetime") == 0)
        return tool_get_datetime(out, max);
    if (strcmp(name, "memorize") == 0) {
        ret = tool_memorize(input_json, out, max);
        needs_save = 1;
    } else if (strcmp(name, "recall") == 0)
        return tool_recall(input_json, out, max);
    else if (strcmp(name, "forget") == 0) {
        ret = tool_forget(input_json, out, max);
        needs_save = 1;
    } else if (strcmp(name, "set_api_key") == 0)
        return tool_set_api_key(input_json, out, max);
    else if (strcmp(name, "create_tool") == 0) {
        ret = tool_create_tool(input_json, out, max);
        needs_save = 1;
    } else if (strcmp(name, "delete_tool") == 0) {
        ret = tool_delete_tool(input_json, out, max);
        needs_save = 1;
    } else if (strcmp(name, "list_tools") == 0)
        return tool_list_tools(out, max);
    else if (strcmp(name, "http_request") == 0)
        return tool_http_request(input_json, out, max);
    else if (strcmp(name, "set_reminder") == 0) {
        ret = tool_set_reminder(input_json, out, max);
        needs_save = 1;
    } else if (strcmp(name, "cancel_reminder") == 0) {
        ret = tool_cancel_reminder(input_json, out, max);
        needs_save = 1;
    } else if (strcmp(name, "list_reminders") == 0)
        return tool_list_reminders(out, max);
    else if (strcmp(name, "display_text") == 0)
        return tool_display_text(input_json, out, max);
    else if (strcmp(name, "display_image") == 0)
        return tool_display_image(input_json, out, max);
    else if (strcmp(name, "ask_input") == 0)
        return tool_ask_input(input_json, out, max);
    else if (strcmp(name, "ask_confirm") == 0)
        return tool_ask_confirm(input_json, out, max);
    else if (strcmp(name, "ask_choice") == 0)
        return tool_ask_choice(input_json, out, max);
    else if (strcmp(name, "show_notification") == 0)
        return tool_show_notification(input_json, out, max);
    else if (strcmp(name, "set_theme") == 0)
        return tool_set_theme(input_json, out, max);
    else if (strcmp(name, "show_debug_log") == 0)
        return tool_show_debug_log(input_json, out, max);
    else if (strcmp(name, "configure") == 0)
        return tool_configure(input_json, out, max);
    else {
        /* Dynamic tool lookup */
        for (int i = 0; i < DYN_TOOL_SLOTS; i++) {
            if (dyn_tools[i].active && strcmp(dyn_tools[i].name, name) == 0) {
                return execute_dynamic_tool(&dyn_tools[i], input_json, out, max);
            }
        }
        int p = 0;
        p = str_append(out, p, "{\"error\":\"unknown tool: ");
        p = str_append(out, p, name);
        p = str_append(out, p, "\"}");
        out[p] = '\0';
        return p;
    }

    if (needs_save) persist_save();
    return ret;
}

int tool_memory_dump(char *buf, int max) {
    int p = 0;
    int count = 0;

    for (int i = 0; i < MEM_SLOTS; i++)
        if (mem_store[i].active) count++;

    if (count == 0) {
        p = str_append(buf, p, "Memory empty.\n");
        buf[p] = '\0';
        return p;
    }

    for (int i = 0; i < MEM_SLOTS && p < max - 100; i++) {
        if (!mem_store[i].active) continue;
        p = str_append(buf, p, "  ");
        p = str_append(buf, p, mem_store[i].key);
        p = str_append(buf, p, " = ");
        p = str_append(buf, p, mem_store[i].value);
        buf[p++] = '\n';
    }

    buf[p] = '\0';
    return p;
}

/* ========================================================================= */
/* Test accessors                                                            */
/* ========================================================================= */

int tool_memory_has_key(const char *key, char *value_out, int max) {
    for (int i = 0; i < MEM_SLOTS; i++) {
        if (mem_store[i].active && strcmp(mem_store[i].key, key) == 0) {
            if (value_out) {
                strncpy(value_out, mem_store[i].value, max - 1);
                value_out[max - 1] = '\0';
            }
            return 1;
        }
    }
    return 0;
}

int tool_memory_count(void) {
    int count = 0;
    for (int i = 0; i < MEM_SLOTS; i++)
        if (mem_store[i].active) count++;
    return count;
}

int tool_dyn_tool_exists(const char *name) {
    for (int i = 0; i < DYN_TOOL_SLOTS; i++)
        if (dyn_tools[i].active && strcmp(dyn_tools[i].name, name) == 0)
            return 1;
    return 0;
}

void tool_memory_clear(void) {
    memset(mem_store, 0, sizeof(mem_store));
}

void tool_dyn_tools_clear(void) {
    memset(dyn_tools, 0, sizeof(dyn_tools));
}

void tool_force_save(void) { persist_save(); }
void tool_force_load(void) { persist_load(); }
