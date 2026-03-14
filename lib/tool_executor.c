/* AiOS — Tool Executor
   Dispatches LLM tool calls to OS primitives.
   Provides AI with general-purpose memory (RAM-backed, persistent store is P1).

   Tools available to the AI:
     memorize  — store key→value
     recall    — retrieve value by key, or list all keys
     forget    — delete an entry
     get_datetime — read hardware clock
*/

#include "include/tool_executor.h"
#include "include/rtc.h"
#include "include/string.h"

/* ========================================================================= */
/* AI Memory Store — in-RAM key-value pairs                                  */
/* When ATA driver + block store arrive, this swaps to disk-backed.          */
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
/* Public API                                                                */
/* ========================================================================= */

void tool_executor_init(void) {
    memset(mem_store, 0, sizeof(mem_store));
}

int tool_execute(const char *name, const char *input_json, char *out, int max) {
    if (strcmp(name, "get_datetime") == 0)
        return tool_get_datetime(out, max);
    if (strcmp(name, "memorize") == 0)
        return tool_memorize(input_json, out, max);
    if (strcmp(name, "recall") == 0)
        return tool_recall(input_json, out, max);
    if (strcmp(name, "forget") == 0)
        return tool_forget(input_json, out, max);
    if (strcmp(name, "set_api_key") == 0)
        return tool_set_api_key(input_json, out, max);

    int p = 0;
    p = str_append(out, p, "{\"error\":\"unknown tool: ");
    p = str_append(out, p, name);
    p = str_append(out, p, "\"}");
    out[p] = '\0';
    return p;
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
