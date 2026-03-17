#!/bin/bash
# AiOS VM launcher using libvirt/KVM with audio + clipboard support
#
# Strategy: create the domain XML directly (not via virt-install) so we can
# include sound card, audio backend, SPICE clipboard channel, and security
# labels correctly from the start — no destroy/redefine dance needed.

set -euo pipefail

ISO="${1:-build/live-image-amd64.hybrid.iso}"

if [ ! -f "${ISO}" ]; then
    echo "ISO not found: ${ISO}"
    echo "Run ./build.sh first"
    exit 1
fi

ISO="$(cd "$(dirname "${ISO}")" && pwd)/$(basename "${ISO}")"
VM_NAME="aios-live"
UID_NUM=$(id -u)

# ─── Check dependencies ──────────────────────────────────────
for cmd in virsh virt-viewer; do
    if ! command -v "${cmd}" &>/dev/null; then
        echo "${cmd} not installed. Install with:"
        echo "  sudo apt install virt-manager virt-viewer"
        exit 1
    fi
done

# Make sure libvirtd is running
if ! systemctl is-active --quiet libvirtd 2>/dev/null; then
    echo "[*] Starting libvirtd..."
    sudo systemctl start libvirtd
fi

# ─── Clean up previous VM ────────────────────────────────────
if virsh --connect qemu:///system list --all --name 2>/dev/null | grep -q "^${VM_NAME}$"; then
    echo "[*] Removing previous ${VM_NAME} VM..."
    virsh --connect qemu:///system destroy "${VM_NAME}" 2>/dev/null || true
    virsh --connect qemu:///system undefine "${VM_NAME}" 2>/dev/null || true
fi

# ─── Check QEMU user config ──────────────────────────────────
QEMU_CONF="/etc/libvirt/qemu.conf"
CURRENT_USER=$(whoami)
if [ -f "${QEMU_CONF}" ]; then
    if ! grep -q "^user = \"${CURRENT_USER}\"" "${QEMU_CONF}" 2>/dev/null; then
        echo ""
        echo "[!] AUDIO FIX NEEDED: QEMU needs to run as your user for audio."
        echo "    Run once:"
        echo "      sudo sed -i 's/^#user = \"libvirt-qemu\"/user = \"${CURRENT_USER}\"/' ${QEMU_CONF}"
        echo "      sudo sed -i 's/^#group = \"libvirt-qemu\"/group = \"${CURRENT_USER}\"/' ${QEMU_CONF}"
        echo "      sudo systemctl restart libvirtd"
        echo ""
    fi
fi

# ─── Audio strategy ───────────────────────────────────────────
# Use SPICE audio: sound is carried over the SPICE protocol to the viewer.
# This avoids PipeWire/PulseAudio permission issues with libvirt-qemu user.
echo "[*] Audio: SPICE (routed through viewer)"
AUDIO_XML="<audio id='1' type='spice'/>"

# ─── Build domain XML directly ───────────────────────────────
echo "[*] Creating AiOS VM..."
echo "[*] ISO: ${ISO}"

TMPXML=$(mktemp /tmp/aios-vm-XXXXX.xml)
cat > "${TMPXML}" << XMLEOF
<domain type='kvm'>
  <name>${VM_NAME}</name>
  <memory unit='MiB'>4096</memory>
  <vcpu>4</vcpu>

  <os>
    <type arch='x86_64' machine='q35'>hvm</type>
    <boot dev='cdrom'/>
  </os>

  <features>
    <acpi/>
    <apic/>
  </features>

  <cpu mode='host-passthrough'/>

  <devices>
    <!-- Boot ISO -->
    <disk type='file' device='cdrom'>
      <driver name='qemu' type='raw'/>
      <source file='${ISO}'/>
      <target dev='sda' bus='sata'/>
      <readonly/>
    </disk>

    <!-- Network -->
    <interface type='network'>
      <source network='default'/>
      <model type='virtio'/>
    </interface>

    <!-- Graphics: SPICE with clipboard/paste support -->
    <graphics type='spice' autoport='yes'>
      <clipboard copypaste='yes'/>
      <filetransfer enable='yes'/>
    </graphics>

    <!-- Video -->
    <video>
      <model type='qxl' ram='65536' vram='65536'/>
    </video>

    <!-- SPICE agent channel (clipboard sharing) -->
    <channel type='spicevmc'>
      <target type='virtio' name='com.redhat.spice.0'/>
    </channel>

    <!-- Sound card: Intel HDA -->
    <sound model='ich9'>
      <audio id='1'/>
    </sound>

    <!-- Audio backend: connects QEMU to host audio -->
    ${AUDIO_XML}

    <!-- Tablet for better mouse integration -->
    <input type='tablet' bus='usb'/>

  </devices>

  <!-- Disable security labels so QEMU runs as the configured user -->
  <seclabel type='none' model='none'/>
</domain>
XMLEOF

# Define and start
virsh --connect qemu:///system define "${TMPXML}" > /dev/null
rm -f "${TMPXML}"

echo "[*] Starting VM..."
virsh --connect qemu:///system start "${VM_NAME}"

echo "[*] VM started"
echo "[*] Audio + clipboard enabled via SPICE"

# ─── Connect viewer ──────────────────────────────────────────
sleep 2

SPICE_URI=$(virsh --connect qemu:///system domdisplay "${VM_NAME}" 2>/dev/null || true)

if [ -n "${SPICE_URI}" ] && command -v remote-viewer &>/dev/null; then
    echo "[*] Connecting: ${SPICE_URI}"
    exec remote-viewer "${SPICE_URI}"
elif command -v virt-viewer &>/dev/null; then
    echo "[*] Connecting with virt-viewer..."
    exec virt-viewer --connect qemu:///system "${VM_NAME}"
else
    echo "[*] Connect manually with: virt-manager --connect qemu:///system"
fi
