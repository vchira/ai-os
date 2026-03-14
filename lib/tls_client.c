/* AiOS — TLS HTTPS client with persistent connections
   Wraps mbedTLS around lwIP raw TCP for direct HTTPS to API servers.
   Keeps connections alive — no TLS handshake per request after the first. */

#include "include/tls_client.h"
#include "include/string.h"
#include "include/heap.h"
#include "include/io.h"
#include "include/net.h"

#include "lib/lwip/src/include/lwip/tcp.h"
#include "lib/lwip/src/include/lwip/dns.h"
#include "lib/lwip/src/include/lwip/ip4_addr.h"
#include "lib/lwip/src/include/lwip/pbuf.h"

#include "mbedtls/ssl.h"
#include "mbedtls/ctr_drbg.h"
#include "mbedtls/entropy.h"

extern void net_poll(void);
extern u32_t sys_now(void);

#define TLS_RECV_BUF   32768   /* 32KB — must hold TLS records + some slack */
#define TLS_TIMEOUT_MS 30000

/* Debug helpers */
#include "include/stdio.h"
#include "include/debug_log.h"

#if AIOS_DEBUG
#define DBG(fmt, ...) do { \
    char _dbg[140]; \
    snprintf(_dbg, sizeof(_dbg), "[%lu] " fmt, (unsigned long)sys_now(), ##__VA_ARGS__); \
    dbg_log(_dbg); \
} while(0)
#else
#define DBG(fmt, ...) ((void)0)
#endif

/* ========================================================================= */
/* Hardware entropy for mbedTLS (RDTSC-based)                                */
/* ========================================================================= */

int mbedtls_hardware_poll(void *data, unsigned char *output,
                          size_t len, size_t *olen) {
    (void)data;
    size_t i = 0;
    while (i < len) {
        uint32_t lo, hi;
        __asm__ volatile("rdtsc" : "=a"(lo), "=d"(hi));
        lo ^= hi;
        lo ^= sys_now();
        size_t chunk = (len - i) < 4 ? (len - i) : 4;
        memcpy(output + i, &lo, chunk);
        i += chunk;
        for (volatile int j = 0; j < 50; j++);
    }
    *olen = len;
    return 0;
}

/* ========================================================================= */
/* Global TLS context (reused across connections)                            */
/* ========================================================================= */

static mbedtls_entropy_context    g_entropy;
static mbedtls_ctr_drbg_context   g_ctr_drbg;
static int g_tls_ready = 0;

void tls_init(void) {
    DBG("tls_init: seeding RNG...\n");
    mbedtls_entropy_init(&g_entropy);
    mbedtls_ctr_drbg_init(&g_ctr_drbg);

    int ret = mbedtls_ctr_drbg_seed(&g_ctr_drbg, mbedtls_entropy_func,
                                    &g_entropy, NULL, 0);
    if (ret == 0) {
        g_tls_ready = 1;
        DBG("tls_init: RNG ready\n");
    } else {
        DBG("tls_init: RNG seed FAILED ret=%d\n", ret);
    }
}

/* ========================================================================= */
/* TCP transport layer for mbedTLS                                           */
/* ========================================================================= */

typedef struct {
    struct tcp_pcb *pcb;
    uint8_t  recv_buf[TLS_RECV_BUF];
    volatile int recv_wr;
    volatile int recv_rd;
    volatile int connected;
    volatile int error;
    volatile int remote_closed;
} tls_tcp_t;

/* --- lwIP TCP callbacks --- */

static err_t tls_connected_cb(void *arg, struct tcp_pcb *pcb, err_t err) {
    tls_tcp_t *t = (tls_tcp_t *)arg;
    (void)pcb;
    if (err == ERR_OK) {
        t->connected = 1;
        DBG("tcp: connected OK\n");
    } else {
        t->error = 1;
        DBG("tcp: connect err=%d\n", (int)err);
    }
    return ERR_OK;
}

static err_t tls_recv_cb(void *arg, struct tcp_pcb *pcb, struct pbuf *p, err_t err) {
    tls_tcp_t *t = (tls_tcp_t *)arg;
    (void)err;

    if (!p) {
        t->remote_closed = 1;
        DBG("tcp: remote closed\n");
        return ERR_OK;
    }

    int total_copied = 0;
    struct pbuf *q;
    for (q = p; q != NULL; q = q->next) {
        const uint8_t *src = (const uint8_t *)q->payload;
        for (int i = 0; i < (int)q->len; i++) {
            int next_wr = (t->recv_wr + 1) % TLS_RECV_BUF;
            if (next_wr == t->recv_rd) break;  /* buffer full — drop */
            t->recv_buf[t->recv_wr] = src[i];
            t->recv_wr = next_wr;
            total_copied++;
        }
    }

    int pbuf_total = p->tot_len;
    tcp_recved(pcb, p->tot_len);
    /* CRITICAL: Force immediate ACK — without TF_ACK_NOW, lwIP only sets
       TF_ACK_DELAY and tcp_output() won't send the ACK. The server stalls
       waiting for our ACK before sending the rest of chunked responses. */
    tcp_set_flags(pcb, TF_ACK_NOW);
    tcp_output(pcb);
    pbuf_free(p);

    DBG("tcp: recv %d bytes (pbuf %d)\n", total_copied, pbuf_total);
    return ERR_OK;
}

static void tls_err_cb(void *arg, err_t err) {
    tls_tcp_t *t = (tls_tcp_t *)arg;
    (void)err;
    t->error = 1;
    t->pcb = NULL;  /* lwIP already freed the PCB on error */
    DBG("tcp: err callback err=%d\n", (int)err);
}

/* --- mbedTLS BIO callbacks --- */

static int tls_bio_send(void *ctx, const unsigned char *buf, size_t len) {
    tls_tcp_t *t = (tls_tcp_t *)ctx;
    if (t->error || !t->pcb) return MBEDTLS_ERR_SSL_INTERNAL_ERROR;

    size_t written = 0;
    u32_t start = sys_now();
    while (written < len) {
        if (t->error) return MBEDTLS_ERR_SSL_INTERNAL_ERROR;
        if (sys_now() - start > TLS_TIMEOUT_MS) return MBEDTLS_ERR_SSL_TIMEOUT;

        u16_t sndbuf = tcp_sndbuf(t->pcb);
        if (sndbuf == 0) {
            net_poll();
            __asm__ volatile("hlt");
            continue;
        }

        u16_t chunk = (u16_t)((len - written) > sndbuf ? sndbuf : (len - written));
        err_t err = tcp_write(t->pcb, buf + written, chunk, TCP_WRITE_FLAG_COPY);
        if (err != ERR_OK) return MBEDTLS_ERR_SSL_INTERNAL_ERROR;
        written += chunk;
    }
    tcp_output(t->pcb);
    return (int)len;
}

static int tls_bio_recv(void *ctx, unsigned char *buf, size_t len) {
    tls_tcp_t *t = (tls_tcp_t *)ctx;
    u32_t start = sys_now();

    while (t->recv_rd == t->recv_wr) {
        if (t->error) return MBEDTLS_ERR_SSL_INTERNAL_ERROR;
        if (t->remote_closed) return 0;
        if (sys_now() - start > TLS_TIMEOUT_MS) {
            DBG("bio_recv: TIMEOUT after %lums\n", (unsigned long)(sys_now() - start));
            return MBEDTLS_ERR_SSL_TIMEOUT;
        }
        net_poll();
        /* Belt-and-suspenders: force-flush any pending ACKs */
        if (t->pcb) {
            tcp_set_flags(t->pcb, TF_ACK_NOW);
            tcp_output(t->pcb);
        }
        __asm__ volatile("hlt");
    }

    int copied = 0;
    while (copied < (int)len && t->recv_rd != t->recv_wr) {
        buf[copied++] = t->recv_buf[t->recv_rd];
        t->recv_rd = (t->recv_rd + 1) % TLS_RECV_BUF;
    }
    return copied;
}

/* ========================================================================= */
/* DNS resolution helper with caching                                        */
/* ========================================================================= */

static volatile int dns_done;
static ip_addr_t    dns_result;

/* DNS cache — avoids re-resolving the same hostname */
static char      dns_cache_host[64];
static ip_addr_t dns_cache_ip;
static int       dns_cache_valid = 0;

static void dns_cb(const char *name, const ip_addr_t *addr, void *arg) {
    (void)name; (void)arg;
    if (addr) {
        ip_addr_copy(dns_result, *addr);
        dns_done = 1;
    } else {
        dns_done = -1;
    }
}

static int resolve_host(const char *hostname, ip_addr_t *out) {
    /* Check cache first */
    if (dns_cache_valid && strcmp(hostname, dns_cache_host) == 0) {
        ip_addr_copy(*out, dns_cache_ip);
        DBG("dns: cache hit for %s\n", hostname);
        return 0;
    }

    DBG("dns: resolving %s...\n", hostname);

    /* Ensure network stack is fresh */
    for (int i = 0; i < 10; i++) net_poll();

    dns_done = 0;
    err_t err = dns_gethostbyname(hostname, out, dns_cb, NULL);
    if (err == ERR_OK) {
        DBG("dns: resolved immediately\n");
        /* Cache the result */
        ip_addr_copy(dns_cache_ip, *out);
        int hlen = strlen(hostname);
        if (hlen >= (int)sizeof(dns_cache_host)) hlen = sizeof(dns_cache_host) - 1;
        memcpy(dns_cache_host, hostname, hlen);
        dns_cache_host[hlen] = '\0';
        dns_cache_valid = 1;
        return 0;
    }
    if (err != ERR_INPROGRESS) {
        DBG("dns: gethostbyname err=%d\n", (int)err);
        return -1;
    }

    u32_t start = sys_now();
    while (!dns_done && (sys_now() - start) < 10000) {
        net_poll();
        __asm__ volatile("hlt");
    }

    if (dns_done == 1) {
        ip_addr_copy(*out, dns_result);
        DBG("dns: resolved in %lums\n", (unsigned long)(sys_now() - start));
        /* Cache the result */
        ip_addr_copy(dns_cache_ip, dns_result);
        int hlen = strlen(hostname);
        if (hlen >= (int)sizeof(dns_cache_host)) hlen = sizeof(dns_cache_host) - 1;
        memcpy(dns_cache_host, hostname, hlen);
        dns_cache_host[hlen] = '\0';
        dns_cache_valid = 1;
        return 0;
    }
    DBG("dns: FAILED (timeout or error)\n");
    return -1;
}

/* ========================================================================= */
/* Persistent TLS connection — reused across requests                        */
/* ========================================================================= */

static tls_tcp_t          *g_tcp = NULL;
static mbedtls_ssl_context g_ssl;
static mbedtls_ssl_config  g_conf;
static int                 g_conn_active = 0;
static char                g_conn_host[64];

static void tls_disconnect(void) {
    DBG("tls_disconnect: tearing down\n");
    if (g_conn_active) {
        mbedtls_ssl_close_notify(&g_ssl);
        mbedtls_ssl_free(&g_ssl);
        mbedtls_ssl_config_free(&g_conf);
        g_conn_active = 0;
    }
    if (g_tcp) {
        if (g_tcp->pcb) {
            tcp_arg(g_tcp->pcb, NULL);
            tcp_recv(g_tcp->pcb, NULL);
            tcp_err(g_tcp->pcb, NULL);
            if (tcp_close(g_tcp->pcb) != ERR_OK)
                tcp_abort(g_tcp->pcb);
        }
        free(g_tcp);
        g_tcp = NULL;
    }
    g_conn_host[0] = '\0';
    /* Aggressively drain any remaining TCP state */
    for (int i = 0; i < 20; i++) {
        net_poll();
        __asm__ volatile("hlt");
    }
    DBG("tls_disconnect: done\n");
}

static int tls_conn_alive(void) {
    if (!g_conn_active || !g_tcp) return 0;
    if (g_tcp->error || g_tcp->remote_closed || !g_tcp->pcb) return 0;
    return 1;
}

static int tls_establish(const char *hostname) {
    u32_t t0 = sys_now();

    /* DNS */
    ip_addr_t server_ip;
    if (resolve_host(hostname, &server_ip) != 0) return -2;

    DBG("tcp: connecting to %s:443...\n", hostname);

    /* TCP connect — retry up to 3 times with increasing delay */
    int tcp_ok = 0;
    for (int attempt = 0; attempt < 3 && !tcp_ok; attempt++) {
        if (attempt > 0) {
            DBG("tcp: retry attempt %d...\n", attempt + 1);
            /* Wait before retry — QEMU SLIRP needs time between connections */
            u32_t wait_start = sys_now();
            u32_t delay = (attempt == 1) ? 1000 : 2000;
            while (sys_now() - wait_start < delay) {
                net_poll();
                __asm__ volatile("hlt");
            }
        }

        g_tcp = calloc(1, sizeof(tls_tcp_t));
        if (!g_tcp) return -3;

        struct tcp_pcb *pcb = tcp_new();
        if (!pcb) { free(g_tcp); g_tcp = NULL; return -10; }
        g_tcp->pcb = pcb;

        tcp_arg(pcb, g_tcp);
        tcp_recv(pcb, tls_recv_cb);
        tcp_err(pcb, tls_err_cb);

        err_t err = tcp_connect(pcb, &server_ip, 443, tls_connected_cb);
        if (err != ERR_OK) {
            DBG("tcp: tcp_connect err=%d\n", (int)err);
            tcp_abort(pcb);
            free(g_tcp); g_tcp = NULL;
            continue;
        }

        u32_t start = sys_now();
        while (!g_tcp->connected && !g_tcp->error && (sys_now() - start) < 15000) {
            net_poll();
            __asm__ volatile("hlt");
        }
        if (g_tcp->connected && !g_tcp->error) {
            tcp_ok = 1;
            DBG("tcp: connected in %lums\n", (unsigned long)(sys_now() - start));
        } else {
            DBG("tcp: connect FAILED (connected=%d error=%d elapsed=%lums)\n",
                g_tcp->connected, g_tcp->error, (unsigned long)(sys_now() - start));
            if (g_tcp->pcb) tcp_abort(g_tcp->pcb);
            free(g_tcp); g_tcp = NULL;
        }
    }
    if (!tcp_ok) return -12;

    /* TLS handshake */
    DBG("tls: starting handshake...\n");
    int ret;
    u32_t hs_start = sys_now();
    mbedtls_ssl_init(&g_ssl);
    mbedtls_ssl_config_init(&g_conf);

    ret = mbedtls_ssl_config_defaults(&g_conf,
            MBEDTLS_SSL_IS_CLIENT,
            MBEDTLS_SSL_TRANSPORT_STREAM,
            MBEDTLS_SSL_PRESET_DEFAULT);
    if (ret != 0) {
        DBG("tls: config_defaults err=%d\n", ret);
        goto fail;
    }

    mbedtls_ssl_conf_authmode(&g_conf, MBEDTLS_SSL_VERIFY_NONE);
    mbedtls_ssl_conf_rng(&g_conf, mbedtls_ctr_drbg_random, &g_ctr_drbg);

    ret = mbedtls_ssl_setup(&g_ssl, &g_conf);
    if (ret != 0) {
        DBG("tls: ssl_setup err=%d\n", ret);
        goto fail;
    }

    mbedtls_ssl_set_hostname(&g_ssl, hostname);
    mbedtls_ssl_set_bio(&g_ssl, g_tcp, tls_bio_send, tls_bio_recv, NULL);

    while ((ret = mbedtls_ssl_handshake(&g_ssl)) != 0) {
        if (ret != MBEDTLS_ERR_SSL_WANT_READ &&
            ret != MBEDTLS_ERR_SSL_WANT_WRITE) {
            DBG("tls: handshake FAILED err=%d\n", ret);
            goto fail;
        }
    }

    g_conn_active = 1;
    int hlen = strlen(hostname);
    if (hlen >= (int)sizeof(g_conn_host)) hlen = sizeof(g_conn_host) - 1;
    memcpy(g_conn_host, hostname, hlen);
    g_conn_host[hlen] = '\0';

    DBG("tls: handshake OK in %lums (total connect %lums)\n",
        (unsigned long)(sys_now() - hs_start),
        (unsigned long)(sys_now() - t0));
    return 0;

fail:
    mbedtls_ssl_free(&g_ssl);
    mbedtls_ssl_config_free(&g_conf);
    if (g_tcp && g_tcp->pcb) {
        tcp_arg(g_tcp->pcb, NULL);
        tcp_recv(g_tcp->pcb, NULL);
        tcp_err(g_tcp->pcb, NULL);
        tcp_abort(g_tcp->pcb);
    }
    if (g_tcp) { free(g_tcp); g_tcp = NULL; }
    return ret < 0 ? ret : -99;
}

/* ========================================================================= */
/* https_post — HTTPS POST with persistent connection                        */
/* ========================================================================= */

static int https_request_once(const char *method, const char *hostname,
                               const char *path, const char *headers,
                               const char *body, int body_len,
                               char *resp_buf, int resp_max) {
    int ret;
    u32_t t0 = sys_now();

    /* Ensure connected */
    net_poll();
    if (!tls_conn_alive() || strcmp(hostname, g_conn_host) != 0) {
        DBG("http: need new connection to %s\n", hostname);
        tls_disconnect();
        ret = tls_establish(hostname);
        if (ret != 0) {
            DBG("http: establish FAILED ret=%d\n", ret);
            return ret;
        }
    } else {
        DBG("http: reusing existing connection\n");
    }

    /* --- Build HTTP request (keep-alive) --- */
    char len_str[16] = "";
    if (body_len > 0) {
        int tmp = body_len, i = 0;
        char rev[16];
        if (tmp == 0) rev[i++] = '0';
        while (tmp > 0) { rev[i++] = '0' + (tmp % 10); tmp /= 10; }
        for (int j = 0; j < i; j++) len_str[j] = rev[i - 1 - j];
        len_str[i] = '\0';
    }

    char *req_hdr = malloc(2048);
    if (!req_hdr) return -3;

    int pos = 0;
    const char *s;
    memcpy(req_hdr + pos, method, strlen(method)); pos += strlen(method);
    s = " "; memcpy(req_hdr + pos, s, 1); pos += 1;
    memcpy(req_hdr + pos, path, strlen(path)); pos += strlen(path);
    s = " HTTP/1.1\r\nHost: "; memcpy(req_hdr + pos, s, strlen(s)); pos += strlen(s);
    memcpy(req_hdr + pos, hostname, strlen(hostname)); pos += strlen(hostname);
    if (body_len > 0) {
        s = "\r\nContent-Length: "; memcpy(req_hdr + pos, s, strlen(s)); pos += strlen(s);
        memcpy(req_hdr + pos, len_str, strlen(len_str)); pos += strlen(len_str);
    }
    s = "\r\nConnection: keep-alive\r\n"; memcpy(req_hdr + pos, s, strlen(s)); pos += strlen(s);
    if (headers && *headers) {
        memcpy(req_hdr + pos, headers, strlen(headers)); pos += strlen(headers);
    }
    s = "\r\n"; memcpy(req_hdr + pos, s, strlen(s)); pos += strlen(s);
    req_hdr[pos] = '\0';

    /* Send header */
    DBG("http: sending %s %d-byte header + %d-byte body\n", method, pos, body_len);
    ret = mbedtls_ssl_write(&g_ssl, (const unsigned char *)req_hdr, pos);
    free(req_hdr);
    if (ret < 0) {
        DBG("http: header write err=%d\n", ret);
        tls_disconnect();
        return ret;
    }

    /* Send body (POST only) */
    if (body_len > 0 && body) {
        int sent = 0;
        while (sent < body_len) {
            ret = mbedtls_ssl_write(&g_ssl, (const unsigned char *)body + sent,
                                    body_len - sent);
            if (ret < 0) {
                DBG("http: body write err=%d\n", ret);
                tls_disconnect();
                return ret;
            }
            sent += ret;
        }
        DBG("http: body sent OK\n");
    }

    /* --- Read HTTP response --- */
    int total = 0;
    char *hdr_end = NULL;
    u32_t read_start = sys_now();

    /* Phase 1: Read until end of headers */
    DBG("http: phase1 reading headers...\n");
    while (total < resp_max - 1) {
        ret = mbedtls_ssl_read(&g_ssl, (unsigned char *)resp_buf + total,
                               resp_max - 1 - total);
        if (ret == 0 || ret == MBEDTLS_ERR_SSL_PEER_CLOSE_NOTIFY) {
            DBG("http: phase1 connection closed (total=%d)\n", total);
            tls_disconnect();
            break;
        }
        if (ret < 0) {
            if (ret == MBEDTLS_ERR_SSL_WANT_READ) continue;
            DBG("http: phase1 read err=%d\n", ret);
            tls_disconnect();
            return ret;
        }
        total += ret;
        resp_buf[total] = '\0';
        DBG("http: phase1 read %d bytes (total=%d)\n", ret, total);

        hdr_end = strstr(resp_buf, "\r\n\r\n");
        if (hdr_end) break;
    }

    if (!hdr_end) {
        DBG("http: no header end found (total=%d)\n", total);
        return (total > 0) ? total : -14;
    }

    /* Parse Content-Length and Transfer-Encoding from headers */
    int content_length = -1;
    int chunked = 0;
    {
        char saved = *hdr_end;
        *hdr_end = '\0';

        /* Search for Content-Length at start of a header line (\r\n prefix) */
        char *cl = strstr(resp_buf, "\r\nContent-Length:");
        if (!cl) cl = strstr(resp_buf, "\r\ncontent-length:");
        if (!cl) cl = strstr(resp_buf, "\r\nContent-length:");
        if (cl) {
            cl = strchr(cl + 2, ':') + 1;
            while (*cl == ' ') cl++;
            content_length = 0;
            while (*cl >= '0' && *cl <= '9') {
                content_length = content_length * 10 + (*cl - '0');
                cl++;
            }
        }

        char *te = strstr(resp_buf, "\r\nTransfer-Encoding:");
        if (!te) te = strstr(resp_buf, "\r\ntransfer-encoding:");
        if (te && strstr(te, "chunked")) {
            chunked = 1;
        }

        *hdr_end = saved;
    }

    int body_offset = (hdr_end + 4) - resp_buf;
    int body_have = total - body_offset;

    DBG("http: phase1 done in %lums total=%d hdrOff=%d bodyHave=%d CL=%d chunked=%d\n",
        (unsigned long)(sys_now() - read_start),
        total, body_offset, body_have, content_length, chunked);

    /* Phase 2: Read remaining body */
    if (content_length >= 0) {
        DBG("http: phase2 CL mode, need %d have %d\n", content_length, body_have);
        /* Read exactly content_length bytes */
        while (body_have < content_length && total < resp_max - 1) {
            ret = mbedtls_ssl_read(&g_ssl, (unsigned char *)resp_buf + total,
                                   resp_max - 1 - total);
            if (ret == 0 || ret == MBEDTLS_ERR_SSL_PEER_CLOSE_NOTIFY) {
                DBG("http: phase2 CL close, have=%d/%d\n", body_have, content_length);
                tls_disconnect();
                break;
            }
            if (ret < 0) {
                if (ret == MBEDTLS_ERR_SSL_WANT_READ) continue;
                DBG("http: phase2 CL read err=%d have=%d/%d\n", ret, body_have, content_length);
                tls_disconnect();
                break;
            }
            total += ret;
            body_have += ret;
            DBG("http: phase2 CL read %d (have=%d/%d)\n", ret, body_have, content_length);
        }
    } else if (chunked) {
        DBG("http: phase2 chunked mode, have=%d so far\n", body_have);
        /* Read until final chunk marker "\r\n0\r\n" appears in body */
        int chunk_reads = 0;
        while (total < resp_max - 1) {
            resp_buf[total] = '\0';
            char *body_area = resp_buf + body_offset;
            if (strstr(body_area, "\r\n0\r\n") ||
                (body_have >= 3 && body_area[0] == '0' &&
                 body_area[1] == '\r' && body_area[2] == '\n')) {
                DBG("http: phase2 chunk done, reads=%d have=%d in %lums\n",
                    chunk_reads, body_have,
                    (unsigned long)(sys_now() - read_start));
                break;
            }
            ret = mbedtls_ssl_read(&g_ssl, (unsigned char *)resp_buf + total,
                                   resp_max - 1 - total);
            if (ret == 0 || ret == MBEDTLS_ERR_SSL_PEER_CLOSE_NOTIFY) {
                DBG("http: phase2 chunk close, reads=%d have=%d\n", chunk_reads, body_have);
                tls_disconnect();
                break;
            }
            if (ret < 0) {
                if (ret == MBEDTLS_ERR_SSL_WANT_READ) continue;
                DBG("http: phase2 chunk err=%d reads=%d have=%d\n", ret, chunk_reads, body_have);
                tls_disconnect();
                break;
            }
            total += ret;
            body_have += ret;
            chunk_reads++;
            DBG("http: phase2 chunk read %d (total_body=%d reads=%d)\n", ret, body_have, chunk_reads);
        }
    } else {
        /* No content-length, no chunked — read until close (fallback) */
        DBG("http: phase2 fallback (no CL, no chunked)\n");
        while (total < resp_max - 1) {
            ret = mbedtls_ssl_read(&g_ssl, (unsigned char *)resp_buf + total,
                                   resp_max - 1 - total);
            if (ret == 0 || ret == MBEDTLS_ERR_SSL_PEER_CLOSE_NOTIFY) {
                DBG("http: phase2 fallback close, have=%d\n", total - body_offset);
                tls_disconnect();
                break;
            }
            if (ret < 0) {
                if (ret == MBEDTLS_ERR_SSL_WANT_READ) continue;
                DBG("http: phase2 fallback err=%d\n", ret);
                break;
            }
            total += ret;
        }
    }
    resp_buf[total] = '\0';

    /* --- Extract body --- */
    int body_sz = total - body_offset;
    memmove(resp_buf, resp_buf + body_offset, body_sz);
    resp_buf[body_sz] = '\0';

    DBG("http: raw body %d bytes, total time %lums\n",
        body_sz, (unsigned long)(sys_now() - t0));

    /* Decode chunked transfer encoding if needed */
    if (chunked && body_sz > 0) {
        char *src = resp_buf;
        char *dst = resp_buf;
        char *end = resp_buf + body_sz;

        while (src < end) {
            int chunk_size = 0;
            while (src < end && *src != '\r' && *src != '\n') {
                char c = *src++;
                if (c >= '0' && c <= '9')      chunk_size = chunk_size * 16 + (c - '0');
                else if (c >= 'a' && c <= 'f') chunk_size = chunk_size * 16 + (c - 'a' + 10);
                else if (c >= 'A' && c <= 'F') chunk_size = chunk_size * 16 + (c - 'A' + 10);
                else break;
            }
            if (src < end && *src == '\r') src++;
            if (src < end && *src == '\n') src++;

            if (chunk_size == 0) break;

            int to_copy = chunk_size;
            if (src + to_copy > end) to_copy = end - src;
            if (dst != src) memmove(dst, src, to_copy);
            dst += to_copy;
            src += to_copy;

            if (src < end && *src == '\r') src++;
            if (src < end && *src == '\n') src++;
        }
        *dst = '\0';
        int decoded = dst - resp_buf;
        DBG("http: chunked decoded %d bytes\n", decoded);
        return decoded;
    }

    return body_sz;
}

int https_post_bin(const char *hostname, const char *path,
                   const char *headers, const char *body, int body_len,
                   char *resp_buf, int resp_max) {

    /* Selftest hook — intercept before touching the network */
    {
        extern int selftest_active;
        extern int selftest_get_response(const char *host, char *buf, int max);
        if (selftest_active) {
            return selftest_get_response(hostname, resp_buf, resp_max);
        }
    }

    if (!g_tls_ready) {
        DBG("https_post: TLS not ready!\n");
        return -1;
    }

    int had_conn = g_conn_active;
    DBG("https_post: %s%s (existing=%d, body=%d)\n", hostname, path, had_conn, body_len);

    int ret = https_request_once("POST", hostname, path, headers, body, body_len, resp_buf, resp_max);

    if (ret < 0 && had_conn) {
        DBG("https_post: retry (stale conn), ret was %d\n", ret);
        tls_disconnect();
        ret = https_request_once("POST", hostname, path, headers, body, body_len, resp_buf, resp_max);
    }

    DBG("https_post: final ret=%d\n", ret);
    return ret;
}

int https_post(const char *hostname, const char *path,
               const char *headers, const char *body,
               char *resp_buf, int resp_max) {
    return https_post_bin(hostname, path, headers, body, strlen(body),
                          resp_buf, resp_max);
}

int https_get(const char *hostname, const char *path,
              const char *extra_headers, char *resp_buf, int resp_max) {
    /* Selftest hook */
    {
        extern int selftest_active;
        extern int selftest_get_response(const char *host, char *buf, int max);
        if (selftest_active) {
            return selftest_get_response(hostname, resp_buf, resp_max);
        }
    }

    if (!g_tls_ready) {
        DBG("https_get: TLS not ready!\n");
        return -1;
    }

    int had_conn = g_conn_active;
    DBG("https_get: %s%s (existing=%d)\n", hostname, path, had_conn);

    int ret = https_request_once("GET", hostname, path, extra_headers,
                                  NULL, 0, resp_buf, resp_max);

    if (ret < 0 && had_conn) {
        DBG("https_get: retry (stale conn), ret was %d\n", ret);
        tls_disconnect();
        ret = https_request_once("GET", hostname, path, extra_headers,
                                  NULL, 0, resp_buf, resp_max);
    }

    DBG("https_get: final ret=%d\n", ret);
    return ret;
}
