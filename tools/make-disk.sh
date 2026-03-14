#!/bin/bash
# AiOS — Create bootable disk image (dd-able to HDD/SSD/SD card)
# Usage: bash tools/make-disk.sh [output-path]
#
# Disk layout:
#   Sector 0         MBR (partition table + GRUB boot code)
#   Sector 1-2047    GRUB core image + reserved
#   Sector 2048-2115 Persistence area (AI memory, tools, scheduler)
#   Sector 4096+     FAT16 partition (/boot/aios.bin, /boot/grub/grub.cfg)

set -e

DISK="${1:-build/aios-boot.img}"
KERNEL="build/aios.bin"
GRUB_CFG="tools/grub-disk.cfg"
GRUB_MODS="/usr/lib/grub/i386-pc"
SIZE_MB=32
PART_START=4096  # sectors (2MB offset, leaves room for persistence at LBA 2048)

# Check prerequisites
missing=""
for tool in sfdisk mformat mcopy mmd grub-mkimage; do
    if ! command -v "$tool" &>/dev/null; then
        missing="$missing $tool"
    fi
done
if [ -n "$missing" ]; then
    echo "Missing tools:$missing"
    echo "Install: sudo apt install mtools grub-pc-bin"
    exit 1
fi

if [ ! -f "$KERNEL" ]; then
    echo "Error: $KERNEL not found. Run 'make' first."
    exit 1
fi

if [ ! -d "$GRUB_MODS" ]; then
    echo "Error: GRUB i386-pc modules not found at $GRUB_MODS"
    echo "Install: sudo apt install grub-pc-bin"
    exit 1
fi

echo "Creating ${SIZE_MB}MB bootable disk image..."

TOTAL_SECTORS=$((SIZE_MB * 2048))
PART_SECTORS=$((TOTAL_SECTORS - PART_START))

# 1. Create blank disk image
dd if=/dev/zero of="$DISK" bs=512 count=$TOTAL_SECTORS status=none

# 2. Create MBR partition table with one FAT16 partition
sfdisk --quiet "$DISK" <<EOF
label: dos
start=${PART_START}, type=06
EOF

# 3. Create and format FAT16 partition as a separate file
TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

dd if=/dev/zero of="$TMPDIR/fat.img" bs=512 count=$PART_SECTORS status=none
mformat -i "$TMPDIR/fat.img" -v AIOS ::

# 4. Populate filesystem
mmd -i "$TMPDIR/fat.img" ::/boot
mmd -i "$TMPDIR/fat.img" ::/boot/grub
mcopy -i "$TMPDIR/fat.img" "$KERNEL" ::/boot/aios.bin
mcopy -i "$TMPDIR/fat.img" "$GRUB_CFG" ::/boot/grub/grub.cfg

# 5. Insert FAT partition into disk image at partition offset
dd if="$TMPDIR/fat.img" of="$DISK" bs=512 seek=$PART_START conv=notrunc status=none

# 6. Install GRUB boot code
# Embedded config tells GRUB where to find its modules and config
cat > "$TMPDIR/embed.cfg" <<'GCEOF'
set root=(hd0,msdos1)
set prefix=(hd0,msdos1)/boot/grub
configfile /boot/grub/grub.cfg
GCEOF

# Build GRUB core image with required modules
grub-mkimage -O i386-pc \
    -o "$TMPDIR/core.img" \
    -c "$TMPDIR/embed.cfg" \
    -p '(hd0,msdos1)/boot/grub' \
    biosdisk part_msdos fat normal multiboot boot configfile \
    vbe all_video gfxterm

# Check core.img fits before partition start
CORE_SECTORS=$(( ($(stat -c%s "$TMPDIR/core.img") + 511) / 512 ))
if [ $((CORE_SECTORS + 1)) -ge $PART_START ]; then
    echo "Error: GRUB core.img too large ($CORE_SECTORS sectors)"
    exit 1
fi

# Write MBR boot code (first 440 bytes, preserves partition table)
dd if="$GRUB_MODS/boot.img" of="$DISK" bs=440 count=1 conv=notrunc status=none

# Write core.img starting at sector 1
dd if="$TMPDIR/core.img" of="$DISK" bs=512 seek=1 conv=notrunc status=none

KERNEL_SIZE=$(stat -c%s "$KERNEL")
echo "Done: $DISK (${SIZE_MB}MB)"
echo "  Kernel: $KERNEL_SIZE bytes"
echo "  GRUB core: $CORE_SECTORS sectors"
echo ""
echo "Install to disk:"
echo "  dd if=$DISK of=/dev/sdX bs=4M status=progress"
