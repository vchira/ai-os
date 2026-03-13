/* AiOS LLM Provider System — multi-backend AI routing
   Supports: Claude (via proxy), OpenAI (via proxy), Ollama (direct HTTP) */

#include "lib/lwip/src/include/lwip/tcp.h"
#include "lib/lwip/src/include/lwip/dns.h"
#include "lib/lwip/src/include/lwip/ip4_addr.h"
#include "include/string.h"
#include "include/heap.h"
#include "include/io.h"
#include "include/net.h"

/* Build-time API keys — set in .env or command line */
#ifndef CLAUDE_API_KEY
#define CLAUDE_API_KEY "your-api-key-here"
#endif
#ifndef OPENAI_API_KEY
#define OPENAI_API_KEY "your-api-key-here"
#endif

/* Keyboard layout info for system prompt */
extern int keyboard_get_layout(void);
extern const char *keyboard_get_layout_name(int id);
extern int keyboard_get_num_layouts(void);

extern int http_post(ip_addr_t *server_ip, uint16_t port,
                     const char *host, const char *path,
                     const char *headers, const char *body,
                     char *resp_buf, int resp_max);
extern void net_poll(void);
extern u32_t sys_now(void);

/* ========================================================================= */
/* Provider definitions                                                      */
/* ========================================================================= */

#define MAX_PROVIDERS 4

typedef struct {
    const char *name;           /* Display name */
    const char *model;          /* Model string */
    uint8_t    ip[4];           /* Server IP */
    uint16_t   port;            /* Server port */
    const char *host;           /* HTTP Host header */
    const char *path;           /* API endpoint path */
    int        needs_proxy;     /* 1 = needs HTTPS proxy, 0 = plain HTTP */
    int        configured;      /* 1 = has valid API key / reachable */
} llm_provider_t;

static llm_provider_t providers[MAX_PROVIDERS];
static int num_providers = 0;
static int active_provider = -1;

/* Proxy config for HTTPS providers */
static uint8_t proxy_ip[4] = {10, 0, 2, 2};  /* SLIRP gateway */
static uint16_t proxy_port = 8080;

/* ========================================================================= */
/* Shared utilities                                                          */
/* ========================================================================= */

static int set_error(char *response, int max_len, const char *msg) {
    int len = strlen(msg);
    if (len >= max_len) len = max_len - 1;
    memcpy(response, msg, len);
    response[len] = '\0';
    return -1;
}

static int json_escape(const char *src, char *dst, int max) {
    int i = 0;
    while (*src && i < max - 2) {
        if (*src == '"' || *src == '\\') {
            dst[i++] = '\\';
        } else if (*src == '\n') {
            dst[i++] = '\\'; dst[i++] = 'n'; src++; continue;
        } else if (*src == '\r') {
            dst[i++] = '\\'; dst[i++] = 'r'; src++; continue;
        } else if (*src == '\t') {
            dst[i++] = '\\'; dst[i++] = 't'; src++; continue;
        }
        dst[i++] = *src++;
    }
    dst[i] = '\0';
    return i;
}

/* Extract JSON string value after a key like "text":"..." or "content":"..." */
static int extract_json_string(const char *json, const char *key, char *out, int max_len) {
    const char *p = json;
    int key_len = strlen(key);
    while ((p = strstr(p, key)) != NULL) {
        p += key_len;
        if (*p != '"') { p++; continue; }
        p++; /* skip opening quote */
        int i = 0;
        while (*p && *p != '"' && i < max_len - 1) {
            if (*p == '\\' && *(p + 1)) {
                p++;
                switch (*p) {
                    case 'n': out[i++] = '\n'; break;
                    case 'r': break;
                    case 't': out[i++] = '\t'; break;
                    case '"': out[i++] = '"'; break;
                    case '\\': out[i++] = '\\'; break;
                    default: out[i++] = *p; break;
                }
            } else {
                out[i++] = *p;
            }
            p++;
        }
        out[i] = '\0';
        return i;
    }
    return -1;
}

/* Build the AiOS system prompt with live context */
static int build_system_prompt(char *buf, int max) {
    int sp = 0;
    const char *t;

    t = "You are the AI running inside AiOS, a bare-metal x86 operating system. "
        "You are NOT running on Linux, macOS, or Windows. "
        "You run directly on hardware. You have direct control over this OS. "
        "Keep responses concise and terminal-friendly, max 78 chars wide. "
        "No markdown formatting.\\n\\n"
        "CAPABILITIES you can use (tell the user to type these):\\n"
        "/keyboard <id> - Switch keyboard layout\\n"
        "/provider <id> - Switch AI provider\\n"
        "/shell - Enter system shell\\n"
        "/net - Show network status\\n"
        "/color <hex> - Change text color\\n"
        "/meminfo, /cpuinfo, /uptime - System info\\n"
        "/clear, /reboot, /halt - System control\\n\\n";
    memcpy(buf + sp, t, strlen(t)); sp += strlen(t);

    t = "CURRENT STATE:\\n"
        "Keyboard layout: ";
    memcpy(buf + sp, t, strlen(t)); sp += strlen(t);

    int cur_layout = keyboard_get_layout();
    const char *layout_name = keyboard_get_layout_name(cur_layout);
    if (layout_name) {
        memcpy(buf + sp, layout_name, strlen(layout_name));
        sp += strlen(layout_name);
    }
    t = " (id=";
    memcpy(buf + sp, t, strlen(t)); sp += strlen(t);
    buf[sp++] = '0' + cur_layout;
    t = ")\\nAvailable layouts: ";
    memcpy(buf + sp, t, strlen(t)); sp += strlen(t);

    int num_layouts = keyboard_get_num_layouts();
    for (int i = 0; i < num_layouts; i++) {
        if (i > 0) { buf[sp++] = ','; buf[sp++] = ' '; }
        buf[sp++] = '0' + i;
        buf[sp++] = '=';
        const char *ln = keyboard_get_layout_name(i);
        if (ln) { memcpy(buf + sp, ln, strlen(ln)); sp += strlen(ln); }
    }

    t = "\\nAI Provider: ";
    memcpy(buf + sp, t, strlen(t)); sp += strlen(t);
    if (active_provider >= 0 && active_provider < num_providers) {
        const char *pn = providers[active_provider].name;
        memcpy(buf + sp, pn, strlen(pn)); sp += strlen(pn);
    }

    t = "\\nNetwork: ";
    memcpy(buf + sp, t, strlen(t)); sp += strlen(t);
    t = net_is_up() ? "Up" : "Down";
    memcpy(buf + sp, t, strlen(t)); sp += strlen(t);

    t = "\\n\\nWhen the user asks to change keyboard layout, tell them to type "
        "/keyboard <id> with the correct id. Do NOT suggest Linux/macOS/Windows commands.";
    memcpy(buf + sp, t, strlen(t)); sp += strlen(t);
    buf[sp] = '\0';
    return sp;
}

static int decode_http_error(int ret, char *response, int max_len) {
    if (ret == -10) return set_error(response, max_len, "TCP socket allocation failed.");
    if (ret == -11) return set_error(response, max_len, "TCP connect failed.");
    if (ret == -12) return set_error(response, max_len, "Connection refused or reset.");
    if (ret == -13) return set_error(response, max_len, "Connection timed out.");
    if (ret == -14) return set_error(response, max_len, "Server error during response.");
    if (ret == -15) return set_error(response, max_len, "Empty response from server.");
    return set_error(response, max_len, "HTTP request failed (unknown error).");
}

/* ========================================================================= */
/* Provider: Claude (Anthropic)                                              */
/* ========================================================================= */

static int claude_ask_impl(const char *question, char *response, int max_len,
                           const char *model) {
    if (strcmp(CLAUDE_API_KEY, "your-api-key-here") == 0)
        return set_error(response, max_len,
            "No Claude API key. Set CLAUDE_API_KEY in .env");

    char *body = malloc(4096);
    char *escaped = malloc(1024);
    char *resp_buf = malloc(8192);
    if (!body || !escaped || !resp_buf) {
        if (body) free(body);
        if (escaped) free(escaped);
        if (resp_buf) free(resp_buf);
        return set_error(response, max_len, "Out of heap memory.");
    }

    json_escape(question, escaped, 1024);

    char sys_prompt[1024];
    int sp = build_system_prompt(sys_prompt, sizeof(sys_prompt));

    /* Build Claude API body */
    int pos = 0;
    const char *s;
    s = "{\"model\":\"";
    memcpy(body + pos, s, strlen(s)); pos += strlen(s);
    memcpy(body + pos, model, strlen(model)); pos += strlen(model);
    s = "\",\"max_tokens\":512,\"system\":\"";
    memcpy(body + pos, s, strlen(s)); pos += strlen(s);
    memcpy(body + pos, sys_prompt, sp); pos += sp;
    s = "\",\"messages\":[{\"role\":\"user\",\"content\":\"";
    memcpy(body + pos, s, strlen(s)); pos += strlen(s);
    memcpy(body + pos, escaped, strlen(escaped)); pos += strlen(escaped);
    s = "\"}]}";
    memcpy(body + pos, s, strlen(s)); pos += strlen(s);
    body[pos] = '\0';

    /* Headers */
    char headers[512];
    int hpos = 0;
    s = "Content-Type: application/json\r\n";
    memcpy(headers + hpos, s, strlen(s)); hpos += strlen(s);
    s = "x-api-key: " CLAUDE_API_KEY "\r\n";
    memcpy(headers + hpos, s, strlen(s)); hpos += strlen(s);
    s = "anthropic-version: 2023-06-01\r\n";
    memcpy(headers + hpos, s, strlen(s)); hpos += strlen(s);
    headers[hpos] = '\0';

    /* Connect via proxy */
    ip_addr_t server_ip;
    IP4_ADDR(&server_ip, proxy_ip[0], proxy_ip[1], proxy_ip[2], proxy_ip[3]);

    int ret = http_post(&server_ip, proxy_port, "api.anthropic.com",
                        "/v1/messages", headers, body, resp_buf, 8192);
    free(body);
    free(escaped);

    if (ret <= 0) { free(resp_buf); return decode_http_error(ret, response, max_len); }

    int text_len = extract_json_string(resp_buf, "\"text\":\"", response, max_len);
    if (text_len < 0) {
        int raw_len = strlen(resp_buf);
        if (raw_len > max_len - 30) raw_len = max_len - 30;
        s = "Could not parse response:\n";
        memcpy(response, s, strlen(s));
        memcpy(response + strlen(s), resp_buf, raw_len);
        response[strlen(s) + raw_len] = '\0';
        free(resp_buf);
        return -1;
    }
    free(resp_buf);
    return text_len;
}

/* ========================================================================= */
/* Provider: OpenAI                                                          */
/* ========================================================================= */

static int openai_ask_impl(const char *question, char *response, int max_len,
                           const char *model) {
    if (strcmp(OPENAI_API_KEY, "your-api-key-here") == 0)
        return set_error(response, max_len,
            "No OpenAI API key. Set OPENAI_API_KEY in .env");

    char *body = malloc(4096);
    char *escaped = malloc(1024);
    char *resp_buf = malloc(8192);
    if (!body || !escaped || !resp_buf) {
        if (body) free(body);
        if (escaped) free(escaped);
        if (resp_buf) free(resp_buf);
        return set_error(response, max_len, "Out of heap memory.");
    }

    json_escape(question, escaped, 1024);

    char sys_prompt[1024];
    int sp = build_system_prompt(sys_prompt, sizeof(sys_prompt));

    /* OpenAI chat completion body */
    int pos = 0;
    const char *s;
    s = "{\"model\":\"";
    memcpy(body + pos, s, strlen(s)); pos += strlen(s);
    memcpy(body + pos, model, strlen(model)); pos += strlen(model);
    s = "\",\"max_tokens\":512,\"messages\":["
        "{\"role\":\"system\",\"content\":\"";
    memcpy(body + pos, s, strlen(s)); pos += strlen(s);
    memcpy(body + pos, sys_prompt, sp); pos += sp;
    s = "\"},{\"role\":\"user\",\"content\":\"";
    memcpy(body + pos, s, strlen(s)); pos += strlen(s);
    memcpy(body + pos, escaped, strlen(escaped)); pos += strlen(escaped);
    s = "\"}]}";
    memcpy(body + pos, s, strlen(s)); pos += strlen(s);
    body[pos] = '\0';

    /* Headers */
    char headers[512];
    int hpos = 0;
    s = "Content-Type: application/json\r\n";
    memcpy(headers + hpos, s, strlen(s)); hpos += strlen(s);
    s = "Authorization: Bearer " OPENAI_API_KEY "\r\n";
    memcpy(headers + hpos, s, strlen(s)); hpos += strlen(s);
    headers[hpos] = '\0';

    /* Connect via proxy (OpenAI also needs HTTPS) */
    ip_addr_t server_ip;
    IP4_ADDR(&server_ip, proxy_ip[0], proxy_ip[1], proxy_ip[2], proxy_ip[3]);

    int ret = http_post(&server_ip, proxy_port, "api.openai.com",
                        "/v1/chat/completions", headers, body, resp_buf, 8192);
    free(body);
    free(escaped);

    if (ret <= 0) { free(resp_buf); return decode_http_error(ret, response, max_len); }

    /* OpenAI response: "content":"..." */
    int text_len = extract_json_string(resp_buf, "\"content\":\"", response, max_len);
    if (text_len < 0) {
        int raw_len = strlen(resp_buf);
        if (raw_len > max_len - 30) raw_len = max_len - 30;
        s = "Could not parse response:\n";
        memcpy(response, s, strlen(s));
        memcpy(response + strlen(s), resp_buf, raw_len);
        response[strlen(s) + raw_len] = '\0';
        free(resp_buf);
        return -1;
    }
    free(resp_buf);
    return text_len;
}

/* ========================================================================= */
/* Provider: Ollama (LAN — plain HTTP, no proxy/TLS needed!)                 */
/* ========================================================================= */

static int ollama_ask_impl(const char *question, char *response, int max_len,
                           const char *model, const uint8_t *host_ip, uint16_t port) {
    char *body = malloc(4096);
    char *escaped = malloc(1024);
    char *resp_buf = malloc(8192);
    if (!body || !escaped || !resp_buf) {
        if (body) free(body);
        if (escaped) free(escaped);
        if (resp_buf) free(resp_buf);
        return set_error(response, max_len, "Out of heap memory.");
    }

    json_escape(question, escaped, 1024);

    char sys_prompt[1024];
    int sp = build_system_prompt(sys_prompt, sizeof(sys_prompt));

    /* Ollama /api/chat body (OpenAI-compatible chat format) */
    int pos = 0;
    const char *s;
    s = "{\"model\":\"";
    memcpy(body + pos, s, strlen(s)); pos += strlen(s);
    memcpy(body + pos, model, strlen(model)); pos += strlen(model);
    s = "\",\"stream\":false,\"messages\":["
        "{\"role\":\"system\",\"content\":\"";
    memcpy(body + pos, s, strlen(s)); pos += strlen(s);
    memcpy(body + pos, sys_prompt, sp); pos += sp;
    s = "\"},{\"role\":\"user\",\"content\":\"";
    memcpy(body + pos, s, strlen(s)); pos += strlen(s);
    memcpy(body + pos, escaped, strlen(escaped)); pos += strlen(escaped);
    s = "\"}]}";
    memcpy(body + pos, s, strlen(s)); pos += strlen(s);
    body[pos] = '\0';

    /* Minimal headers — Ollama needs no auth */
    char headers[128];
    int hpos = 0;
    s = "Content-Type: application/json\r\n";
    memcpy(headers + hpos, s, strlen(s)); hpos += strlen(s);
    headers[hpos] = '\0';

    /* Direct connection — plain HTTP! */
    ip_addr_t server_ip;
    IP4_ADDR(&server_ip, host_ip[0], host_ip[1], host_ip[2], host_ip[3]);

    int ret = http_post(&server_ip, port, "ollama",
                        "/api/chat", headers, body, resp_buf, 8192);
    free(body);
    free(escaped);

    if (ret <= 0) { free(resp_buf); return decode_http_error(ret, response, max_len); }

    /* Ollama /api/chat response: "content":"..." inside message object */
    int text_len = extract_json_string(resp_buf, "\"content\":\"", response, max_len);
    if (text_len < 0) {
        int raw_len = strlen(resp_buf);
        if (raw_len > max_len - 30) raw_len = max_len - 30;
        s = "Could not parse response:\n";
        memcpy(response, s, strlen(s));
        memcpy(response + strlen(s), resp_buf, raw_len);
        response[strlen(s) + raw_len] = '\0';
        free(resp_buf);
        return -1;
    }
    free(resp_buf);
    return text_len;
}

/* ========================================================================= */
/* Provider registry — public API                                            */
/* ========================================================================= */

void llm_init(void) {
    num_providers = 0;

    /* Provider 0: Claude (via proxy) */
    providers[0].name = "Claude";
    providers[0].model = "claude-sonnet-4-20250514";
    providers[0].ip[0] = proxy_ip[0]; providers[0].ip[1] = proxy_ip[1];
    providers[0].ip[2] = proxy_ip[2]; providers[0].ip[3] = proxy_ip[3];
    providers[0].port = proxy_port;
    providers[0].host = "api.anthropic.com";
    providers[0].path = "/v1/messages";
    providers[0].needs_proxy = 1;
    providers[0].configured = (strcmp(CLAUDE_API_KEY, "your-api-key-here") != 0);
    num_providers++;

    /* Provider 1: OpenAI (via proxy) */
    providers[1].name = "OpenAI";
    providers[1].model = "gpt-4o-mini";
    providers[1].ip[0] = proxy_ip[0]; providers[1].ip[1] = proxy_ip[1];
    providers[1].ip[2] = proxy_ip[2]; providers[1].ip[3] = proxy_ip[3];
    providers[1].port = proxy_port;
    providers[1].host = "api.openai.com";
    providers[1].path = "/v1/chat/completions";
    providers[1].needs_proxy = 1;
    providers[1].configured = (strcmp(OPENAI_API_KEY, "your-api-key-here") != 0);
    num_providers++;

    /* Provider 2: Ollama (LAN — direct HTTP) */
    providers[2].name = "Ollama";
    providers[2].model = "llama3.2";
    providers[2].ip[0] = 10; providers[2].ip[1] = 0;
    providers[2].ip[2] = 2;  providers[2].ip[3] = 2;  /* Host via SLIRP */
    providers[2].port = 11434;
    providers[2].host = "ollama";
    providers[2].path = "/api/chat";
    providers[2].needs_proxy = 0;  /* Plain HTTP! */
    providers[2].configured = 1;   /* No key needed */
    num_providers++;

    /* Default to first configured provider */
    active_provider = -1;
    for (int i = 0; i < num_providers; i++) {
        if (providers[i].configured) {
            active_provider = i;
            break;
        }
    }
}

int llm_get_num_providers(void) {
    return num_providers;
}

int llm_get_active(void) {
    return active_provider;
}

int llm_set_active(int id) {
    if (id < 0 || id >= num_providers) return -1;
    active_provider = id;
    return 0;
}

const char *llm_get_provider_name(int id) {
    if (id < 0 || id >= num_providers) return 0;
    return providers[id].name;
}

const char *llm_get_provider_model(int id) {
    if (id < 0 || id >= num_providers) return 0;
    return providers[id].model;
}

int llm_is_configured(int id) {
    if (id < 0 || id >= num_providers) return 0;
    return providers[id].configured;
}

/* The main entry point — replaces claude_ask */
int llm_ask(const char *question, char *response, int max_len) {
    if (!net_is_up())
        return set_error(response, max_len,
            "No network. DHCP has not assigned an IP yet.\n"
            "Wait a few seconds and try again, or use /net to check.");

    if (active_provider < 0)
        return set_error(response, max_len,
            "No AI provider configured.\n"
            "Set API keys in .env or use /provider to select Ollama.");

    llm_provider_t *p = &providers[active_provider];

    /* Route to the right backend */
    if (active_provider == 0) {
        return claude_ask_impl(question, response, max_len, p->model);
    } else if (active_provider == 1) {
        return openai_ask_impl(question, response, max_len, p->model);
    } else if (active_provider == 2) {
        return ollama_ask_impl(question, response, max_len,
                               p->model, p->ip, p->port);
    }

    return set_error(response, max_len, "Unknown provider.");
}

/* Keep claude_ask as backward-compatible wrapper */
int claude_ask(const char *question, char *response, int max_len) {
    return llm_ask(question, response, max_len);
}
