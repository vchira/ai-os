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

/* LLM Provider system */
void llm_init(void);
int  llm_ask(const char *question, char *response, int max_len);
int  llm_get_num_providers(void);
int  llm_get_active(void);
int  llm_set_active(int id);
const char *llm_get_provider_name(int id);
const char *llm_get_provider_model(int id);
int  llm_is_configured(int id);

/* Backward compat */
int claude_ask(const char *question, char *response, int max_len);

struct netif;
struct netif *net_get_netif(void);

#endif
