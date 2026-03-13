/* Minimal HTTP/1.1 client for AiOS — raw TCP via lwIP */

#include "lib/lwip/src/include/lwip/tcp.h"
#include "lib/lwip/src/include/lwip/dns.h"
#include "lib/lwip/src/include/lwip/pbuf.h"
#include "include/string.h"
#include "include/io.h"
#include "include/heap.h"

#define HTTP_BUF_SIZE   8192
#define HTTP_CONNECT_TIMEOUT_MS  15000  /* 15 seconds */
#define HTTP_RECV_TIMEOUT_MS     30000  /* 30 seconds */

/* Connection state */
typedef struct {
    struct tcp_pcb *pcb;
    char   *recv_buf;
    int     recv_len;
    int     recv_cap;
    int     connected;
    int     done;
    int     error;
} http_conn_t;

static err_t http_recv_cb(void *arg, struct tcp_pcb *pcb, struct pbuf *p, err_t err) {
    http_conn_t *conn = (http_conn_t *)arg;
    (void)pcb;

    if (!p || err != ERR_OK) {
        conn->done = 1;
        return ERR_OK;
    }

    /* Copy data to our buffer */
    struct pbuf *q;
    for (q = p; q != NULL; q = q->next) {
        int space = conn->recv_cap - conn->recv_len - 1;
        int to_copy = (int)q->len < space ? (int)q->len : space;
        if (to_copy > 0) {
            memcpy(conn->recv_buf + conn->recv_len, q->payload, to_copy);
            conn->recv_len += to_copy;
        }
    }
    conn->recv_buf[conn->recv_len] = '\0';

    tcp_recved(pcb, p->tot_len);
    pbuf_free(p);
    return ERR_OK;
}

static err_t http_connected_cb(void *arg, struct tcp_pcb *pcb, err_t err) {
    http_conn_t *conn = (http_conn_t *)arg;
    (void)pcb;
    if (err == ERR_OK) conn->connected = 1;
    else conn->error = 1;
    return ERR_OK;
}

static void http_err_cb(void *arg, err_t err) {
    http_conn_t *conn = (http_conn_t *)arg;
    (void)err;
    conn->error = 1;
    conn->pcb = NULL;
}

extern void net_poll(void);
extern u32_t sys_now(void);

/* Low-level HTTP POST — returns response body in resp_buf */
int http_post(ip_addr_t *server_ip, uint16_t port,
              const char *host, const char *path,
              const char *headers, const char *body,
              char *resp_buf, int resp_max) {
    http_conn_t conn;
    memset(&conn, 0, sizeof(conn));
    conn.recv_buf = resp_buf;
    conn.recv_cap = resp_max;

    struct tcp_pcb *pcb = tcp_new();
    if (!pcb) return -10;  /* TCP alloc failed */
    conn.pcb = pcb;

    tcp_arg(pcb, &conn);
    tcp_recv(pcb, http_recv_cb);
    tcp_err(pcb, http_err_cb);

    /* Connect */
    err_t err = tcp_connect(pcb, server_ip, port, http_connected_cb);
    if (err != ERR_OK) {
        tcp_abort(pcb);
        return -11;  /* TCP connect call failed */
    }

    /* Wait for connection — real-time timeout */
    u32_t start = sys_now();
    while (!conn.connected && !conn.error && (sys_now() - start) < HTTP_CONNECT_TIMEOUT_MS) {
        net_poll();
        __asm__ volatile("hlt");
    }
    if (conn.error) {
        if (conn.pcb) tcp_abort(conn.pcb);
        return -12;  /* Connection error */
    }
    if (!conn.connected) {
        if (conn.pcb) tcp_abort(conn.pcb);
        return -13;  /* Connection timeout */
    }

    /* Build and send HTTP request */
    int body_len = strlen(body);
    char len_str[16];
    /* itoa for content-length */
    {
        int tmp = body_len, i = 0;
        char rev[16];
        if (tmp == 0) rev[i++] = '0';
        while (tmp > 0) { rev[i++] = '0' + (tmp % 10); tmp /= 10; }
        for (int j = 0; j < i; j++) len_str[j] = rev[i - 1 - j];
        len_str[i] = '\0';
    }

    /* Send request line + headers */
    char req_hdr[1024];
    int pos = 0;
    const char *s;

    s = "POST "; memcpy(req_hdr + pos, s, strlen(s)); pos += strlen(s);
    memcpy(req_hdr + pos, path, strlen(path)); pos += strlen(path);
    s = " HTTP/1.1\r\nHost: "; memcpy(req_hdr + pos, s, strlen(s)); pos += strlen(s);
    memcpy(req_hdr + pos, host, strlen(host)); pos += strlen(host);
    s = "\r\nContent-Length: "; memcpy(req_hdr + pos, s, strlen(s)); pos += strlen(s);
    memcpy(req_hdr + pos, len_str, strlen(len_str)); pos += strlen(len_str);
    s = "\r\n"; memcpy(req_hdr + pos, s, strlen(s)); pos += strlen(s);
    memcpy(req_hdr + pos, headers, strlen(headers)); pos += strlen(headers);
    s = "\r\n"; memcpy(req_hdr + pos, s, strlen(s)); pos += strlen(s);
    req_hdr[pos] = '\0';

    tcp_write(pcb, req_hdr, pos, TCP_WRITE_FLAG_COPY);
    tcp_write(pcb, body, body_len, TCP_WRITE_FLAG_COPY);
    tcp_output(pcb);

    /* Wait for response — real-time timeout */
    start = sys_now();
    while (!conn.done && !conn.error && (sys_now() - start) < HTTP_RECV_TIMEOUT_MS) {
        net_poll();
        __asm__ volatile("hlt");
    }

    if (conn.pcb) {
        tcp_close(conn.pcb);
    }

    if (conn.error) return -14;  /* Response error */
    if (conn.recv_len == 0) return -15;  /* Empty response */

    /* Find body after \r\n\r\n */
    char *body_start = strstr(resp_buf, "\r\n\r\n");
    if (body_start) {
        body_start += 4;
        int body_offset = body_start - resp_buf;
        int body_sz = conn.recv_len - body_offset;
        memmove(resp_buf, body_start, body_sz);
        resp_buf[body_sz] = '\0';
        return body_sz;
    }

    return conn.recv_len;
}
