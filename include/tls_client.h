/* AiOS — TLS HTTPS client over lwIP + mbedTLS */
#ifndef AIOS_TLS_CLIENT_H
#define AIOS_TLS_CLIENT_H

#include "types.h"

/* One-time TLS subsystem init (call after heap + net are up) */
void tls_init(void);

/* HTTPS POST — resolves hostname via DNS, connects TLS, sends request.
   Same semantics as http_post but over TLS on port 443. */
int https_post(const char *hostname, const char *path,
               const char *headers, const char *body,
               char *resp_buf, int resp_max);

/* Binary-safe HTTPS POST — body_len specifies exact body size (for binary data). */
int https_post_bin(const char *hostname, const char *path,
                   const char *headers, const char *body, int body_len,
                   char *resp_buf, int resp_max);

/* HTTPS GET — downloads response body into resp_buf.
   Returns number of body bytes on success, negative on error. */
int https_get(const char *hostname, const char *path,
              const char *extra_headers, char *resp_buf, int resp_max);

#endif
