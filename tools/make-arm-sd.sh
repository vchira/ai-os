#!/bin/bash
# AiOS — Create SD card image for Raspberry Pi
# Produces build/aios-arm-sd.img (dd-able to SD card)
#
# Requires: arm-none-eabi-gcc, dosfstools (mkfs.vfat)
#
# Usage: make arm-sd

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
BUILD_DIR="$ROOT_DIR/build"
ARM_DIR="$ROOT_DIR/arch/arm"
IMG="$BUILD_DIR/aios-arm-sd.img"
KERNEL="$BUILD_DIR/kernel7.img"

# Check prerequisites
for cmd in arm-none-eabi-gcc arm-none-eabi-ld arm-none-eabi-objcopy; do
    if ! command -v "$cmd" &>/dev/null; then
        echo "Error: $cmd not found. Install arm-none-eabi toolchain:"
        echo "  sudo apt install gcc-arm-none-eabi"
        exit 1
    fi
done

echo "=== Building AiOS for ARM (Raspberry Pi) ==="

mkdir -p "$BUILD_DIR/arm"

# Compile ARM assembly
echo "Assembling ARM boot code..."
arm-none-eabi-gcc -mcpu=arm1176jzf-s -fpic -ffreestanding -nostdlib \
    -c "$ARM_DIR/boot/start.S" -o "$BUILD_DIR/arm/start.o"

# Compile ARM C files
ARM_C_FILES=(
    "$ARM_DIR/kernel/kernel_arm.c"
    "$ARM_DIR/kernel/heap_arm.c"
    "$ARM_DIR/drivers/uart.c"
    "$ARM_DIR/drivers/gpio.c"
    "$ARM_DIR/drivers/mailbox.c"
    "$ARM_DIR/drivers/framebuffer_arm.c"
    "$ARM_DIR/drivers/timer_arm.c"
    "$ARM_DIR/drivers/interrupts_arm.c"
    "$ARM_DIR/drivers/dwc_usb.c"
)

# Shared C files that are architecture-independent
SHARED_C_FILES=(
    "$ROOT_DIR/lib/string.c"
    "$ROOT_DIR/lib/heap.c"
    "$ROOT_DIR/lib/snprintf.c"
)

ARM_CC_FLAGS="-mcpu=arm1176jzf-s -fpic -ffreestanding -nostdlib -nostdinc \
    -fno-builtin -fno-stack-protector -O2 -Wall -Wextra -Wno-unused-parameter \
    -I $ROOT_DIR -I $ROOT_DIR/include \
    -isystem $(arm-none-eabi-gcc -mcpu=arm1176jzf-s -print-file-name=include)"

echo "Compiling ARM kernel..."
OBJ_FILES="$BUILD_DIR/arm/start.o"

for src in "${ARM_C_FILES[@]}" "${SHARED_C_FILES[@]}"; do
    base=$(basename "$src" .c)
    obj="$BUILD_DIR/arm/$base.o"
    arm-none-eabi-gcc $ARM_CC_FLAGS -c "$src" -o "$obj"
    OBJ_FILES="$OBJ_FILES $obj"
done

# Link
echo "Linking ARM kernel..."
arm-none-eabi-ld -T "$ARM_DIR/linker.ld" -nostdlib \
    -o "$BUILD_DIR/arm/kernel_arm.elf" $OBJ_FILES

# Extract raw binary
arm-none-eabi-objcopy -O binary "$BUILD_DIR/arm/kernel_arm.elf" "$KERNEL"

echo "ARM kernel: $KERNEL ($(stat -c%s "$KERNEL") bytes)"

# Create SD card image (64MB FAT32)
echo "Creating SD card image..."
dd if=/dev/zero of="$IMG" bs=1M count=64 status=none

# Create MBR with single FAT32 partition
echo ",,0C,*" | sfdisk -q "$IMG" 2>/dev/null || true

# Format the partition as FAT32
LOOP=$(sudo losetup --find --show --offset $((2048 * 512)) "$IMG")
sudo mkfs.vfat -F 32 "$LOOP" >/dev/null
sudo mkdir -p /tmp/aios-arm-sd
sudo mount "$LOOP" /tmp/aios-arm-sd

# Copy boot files
sudo cp "$KERNEL" /tmp/aios-arm-sd/kernel7.img
sudo cp "$ARM_DIR/config.txt" /tmp/aios-arm-sd/

# RPi firmware files (needed from raspberry pi firmware repository)
# For a real SD card, you'd also need: bootcode.bin, start.elf, fixup.dat
# These are proprietary GPU firmware from https://github.com/raspberrypi/firmware
echo "NOTE: RPi boot requires firmware files (bootcode.bin, start.elf, fixup.dat)"
echo "      Download from: https://github.com/raspberrypi/firmware/tree/master/boot"

# Create a README on the SD card
cat > /tmp/aios-arm-readme.txt << 'HEREDOC'
AiOS - AI-Native Operating System (ARM/Raspberry Pi)

To boot:
1. Download RPi firmware files from:
   https://github.com/raspberrypi/firmware/tree/master/boot
2. Copy bootcode.bin, start.elf, and fixup.dat to this partition
3. Insert SD card into RPi and power on
4. Connect serial console (GPIO 14/15, 115200 baud) or HDMI
HEREDOC
sudo cp /tmp/aios-arm-readme.txt /tmp/aios-arm-sd/README.txt

sudo umount /tmp/aios-arm-sd
sudo losetup -d "$LOOP"
sudo rmdir /tmp/aios-arm-sd 2>/dev/null || true

echo "=== SD card image: $IMG ==="
echo "Write to SD card: sudo dd if=$IMG of=/dev/sdX bs=4M"
