#ifndef AIOS_PCI_H
#define AIOS_PCI_H

#include "types.h"

#define PCI_CONFIG_ADDR  0xCF8
#define PCI_CONFIG_DATA  0xCFC

#define PCI_MAX_BUS      256
#define PCI_MAX_DEV      32
#define PCI_MAX_FUNC     8

typedef struct {
    uint8_t  bus;
    uint8_t  dev;
    uint8_t  func;
    uint16_t vendor_id;
    uint16_t device_id;
    uint8_t  class_code;
    uint8_t  subclass;
    uint32_t bar[6];
    uint8_t  irq;
} pci_device_t;

uint32_t pci_read32(uint8_t bus, uint8_t dev, uint8_t func, uint8_t offset);
uint16_t pci_read16(uint8_t bus, uint8_t dev, uint8_t func, uint8_t offset);
void     pci_write32(uint8_t bus, uint8_t dev, uint8_t func, uint8_t offset, uint32_t val);
void     pci_write16(uint8_t bus, uint8_t dev, uint8_t func, uint8_t offset, uint16_t val);

/* Find a PCI device by vendor/device ID. Returns 1 if found, 0 if not. */
int pci_find_device(uint16_t vendor_id, uint16_t device_id, pci_device_t *out);

/* Find a PCI device by class/subclass/progif. Returns 1 if found, 0 if not.
   Pass -1 for any field to match any value. start_bus/start_dev allow iteration. */
int pci_find_class(uint8_t cls, uint8_t sub, int progif,
                   int start_bus, int start_dev, int start_func,
                   pci_device_t *out);

/* Read the programming interface byte */
uint8_t pci_get_progif(uint8_t bus, uint8_t dev, uint8_t func);

/* Scan and print all PCI devices */
void pci_scan(void);

/* Enable bus mastering for a device */
void pci_enable_bus_mastering(pci_device_t *dev);

#endif
