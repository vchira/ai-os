#ifndef AIOS_RTL8139_H
#define AIOS_RTL8139_H

#include "types.h"

#define RTL8139_VENDOR_ID  0x10EC
#define RTL8139_DEVICE_ID  0x8139

/* RTL8139 registers (offset from IO base) */
#define RTL_MAC0           0x00
#define RTL_MAR0           0x08
#define RTL_TXSTATUS0      0x10
#define RTL_TXADDR0        0x20
#define RTL_RXBUF          0x30
#define RTL_CMD            0x37
#define RTL_CAPR           0x38
#define RTL_CBR            0x3A
#define RTL_IMR            0x3C
#define RTL_ISR            0x3E
#define RTL_TXCONFIG       0x40
#define RTL_RXCONFIG       0x44
#define RTL_CONFIG1        0x52

/* Command register bits */
#define RTL_CMD_RESET      0x10
#define RTL_CMD_RX_ENABLE  0x08
#define RTL_CMD_TX_ENABLE  0x04

/* RX config */
#define RTL_RX_ACCEPT_ALL    0x0F  /* AB+AM+APM+AAP */
#define RTL_RX_WRAP          0x80
#define RTL_RX_BUF_8K       (0 << 11)
#define RTL_RX_BUF_32K      (1 << 11)

/* ISR bits */
#define RTL_ISR_ROK         0x0001
#define RTL_ISR_TOK         0x0004

/* TX status bits */
#define RTL_TX_OWN          0x2000   /* bit 13: DMA completed */
#define RTL_TX_TUN          0x4000   /* bit 14: TX FIFO underrun */
#define RTL_TX_STATUS_OK    0x8000   /* bit 15: TX completed OK */
#define RTL_TX_ABORT        0x40000000  /* bit 30: TX aborted */
#define RTL_TX_CARRIER_LOST 0x20000000  /* bit 29: carrier sense lost */

#define RTL_RX_BUF_SIZE     (8192 + 16 + 1500) /* 8K + header + max frame */
#define RTL_TX_BUF_SIZE     1536
#define RTL_NUM_TX_DESC     4

int  rtl8139_init(void);
int  rtl8139_send(const void *data, uint16_t len);
int  rtl8139_poll(void *buf, uint16_t max_len);
void rtl8139_get_mac(uint8_t mac[6]);

#endif
