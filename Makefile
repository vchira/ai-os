NASM = $(HOME)/.local/bin/nasm
LD = ld
CC = gcc
XORRISO = $(HOME)/.local/bin/xorriso
GRUB_MKRESCUE = grub2-mkrescue

NASM_FLAGS = -f elf32 -I ./
GCC_INCLUDES = $(shell $(CC) -m32 -print-file-name=include)
CC_FLAGS = -m32 -ffreestanding -nostdlib -nostdinc -fno-builtin -fno-stack-protector \
           -fno-pic -O2 -Wall -Wextra -Wno-unused-parameter \
           -isystem $(GCC_INCLUDES) \
           -I . -I lib/lwip/src/include -I include \
           -DMBEDTLS_CONFIG_FILE='"include/mbedtls_config.h"'
LD_FLAGS = -m elf_i386 -T linker.ld -nostdlib

BUILD_DIR = build
ISO_DIR = iso

# Local GRUB i386-pc modules (extracted from RPM)
GRUB_MODULES = /tmp/grub-local/usr/lib/grub/i386-pc

# Assembly source files
ASM_SOURCES = boot/boot.asm \
              kernel/kernel.asm \
              kernel/gdt.asm \
              kernel/idt.asm \
              kernel/memory.asm \
              kernel/paging.asm \
              kernel/crt.asm \
              kernel/c_api.asm \
              drivers/vga.asm \
              drivers/keyboard.asm \
              drivers/timer.asm \
              shell/shell.asm

# C source files — AiOS core
C_SOURCES = lib/string.c \
            lib/heap.c \
            lib/sys_arch.c \
            lib/netif_rtl.c \
            lib/http_client.c \
            lib/claude_api.c \
            drivers/pci.c \
            drivers/rtl8139.c

# lwIP core sources (TCP/IP stack)
LWIP_CORE = lib/lwip/src/core/init.c \
            lib/lwip/src/core/def.c \
            lib/lwip/src/core/dns.c \
            lib/lwip/src/core/inet_chksum.c \
            lib/lwip/src/core/ip.c \
            lib/lwip/src/core/mem.c \
            lib/lwip/src/core/memp.c \
            lib/lwip/src/core/netif.c \
            lib/lwip/src/core/pbuf.c \
            lib/lwip/src/core/raw.c \
            lib/lwip/src/core/stats.c \
            lib/lwip/src/core/sys.c \
            lib/lwip/src/core/tcp.c \
            lib/lwip/src/core/tcp_in.c \
            lib/lwip/src/core/tcp_out.c \
            lib/lwip/src/core/timeouts.c \
            lib/lwip/src/core/udp.c \
            lib/lwip/src/core/ipv4/autoip.c \
            lib/lwip/src/core/ipv4/dhcp.c \
            lib/lwip/src/core/ipv4/etharp.c \
            lib/lwip/src/core/ipv4/icmp.c \
            lib/lwip/src/core/ipv4/igmp.c \
            lib/lwip/src/core/ipv4/ip4.c \
            lib/lwip/src/core/ipv4/ip4_addr.c \
            lib/lwip/src/core/ipv4/ip4_frag.c \
            lib/lwip/src/core/ipv4/acd.c \
            lib/lwip/src/netif/ethernet.c

# Object files
ASM_OBJ = $(patsubst %.asm,$(BUILD_DIR)/%.o,$(ASM_SOURCES))
C_OBJ = $(patsubst %.c,$(BUILD_DIR)/%.o,$(C_SOURCES))
LWIP_OBJ = $(patsubst %.c,$(BUILD_DIR)/%.o,$(LWIP_CORE))
ALL_OBJ = $(ASM_OBJ) $(C_OBJ) $(LWIP_OBJ)

.PHONY: all clean iso run

all: iso

# Assemble each .asm file to .o
$(BUILD_DIR)/%.o: %.asm
	@mkdir -p $(dir $@)
	$(NASM) $(NASM_FLAGS) $< -o $@

# Compile each .c file to .o
$(BUILD_DIR)/%.o: %.c
	@mkdir -p $(dir $@)
	$(CC) $(CC_FLAGS) -c $< -o $@

# Link all object files into kernel binary
$(BUILD_DIR)/aios.bin: $(ALL_OBJ)
	$(LD) $(LD_FLAGS) -o $@ $^

# Create bootable ISO using GRUB2
iso: $(BUILD_DIR)/aios.bin
	@mkdir -p $(ISO_DIR)/boot/grub
	cp $(BUILD_DIR)/aios.bin $(ISO_DIR)/boot/aios.bin
	$(GRUB_MKRESCUE) --xorriso=$(XORRISO) \
		--directory=$(GRUB_MODULES) \
		-o $(BUILD_DIR)/aios.iso $(ISO_DIR)

# Run in QEMU with RTL8139 NIC
run: iso
	qemu-system-i386 -cdrom $(BUILD_DIR)/aios.iso -m 64M \
		-netdev user,id=net0 -device rtl8139,netdev=net0

clean:
	rm -rf $(BUILD_DIR)
	rm -f $(ISO_DIR)/boot/aios.bin
