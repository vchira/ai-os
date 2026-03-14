# AiOS — ARM Build (Raspberry Pi)
# Standalone Makefile for building the ARM kernel
# Requires: arm-none-eabi toolchain
#
# Usage: make -C arch/arm

CROSS = arm-none-eabi-
CC = $(CROSS)gcc
LD = $(CROSS)ld
OBJCOPY = $(CROSS)objcopy

ROOT = ../..
BUILD = $(ROOT)/build/arm

CFLAGS = -mcpu=arm1176jzf-s -fpic -ffreestanding -nostdlib -nostdinc \
         -fno-builtin -fno-stack-protector -O2 -Wall -Wextra \
         -Wno-unused-parameter \
         -I $(ROOT) -I $(ROOT)/include \
         -isystem $(shell $(CC) -mcpu=arm1176jzf-s -print-file-name=include)

LDFLAGS = -T linker.ld -nostdlib

# ARM-specific sources
ARM_ASM = boot/start.S
ARM_C = kernel/kernel_arm.c \
        kernel/heap_arm.c \
        drivers/uart.c \
        drivers/gpio.c \
        drivers/mailbox.c \
        drivers/framebuffer_arm.c \
        drivers/timer_arm.c \
        drivers/interrupts_arm.c \
        drivers/dwc_usb.c

# Shared sources (architecture-independent)
SHARED_C = $(ROOT)/lib/string.c \
           $(ROOT)/lib/heap.c \
           $(ROOT)/lib/snprintf.c

ASM_OBJ = $(patsubst %.S,$(BUILD)/%.o,$(ARM_ASM))
ARM_OBJ = $(patsubst %.c,$(BUILD)/%.o,$(ARM_C))
SHARED_OBJ = $(patsubst $(ROOT)/%.c,$(BUILD)/shared_%.o,$(SHARED_C))

ALL_OBJ = $(ASM_OBJ) $(ARM_OBJ) $(SHARED_OBJ)

.PHONY: all clean

all: $(ROOT)/build/kernel7.img

$(BUILD)/%.o: %.S
	@mkdir -p $(dir $@)
	$(CC) $(CFLAGS) -c $< -o $@

$(BUILD)/%.o: %.c
	@mkdir -p $(dir $@)
	$(CC) $(CFLAGS) -c $< -o $@

$(BUILD)/shared_%.o: $(ROOT)/%.c
	@mkdir -p $(dir $@)
	$(CC) $(CFLAGS) -c $< -o $@

$(BUILD)/kernel_arm.elf: $(ALL_OBJ)
	$(LD) $(LDFLAGS) -o $@ $^

$(ROOT)/build/kernel7.img: $(BUILD)/kernel_arm.elf
	$(OBJCOPY) -O binary $< $@
	@echo "ARM kernel: $@ ($$(stat -c%s $@) bytes)"

clean:
	rm -rf $(BUILD) $(ROOT)/build/kernel7.img
