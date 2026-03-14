#include "include/pci.h"
#include "include/io.h"

extern void fb_print(const char *str);
extern void fb_newline(void);

static uint32_t pci_addr(uint8_t bus, uint8_t dev, uint8_t func, uint8_t offset) {
    return (1u << 31) | ((uint32_t)bus << 16) | ((uint32_t)dev << 11)
         | ((uint32_t)func << 8) | (offset & 0xFC);
}

uint32_t pci_read32(uint8_t bus, uint8_t dev, uint8_t func, uint8_t offset) {
    outl(PCI_CONFIG_ADDR, pci_addr(bus, dev, func, offset));
    return inl(PCI_CONFIG_DATA);
}

uint16_t pci_read16(uint8_t bus, uint8_t dev, uint8_t func, uint8_t offset) {
    outl(PCI_CONFIG_ADDR, pci_addr(bus, dev, func, offset));
    return (uint16_t)(inl(PCI_CONFIG_DATA) >> ((offset & 2) * 8));
}

void pci_write32(uint8_t bus, uint8_t dev, uint8_t func, uint8_t offset, uint32_t val) {
    outl(PCI_CONFIG_ADDR, pci_addr(bus, dev, func, offset));
    outl(PCI_CONFIG_DATA, val);
}

void pci_write16(uint8_t bus, uint8_t dev, uint8_t func, uint8_t offset, uint16_t val) {
    outl(PCI_CONFIG_ADDR, pci_addr(bus, dev, func, offset));
    uint32_t tmp = inl(PCI_CONFIG_DATA);
    int shift = (offset & 2) * 8;
    tmp &= ~(0xFFFF << shift);
    tmp |= (uint32_t)val << shift;
    outl(PCI_CONFIG_DATA, tmp);
}

static void pci_read_bars(pci_device_t *dev) {
    for (int i = 0; i < 6; i++) {
        dev->bar[i] = pci_read32(dev->bus, dev->dev, dev->func, 0x10 + i * 4);
    }
}

int pci_find_device(uint16_t vendor_id, uint16_t device_id, pci_device_t *out) {
    /* Only scan buses 0-7; VMs and most real HW use bus 0 */
    for (int bus = 0; bus < 8; bus++) {
        for (int dev = 0; dev < PCI_MAX_DEV; dev++) {
            uint32_t reg0 = pci_read32(bus, dev, 0, 0);
            if ((reg0 & 0xFFFF) == 0xFFFF) continue;

            for (int func = 0; func < PCI_MAX_FUNC; func++) {
                reg0 = pci_read32(bus, dev, func, 0);
                uint16_t vid = reg0 & 0xFFFF;
                uint16_t did = reg0 >> 16;

                if (vid == 0xFFFF) continue;
                if (vid == vendor_id && did == device_id) {
                    out->bus = bus;
                    out->dev = dev;
                    out->func = func;
                    out->vendor_id = vid;
                    out->device_id = did;
                    uint32_t reg2 = pci_read32(bus, dev, func, 0x08);
                    out->class_code = (reg2 >> 24) & 0xFF;
                    out->subclass = (reg2 >> 16) & 0xFF;
                    uint32_t reg3C = pci_read32(bus, dev, func, 0x3C);
                    out->irq = reg3C & 0xFF;
                    pci_read_bars(out);
                    return 1;
                }
            }
        }
    }
    return 0;
}

/* Print a 4-digit hex value (for compact PCI output) */
static void print_hex16(uint16_t val) {
    static const char hex[] = "0123456789ABCDEF";
    char buf[5];
    buf[0] = hex[(val >> 12) & 0xF];
    buf[1] = hex[(val >> 8) & 0xF];
    buf[2] = hex[(val >> 4) & 0xF];
    buf[3] = hex[val & 0xF];
    buf[4] = '\0';
    fb_print(buf);
}

static void print_hex8(uint8_t val) {
    static const char hex[] = "0123456789ABCDEF";
    char buf[3];
    buf[0] = hex[(val >> 4) & 0xF];
    buf[1] = hex[val & 0xF];
    buf[2] = '\0';
    fb_print(buf);
}

void pci_scan(void) {
    fb_print("PCI devices:\n");
    for (int bus = 0; bus < 8; bus++) {
        for (int dev = 0; dev < 32; dev++) {
            uint32_t reg0 = pci_read32(bus, dev, 0, 0);
            uint16_t vid = reg0 & 0xFFFF;
            if (vid == 0xFFFF) continue;

            uint16_t did = reg0 >> 16;
            uint32_t reg2 = pci_read32(bus, dev, 0, 0x08);
            uint8_t cls = (reg2 >> 24) & 0xFF;
            uint8_t sub = (reg2 >> 16) & 0xFF;

            fb_print("  ");
            print_hex8(bus);
            fb_print(":");
            print_hex8(dev);
            fb_print(" ");
            print_hex16(vid);
            fb_print(":");
            print_hex16(did);
            fb_print(" class ");
            print_hex8(cls);
            fb_print(".");
            print_hex8(sub);
            fb_newline();
        }
    }
}

uint8_t pci_get_progif(uint8_t bus, uint8_t dev, uint8_t func) {
    uint32_t reg2 = pci_read32(bus, dev, func, 0x08);
    return (reg2 >> 8) & 0xFF;
}

int pci_find_class(uint8_t cls, uint8_t sub, int progif,
                   int start_bus, int start_dev, int start_func,
                   pci_device_t *out) {
    for (int bus = start_bus; bus < 8; bus++) {
        int d0 = (bus == start_bus) ? start_dev : 0;
        for (int dev = d0; dev < 32; dev++) {
            uint32_t reg0 = pci_read32(bus, dev, 0, 0);
            if ((reg0 & 0xFFFF) == 0xFFFF) continue;

            int f0 = (bus == start_bus && dev == start_dev) ? start_func : 0;
            for (int func = f0; func < 8; func++) {
                reg0 = pci_read32(bus, dev, func, 0);
                uint16_t vid = reg0 & 0xFFFF;
                if (vid == 0xFFFF) continue;

                uint32_t reg2 = pci_read32(bus, dev, func, 0x08);
                uint8_t c = (reg2 >> 24) & 0xFF;
                uint8_t s = (reg2 >> 16) & 0xFF;
                uint8_t pi = (reg2 >> 8) & 0xFF;

                if (c == cls && s == sub && (progif < 0 || pi == (uint8_t)progif)) {
                    out->bus = bus;
                    out->dev = dev;
                    out->func = func;
                    out->vendor_id = vid;
                    out->device_id = reg0 >> 16;
                    out->class_code = c;
                    out->subclass = s;
                    uint32_t reg3C = pci_read32(bus, dev, func, 0x3C);
                    out->irq = reg3C & 0xFF;
                    pci_read_bars(out);
                    return 1;
                }

                /* If not multifunction, skip remaining funcs */
                if (func == 0) {
                    uint32_t reg3 = pci_read32(bus, dev, 0, 0x0C);
                    if (!((reg3 >> 16) & 0x80)) break;
                }
            }
        }
    }
    return 0;
}

void pci_enable_bus_mastering(pci_device_t *dev) {
    uint16_t cmd = pci_read16(dev->bus, dev->dev, dev->func, 0x04);
    cmd |= (1 << 2); /* Bus Master Enable */
    pci_write16(dev->bus, dev->dev, dev->func, 0x04, cmd);
}
