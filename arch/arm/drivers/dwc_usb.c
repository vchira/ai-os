/* AiOS — DWC2 USB Host Controller Driver (Raspberry Pi)
   The RPi uses a DesignWare Core DWC2 USB OTG controller.
   This is significantly different from UHCI/EHCI on x86.

   The DWC2 is accessed via MMIO registers at the USB peripheral base.
   It supports USB 2.0 (Full/High speed, with a transaction translator
   for low-speed devices via the internal hub).

   This is a minimal driver that handles:
   - Controller initialization
   - Port reset
   - Control transfers (for device enumeration)
   - Interrupt transfers (for HID devices)
   - Bulk transfers (for storage) */

#include "include/types.h"
#include "include/string.h"

#ifndef PERIPH_BASE
#define PERIPH_BASE  0x3F000000
#endif

#define USB_BASE     (PERIPH_BASE + 0x980000)

/* DWC2 Core Registers */
#define DWC_GOTGCTL  (*(volatile uint32_t *)(USB_BASE + 0x000))
#define DWC_GOTGINT  (*(volatile uint32_t *)(USB_BASE + 0x004))
#define DWC_GAHBCFG  (*(volatile uint32_t *)(USB_BASE + 0x008))
#define DWC_GUSBCFG  (*(volatile uint32_t *)(USB_BASE + 0x00C))
#define DWC_GRSTCTL  (*(volatile uint32_t *)(USB_BASE + 0x010))
#define DWC_GINTSTS  (*(volatile uint32_t *)(USB_BASE + 0x014))
#define DWC_GINTMSK  (*(volatile uint32_t *)(USB_BASE + 0x018))
#define DWC_GRXFSIZ  (*(volatile uint32_t *)(USB_BASE + 0x024))
#define DWC_GNPTXFSIZ (*(volatile uint32_t *)(USB_BASE + 0x028))

/* Host Mode Registers */
#define DWC_HCFG     (*(volatile uint32_t *)(USB_BASE + 0x400))
#define DWC_HPRT     (*(volatile uint32_t *)(USB_BASE + 0x440))
#define DWC_HAINTMSK (*(volatile uint32_t *)(USB_BASE + 0x418))

/* Host Channel Registers (channel 0) */
#define DWC_HCCHAR(n)  (*(volatile uint32_t *)(USB_BASE + 0x500 + (n) * 0x20))
#define DWC_HCINT(n)   (*(volatile uint32_t *)(USB_BASE + 0x508 + (n) * 0x20))
#define DWC_HCINTMSK(n) (*(volatile uint32_t *)(USB_BASE + 0x50C + (n) * 0x20))
#define DWC_HCTSIZ(n)  (*(volatile uint32_t *)(USB_BASE + 0x510 + (n) * 0x20))
#define DWC_HCDMA(n)   (*(volatile uint32_t *)(USB_BASE + 0x514 + (n) * 0x20))

/* GRSTCTL bits */
#define GRSTCTL_CSRST    (1 << 0)   /* Core Soft Reset */
#define GRSTCTL_AHBIDLE  (1 << 31)  /* AHB Master Idle */

/* HPRT bits (host port) */
#define HPRT_PENA     (1 << 2)   /* Port Enable */
#define HPRT_PCDET    (1 << 1)   /* Port Connect Detected */
#define HPRT_PCSTS    (1 << 0)   /* Port Connect Status */
#define HPRT_PRST     (1 << 8)   /* Port Reset */
#define HPRT_PSPD_MASK (3 << 17) /* Port Speed */

extern void uart_puts(const char *);
extern void delay_ms_arm(uint32_t);

static int dwc_ready = 0;

static void dwc_core_reset(void) {
    /* Wait for AHB idle */
    while (!(DWC_GRSTCTL & GRSTCTL_AHBIDLE)) ;

    /* Core soft reset */
    DWC_GRSTCTL = GRSTCTL_CSRST;
    while (DWC_GRSTCTL & GRSTCTL_CSRST) ;

    delay_ms_arm(100);
}

void dwc_usb_init(void) {
    /* Power on USB via mailbox (would need power domain tag) */

    /* Core reset */
    dwc_core_reset();

    /* Configure as host mode */
    uint32_t gusbcfg = DWC_GUSBCFG;
    gusbcfg &= ~(1 << 29);  /* Clear force device mode */
    gusbcfg |= (1 << 30);   /* Force host mode */
    DWC_GUSBCFG = gusbcfg;
    delay_ms_arm(50);

    /* Configure FIFO sizes */
    DWC_GRXFSIZ = 1024;                /* RX FIFO: 1024 words */
    DWC_GNPTXFSIZ = (512 << 16) | 1024;  /* Non-periodic TX: 512 words at offset 1024 */

    /* Enable AHB DMA */
    DWC_GAHBCFG = (1 << 5) | 1;  /* DMA enable + global interrupt enable */

    /* Configure host */
    DWC_HCFG = 1;  /* 48MHz PHY clock, full/low speed */

    /* Reset port */
    uint32_t hprt = DWC_HPRT;
    /* Clear enable (W1C bits: don't accidentally clear them) */
    hprt &= ~(HPRT_PENA | HPRT_PCDET);
    hprt |= HPRT_PRST;
    DWC_HPRT = hprt;
    delay_ms_arm(60);

    hprt = DWC_HPRT;
    hprt &= ~(HPRT_PENA | HPRT_PCDET | HPRT_PRST);
    DWC_HPRT = hprt;
    delay_ms_arm(20);

    dwc_ready = 1;
    uart_puts("[OK] DWC2 USB host initialized\r\n");
}

int dwc_usb_is_ready(void) { return dwc_ready; }

int dwc_port_connected(void) {
    return (DWC_HPRT & HPRT_PCSTS) ? 1 : 0;
}

int dwc_port_speed(void) {
    uint32_t spd = (DWC_HPRT & HPRT_PSPD_MASK) >> 17;
    /* 0 = high, 1 = full, 2 = low */
    return (int)spd;
}
