/* AiOS Claude API client — calls Anthropic Messages API over HTTP(S) */

#include "lib/lwip/src/include/lwip/tcp.h"
#include "lib/lwip/src/include/lwip/dns.h"
#include "include/string.h"
#include "include/heap.h"
#include "include/io.h"

/* API key — compiled into the kernel (set at build time) */
#ifndef CLAUDE_API_KEY
#define CLAUDE_API_KEY "your-api-key-here"
#endif

#ifndef CLAUDE_MODEL
#define CLAUDE_MODEL "claude-sonnet-4-20250514"
#endif

#define API_HOST    "api.anthropic.com"
#define API_PATH    "/v1/messages"
#define API_PORT    443  /* HTTPS — will need TLS wrapper */
#define PROXY_PORT  80   /* For LAN proxy mode (plain HTTP) */

/* Forward declaration */
extern int http_post(ip_addr_t *server_ip, uint16_t port,
                     const char *host, const char *path,
                     const char *headers, const char *body,
                     char *resp_buf, int resp_max);
extern void net_poll(void);

/* DNS resolution state */
static ip_addr_t resolved_ip;
static int dns_done;
static int dns_ok;

static void dns_cb(const char *name, const ip_addr_t *addr, void *arg) {
    (void)name;
    (void)arg;
    dns_done = 1;
    if (addr) {
        resolved_ip = *addr;
        dns_ok = 1;
    } else {
        dns_ok = 0;
    }
}

static int resolve_host(const char *hostname, ip_addr_t *out) {
    dns_done = 0;
    dns_ok = 0;

    err_t err = dns_gethostbyname(hostname, out, dns_cb, NULL);
    if (err == ERR_OK) return 0; /* Cached */
    if (err != ERR_INPROGRESS) return -1;

    /* Wait for DNS */
    int timeout = 500;
    while (!dns_done && timeout-- > 0)
        net_poll();

    if (dns_ok) {
        *out = resolved_ip;
        return 0;
    }
    return -1;
}

/* Simple JSON string escaping */
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

/* Extract text from Claude API JSON response.
   Looks for: "text":"..." in the response body */
static int extract_response_text(const char *json, char *out, int max_len) {
    /* Find "type":"text" then "text":"..." */
    const char *p = json;
    while ((p = strstr(p, "\"text\":\"")) != NULL) {
        p += 8; /* skip "text":" */
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

/* LAN proxy configuration */
static ip_addr_t proxy_ip;
static uint16_t  proxy_port = 0;
static int       use_proxy = 0;

void claude_set_proxy(uint32_t ip, uint16_t port) {
    ip_addr_set_ip4_u32(&proxy_ip, ip);
    proxy_port = port;
    use_proxy = 1;
}

int claude_ask(const char *question, char *response, int max_len) {
    /* Build JSON body */
    char *body = malloc(2048);
    char *escaped = malloc(1024);
    char *resp_buf = malloc(8192);
    if (!body || !escaped || !resp_buf) {
        if (body) free(body);
        if (escaped) free(escaped);
        if (resp_buf) free(resp_buf);
        return -1;
    }

    json_escape(question, escaped, 1024);

    /* Build request body */
    int pos = 0;
    const char *s;
    s = "{\"model\":\"" CLAUDE_MODEL "\",\"max_tokens\":512,";
    memcpy(body + pos, s, strlen(s)); pos += strlen(s);
    s = "\"system\":\"You are AiOS, an AI-native operating system assistant. "
        "Keep responses concise and terminal-friendly, max 78 chars wide. No markdown.\",";
    memcpy(body + pos, s, strlen(s)); pos += strlen(s);
    s = "\"messages\":[{\"role\":\"user\",\"content\":\"";
    memcpy(body + pos, s, strlen(s)); pos += strlen(s);
    memcpy(body + pos, escaped, strlen(escaped)); pos += strlen(escaped);
    s = "\"}]}";
    memcpy(body + pos, s, strlen(s)); pos += strlen(s);
    body[pos] = '\0';

    /* Build headers */
    char headers[512];
    int hpos = 0;
    s = "Content-Type: application/json\r\n";
    memcpy(headers + hpos, s, strlen(s)); hpos += strlen(s);
    s = "x-api-key: " CLAUDE_API_KEY "\r\n";
    memcpy(headers + hpos, s, strlen(s)); hpos += strlen(s);
    s = "anthropic-version: 2023-06-01\r\n";
    memcpy(headers + hpos, s, strlen(s)); hpos += strlen(s);
    headers[hpos] = '\0';

    /* Resolve API host */
    ip_addr_t server_ip;
    uint16_t port;
    const char *host;

    if (use_proxy) {
        server_ip = proxy_ip;
        port = proxy_port;
        host = API_HOST;
    } else {
        if (resolve_host(API_HOST, &server_ip) != 0) {
            free(body); free(escaped); free(resp_buf);
            return -1;
        }
        port = PROXY_PORT; /* Plain HTTP for now, TLS added later */
        host = API_HOST;
    }

    /* Send request */
    int ret = http_post(&server_ip, port, host, API_PATH,
                        headers, body, resp_buf, 8192);

    free(body);
    free(escaped);

    if (ret <= 0) {
        free(resp_buf);
        return -1;
    }

    /* Extract response text from JSON */
    int text_len = extract_response_text(resp_buf, response, max_len);
    free(resp_buf);

    return text_len;
}
