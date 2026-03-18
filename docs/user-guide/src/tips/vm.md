# Running in a VM

AiOS runs well in virtual machines. This is useful for testing, development, and trying AiOS without dedicating physical hardware.

## QEMU/KVM (recommended)

QEMU with KVM acceleration provides the best performance for Linux virtual machines.

### Using the included script

The AiOS source repository includes a VM launcher script:

```bash
./distro/run-vm.sh
```

This script configures QEMU with:
- KVM acceleration (if available)
- Virtio graphics for good resolution support
- PipeWire audio passthrough
- SPICE protocol for clipboard sharing
- Appropriate memory and CPU allocation

### Manual QEMU command

If you want to run QEMU manually:

```bash
qemu-system-x86_64 \
  -enable-kvm \
  -m 4G \
  -smp 2 \
  -vga virtio \
  -display spice-app \
  -device virtio-net-pci,netdev=net0 \
  -netdev user,id=net0 \
  -cdrom aios-2.0.31.iso \
  -boot d
```

Key options:
- `-enable-kvm` -- use hardware virtualization (much faster)
- `-m 4G` -- allocate 4 GB RAM (minimum recommended)
- `-vga virtio` -- virtio GPU for best resolution support
- `-display spice-app` -- SPICE display with clipboard sharing
- `-boot d` -- boot from CD-ROM (the ISO)

### Installing to a virtual disk

To test hard drive installation:

```bash
# Create a virtual disk
qemu-img create -f qcow2 aios-disk.qcow2 16G

# Boot with both the ISO and the virtual disk
qemu-system-x86_64 \
  -enable-kvm \
  -m 4G \
  -smp 2 \
  -vga virtio \
  -display spice-app \
  -cdrom aios-2.0.31.iso \
  -drive file=aios-disk.qcow2,format=qcow2 \
  -boot d
```

During the setup wizard, AiOS will detect the virtual disk and offer to install.

### Audio in QEMU

For voice interaction in QEMU, you need audio passthrough:

```bash
-audio driver=pipewire,model=virtio
```

Or using PulseAudio:

```bash
-audio driver=pa,model=virtio
```

Audio in VMs can be tricky. If voice is not working, text interaction works without any audio configuration.

### Clipboard sharing

For clipboard sharing between the host and guest, use SPICE with `virt-viewer`:

```bash
sudo apt install virt-viewer    # On the host
```

The `run-vm.sh` script configures SPICE automatically.

## VirtualBox

### Creating a VM

1. Open VirtualBox and click "New"
2. Set the following:
   - **Name:** AiOS
   - **Type:** Linux
   - **Version:** Debian (64-bit)
3. Allocate at least **4096 MB** of RAM
4. Create a virtual hard disk (16 GB or more, VDI format)
5. Go to **Settings**:
   - **System > Processor:** 2 or more CPUs
   - **Display > Graphics Controller:** VBoxSVGA
   - **Display > Video Memory:** 128 MB
   - **Storage:** Add the AiOS ISO as an optical disk
   - **Audio:** Enable audio, choose your host driver
   - **Network:** NAT (default) for internet access

### Using the included script

The source repository also includes a VirtualBox launcher:

```bash
./distro/run-vm-vbox.sh
```

### VirtualBox limitations

- Audio passthrough is less reliable than QEMU
- Resolution changes may require Guest Additions (not included in AiOS by default)
- Performance is generally lower than QEMU/KVM

## Tips for VM usage

1. **Allocate enough RAM.** 4 GB is the minimum. 8 GB allows better voice recognition models.
2. **Enable hardware virtualization** in your BIOS/UEFI (Intel VT-x / AMD-V). Without it, the VM will be extremely slow.
3. **Use a bridged network** instead of NAT if you want to access the web channel from the host or other devices.
4. **Audio is optional.** If you cannot get audio working in the VM, use text-only mode (`/mic off` and `/speaker off`).
5. **Snapshots are your friend.** Take a VM snapshot after setup completes so you can quickly restore to a clean state.
