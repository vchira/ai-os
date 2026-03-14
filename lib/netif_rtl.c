/* lwIP network interface driver glue for RTL8139 */

#include "lib/lwip/src/include/lwip/netif.h"
#include "lib/lwip/src/include/lwip/pbuf.h"
#include "lib/lwip/src/include/lwip/etharp.h"
#include "lib/lwip/src/include/lwip/dhcp.h"
#include "lib/lwip/src/include/lwip/init.h"
#include "lib/lwip/src/include/lwip/timeouts.h"
#include "lib/lwip/src/include/lwip/ip4_addr.h"
#include "lib/lwip/src/include/lwip/dns.h"
#include "lib/lwip/src/include/netif/ethernet.h"
#include "include/rtl8139.h"
#include "include/string.h"
#include "include/io.h"

static struct netif aios_netif;
static uint8_t rx_frame[1536];

static err_t rtl_linkoutput(struct netif *netif, struct pbuf *p) {
    (void)netif;
    uint8_t frame[1536];
    uint16_t len = 0;

    for (struct pbuf *q = p; q != NULL; q = q->next) {
        if (len + q->len > sizeof(frame)) return ERR_BUF;
        memcpy(frame + len, q->payload, q->len);
        len += q->len;
    }

    /* Pad to minimum ethernet frame size */
    if (len < 60) {
        memset(frame + len, 0, 60 - len);
        len = 60;
    }

    return rtl8139_send(frame, len) == 0 ? ERR_OK : ERR_IF;
}

static err_t rtl_init_cb(struct netif *netif) {
    uint8_t mac[6];
    rtl8139_get_mac(mac);

    netif->name[0] = 'e';
    netif->name[1] = 'n';
    netif->hwaddr_len = 6;
    memcpy(netif->hwaddr, mac, 6);
    netif->mtu = 1500;
    netif->flags = NETIF_FLAG_BROADCAST | NETIF_FLAG_ETHARP | NETIF_FLAG_LINK_UP;
    netif->linkoutput = rtl_linkoutput;
    netif->output = etharp_output;

    return ERR_OK;
}

/* Called periodically from main loop to receive packets */
void net_poll(void) {
    /* Drain all available RX packets (not just one) for better responsiveness */
    for (int pkts = 0; pkts < 16; pkts++) {
        int len = rtl8139_poll(rx_frame, sizeof(rx_frame));
        if (len <= 0) break;
        struct pbuf *p = pbuf_alloc(PBUF_RAW, len, PBUF_POOL);
        if (p) {
            pbuf_take(p, rx_frame, len);
            if (aios_netif.input(p, &aios_netif) != ERR_OK)
                pbuf_free(p);
        }
    }
    sys_check_timeouts();
}

static int net_initialized = 0;

int net_init(void) {
    /* Initialize RTL8139 hardware */
    if (rtl8139_init() != 0) {
        vga_print("[FAIL] RTL8139 NIC not found\n");
        return -1;
    }

    uint8_t mac[6];
    rtl8139_get_mac(mac);
    /* Print MAC as XX:XX:XX:XX:XX:XX */
    {
        static const char hex[] = "0123456789ABCDEF";
        char mac_str[18]; /* XX:XX:XX:XX:XX:XX\0 */
        for (int i = 0; i < 6; i++) {
            mac_str[i * 3]     = hex[(mac[i] >> 4) & 0xF];
            mac_str[i * 3 + 1] = hex[mac[i] & 0xF];
            mac_str[i * 3 + 2] = (i < 5) ? ':' : '\0';
        }
        vga_print("[OK] NIC found, MAC: ");
        vga_print(mac_str);
        vga_newline();
    }

    /* Initialize lwIP */
    lwip_init();

    /* Set up netif */
    ip4_addr_t ip, mask, gw;
    ip4_addr_set_zero(&ip);
    ip4_addr_set_zero(&mask);
    ip4_addr_set_zero(&gw);

    netif_add(&aios_netif, &ip, &mask, &gw, NULL, rtl_init_cb, ethernet_input);
    netif_set_default(&aios_netif);
    netif_set_up(&aios_netif);

    /* Set fallback DNS server (SLIRP default: 10.0.2.3, also try Google 8.8.8.8) */
    {
        ip_addr_t dns0, dns1;
        IP4_ADDR(&dns0, 10, 0, 2, 3);   /* QEMU/libvirt SLIRP DNS */
        IP4_ADDR(&dns1, 8, 8, 8, 8);    /* Google DNS fallback */
        dns_setserver(0, &dns0);
        dns_setserver(1, &dns1);
    }

    /* Start DHCP (may override DNS servers above) */
    dhcp_start(&aios_netif);
    vga_print("[OK] DHCP request sent\n");

    net_initialized = 1;
    return 0;
}

int net_is_up(void) {
    if (!net_initialized) return 0;
    return !ip4_addr_isany_val(*netif_ip4_addr(&aios_netif));
}

void net_get_ip(char *buf, int max_len) {
    if (!net_initialized || !net_is_up()) {
        strncpy(buf, "0.0.0.0", max_len);
        return;
    }
    const ip4_addr_t *ip = netif_ip4_addr(&aios_netif);
    /* Manual IP to string since we don't have snprintf */
    uint32_t addr = ip4_addr_get_u32(ip);
    char *p = buf;
    for (int i = 0; i < 4; i++) {
        uint8_t octet = (addr >> (i * 8)) & 0xFF;
        if (octet >= 100) { *p++ = '0' + octet / 100; }
        if (octet >= 10) { *p++ = '0' + (octet / 10) % 10; }
        *p++ = '0' + octet % 10;
        if (i < 3) *p++ = '.';
    }
    *p = '\0';
}

struct netif *net_get_netif(void) {
    return &aios_netif;
}
