#include "include/rtl8139.h"
#include "include/pci.h"
#include "include/io.h"
#include "include/string.h"

static uint16_t io_base;
static uint8_t  mac_addr[6];
static uint8_t  rx_buffer[RTL_RX_BUF_SIZE] __attribute__((aligned(4)));
static uint8_t  tx_buffers[RTL_NUM_TX_DESC][RTL_TX_BUF_SIZE] __attribute__((aligned(4)));
static int      tx_cur = 0;
static uint16_t rx_offset = 0;

int rtl8139_init(void) {
    pci_device_t dev;
    if (!pci_find_device(RTL8139_VENDOR_ID, RTL8139_DEVICE_ID, &dev))
        return -1;

    /* Get IO base address from BAR0 */
    io_base = dev.bar[0] & ~3u;

    /* Enable bus mastering */
    pci_enable_bus_mastering(&dev);

    /* Power on */
    outb(io_base + RTL_CONFIG1, 0x00);

    /* Software reset */
    outb(io_base + RTL_CMD, RTL_CMD_RESET);
    while (inb(io_base + RTL_CMD) & RTL_CMD_RESET)
        ;

    /* Set RX buffer address */
    outl(io_base + RTL_RXBUF, (uint32_t)rx_buffer);

    /* Disable all interrupts — we use polling only */
    outw(io_base + RTL_IMR, 0x0000);

    /* Clear any pending interrupts */
    outw(io_base + RTL_ISR, 0xFFFF);

    /* RX config: accept all, wrap, 8K buffer */
    outl(io_base + RTL_RXCONFIG,
         RTL_RX_ACCEPT_ALL | RTL_RX_WRAP | RTL_RX_BUF_8K);

    /* Enable RX and TX */
    outb(io_base + RTL_CMD, RTL_CMD_RX_ENABLE | RTL_CMD_TX_ENABLE);

    /* Read MAC address */
    for (int i = 0; i < 6; i++)
        mac_addr[i] = inb(io_base + RTL_MAC0 + i);

    /* Clear any interrupts from init */
    outw(io_base + RTL_ISR, 0xFFFF);

    rx_offset = 0;
    tx_cur = 0;

    return 0;
}

void rtl8139_get_mac(uint8_t mac[6]) {
    memcpy(mac, mac_addr, 6);
}

int rtl8139_send(const void *data, uint16_t len) {
    if (len > RTL_TX_BUF_SIZE) return -1;

    memcpy(tx_buffers[tx_cur], data, len);

    /* Set TX address */
    outl(io_base + RTL_TXADDR0 + tx_cur * 4, (uint32_t)tx_buffers[tx_cur]);

    /* Set TX status: size (clears OWN bit, starts TX) */
    outl(io_base + RTL_TXSTATUS0 + tx_cur * 4, len);

    /* Brief wait for TX — don't block if slow */
    for (int i = 0; i < 1000; i++) {
        uint32_t status = inl(io_base + RTL_TXSTATUS0 + tx_cur * 4);
        if (status & (RTL_TX_STATUS_OK | RTL_TX_OWN)) break;
    }

    tx_cur = (tx_cur + 1) % RTL_NUM_TX_DESC;
    return 0;
}

int rtl8139_poll(void *buf, uint16_t max_len) {
    /* Check if there's data */
    uint8_t cmd = inb(io_base + RTL_CMD);
    if (cmd & 0x01) return 0; /* Buffer empty */

    /* RTL8139 packet header: status(2) + length(2) + data */
    uint16_t status = *(uint16_t *)(rx_buffer + rx_offset);
    uint16_t length = *(uint16_t *)(rx_buffer + rx_offset + 2);

    if (!(status & 0x01)) return 0; /* Not a valid packet */

    /* Packet data starts after the 4-byte header */
    uint16_t data_len = length - 4; /* Subtract CRC */
    if (data_len > max_len) data_len = max_len;

    memcpy(buf, rx_buffer + rx_offset + 4, data_len);

    /* Update read pointer (aligned to 4 bytes) */
    rx_offset = (rx_offset + length + 4 + 3) & ~3u;
    rx_offset %= RTL_RX_BUF_SIZE;

    /* Update CAPR (read pointer) */
    outw(io_base + RTL_CAPR, rx_offset - 16);

    return data_len;
}
