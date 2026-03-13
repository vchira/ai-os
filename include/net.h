#ifndef AIOS_NET_H
#define AIOS_NET_H

#include "types.h"

/* Network initialization (RTL8139 + lwIP + DHCP) */
int  net_init(void);
void net_poll(void);
int  net_is_up(void);
void net_get_ip(char *buf, int max_len);

/* PCI */
void pci_scan(void);

/* Claude API */
int claude_ask(const char *question, char *response, int max_len);

struct netif;
struct netif *net_get_netif(void);

#endif
