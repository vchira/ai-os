/* lwIP configuration for AiOS — freestanding, no threads, polling mode */

#ifndef AIOS_LWIPOPTS_H
#define AIOS_LWIPOPTS_H

/* No OS, no threads */
#define NO_SYS                  1
#define LWIP_SOCKET             0
#define LWIP_NETCONN            0
#define SYS_LIGHTWEIGHT_PROT    0

/* Memory configuration */
#define MEM_ALIGNMENT           4
#define MEM_SIZE                (64 * 1024)  /* 64 KB heap for lwIP */
#define MEMP_NUM_PBUF           32
#define MEMP_NUM_UDP_PCB        4
#define MEMP_NUM_TCP_PCB        8
#define MEMP_NUM_TCP_PCB_LISTEN 4
#define MEMP_NUM_TCP_SEG        32
#define MEMP_NUM_NETBUF         8
#define MEMP_NUM_NETCONN        8

/* Pbuf options */
#define PBUF_POOL_SIZE          32
#define PBUF_POOL_BUFSIZE       1536

/* TCP options */
#define LWIP_TCP                1
#define TCP_MSS                 1460
#define TCP_SND_BUF             (8 * TCP_MSS)
#define TCP_SND_QUEUELEN        (4 * TCP_SND_BUF / TCP_MSS)
#define TCP_WND                 (4 * TCP_MSS)

/* ARP */
#define LWIP_ARP                1
#define ARP_TABLE_SIZE          10
#define ARP_QUEUEING            1

/* IP */
#define LWIP_IPV4               1
#define LWIP_IPV6               0
#define IP_FORWARD              0
#define IP_REASSEMBLY           1
#define IP_FRAG                 1

/* ICMP */
#define LWIP_ICMP               1

/* UDP */
#define LWIP_UDP                1

/* DHCP */
#define LWIP_DHCP               1

/* DNS */
#define LWIP_DNS                1
#define DNS_TABLE_SIZE          4
#define DNS_MAX_NAME_LENGTH     256

/* Raw API callbacks */
#define LWIP_RAW                1
#define LWIP_CALLBACK_API       1

/* Checksum */
#define CHECKSUM_GEN_IP         1
#define CHECKSUM_GEN_UDP        1
#define CHECKSUM_GEN_TCP        1
#define CHECKSUM_CHECK_IP       1
#define CHECKSUM_CHECK_UDP      1
#define CHECKSUM_CHECK_TCP      1

/* Debug (enable selectively) */
#define LWIP_DEBUG              0

/* Stats */
#define LWIP_STATS              0
#define LWIP_STATS_DISPLAY      0

/* Use our own malloc/free from heap.c */
#define MEM_LIBC_MALLOC         1
#define MEMP_MEM_MALLOC         1

#endif
