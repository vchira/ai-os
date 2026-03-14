/* mbedTLS config for AiOS — minimal TLS 1.2 client for HTTPS to Claude API */

#ifndef AIOS_MBEDTLS_CONFIG_H
#define AIOS_MBEDTLS_CONFIG_H

/* System */
#define MBEDTLS_HAVE_ASM
#define MBEDTLS_NO_PLATFORM_ENTROPY
#define MBEDTLS_ENTROPY_HARDWARE_ALT
#define MBEDTLS_NO_UDBL_DIVISION

/* Platform abstraction */
#define MBEDTLS_PLATFORM_C
#define MBEDTLS_PLATFORM_MEMORY

/* We provide our own calloc/free and standard headers */
#include "include/types.h"
#include "include/heap.h"
#include "include/string.h"
#define MBEDTLS_PLATFORM_STD_CALLOC   calloc
#define MBEDTLS_PLATFORM_STD_FREE     free

/* TLS */
#define MBEDTLS_SSL_CLI_C
#define MBEDTLS_SSL_TLS_C
#define MBEDTLS_SSL_PROTO_TLS1_2
#define MBEDTLS_SSL_SERVER_NAME_INDICATION

/* Ciphersuites — just the ones Claude API needs */
#define MBEDTLS_AES_C
#define MBEDTLS_GCM_C
#define MBEDTLS_CIPHER_C
#define MBEDTLS_MD_C
#define MBEDTLS_SHA256_C
#define MBEDTLS_SHA384_C
#define MBEDTLS_SHA512_C

/* Key exchange */
#define MBEDTLS_KEY_EXCHANGE_ECDHE_RSA_ENABLED
#define MBEDTLS_RSA_C
#define MBEDTLS_BIGNUM_C
#define MBEDTLS_OID_C
#define MBEDTLS_PKCS1_V15
#define MBEDTLS_PK_C
#define MBEDTLS_PK_PARSE_C

/* ECC */
#define MBEDTLS_ECP_C
#define MBEDTLS_ECDH_C
#define MBEDTLS_ECP_DP_SECP256R1_ENABLED
#define MBEDTLS_ECP_DP_SECP384R1_ENABLED

/* X.509 */
#define MBEDTLS_X509_CRT_PARSE_C
#define MBEDTLS_X509_USE_C
#define MBEDTLS_ASN1_PARSE_C
#define MBEDTLS_ASN1_WRITE_C
#define MBEDTLS_BASE64_C
#define MBEDTLS_PEM_PARSE_C

/* CTR-DRBG for random */
#define MBEDTLS_CTR_DRBG_C
#define MBEDTLS_ENTROPY_C

/* Network (we provide our own net callbacks) */
/* Do NOT define MBEDTLS_NET_C — we use custom callbacks */

/* Standard TLS max record size — must be 16384 since we don't negotiate
   max_fragment_length and servers may send full-size records. */
#define MBEDTLS_SSL_MAX_CONTENT_LEN  16384

#endif
