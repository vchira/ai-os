#!/bin/bash
# AiOS Linux Distribution Builder
# Based on Debian Bookworm with live-build
#
# Prerequisites:
#   sudo apt install live-build live-boot live-config
#
# Usage:
#   cd distro && sudo ./build.sh
#
# Outputs:
#   build/aios-live-amd64.hybrid.iso

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BUILD_DIR="${SCRIPT_DIR}/build"
AIOS_APP_DIR="${SCRIPT_DIR}/../aios-app"

echo "========================================"
echo "  AiOS Linux Distribution Builder"
echo "  Base: Debian Bookworm (12)"
echo "========================================"
echo ""

# Clean previous build
if [ -d "${BUILD_DIR}" ]; then
    echo "[*] Cleaning previous build..."
    cd "${BUILD_DIR}"
    sudo lb clean --purge 2>/dev/null || true
    cd "${SCRIPT_DIR}"
    rm -rf "${BUILD_DIR}"
fi

mkdir -p "${BUILD_DIR}"
cd "${BUILD_DIR}"

# ─── Configure live-build ─────────────────────────────────────────

echo "[*] Configuring live-build..."

lb config \
    --distribution bookworm \
    --archive-areas "main contrib non-free non-free-firmware" \
    --architectures amd64 \
    --binary-images iso-hybrid \
    --bootloaders "grub-efi,syslinux" \
    --debian-installer live \
    --debian-installer-gui true \
    --firmware-binary true \
    --firmware-chroot true \
    --iso-application "AiOS" \
    --iso-publisher "AiOS Project" \
    --iso-volume "AiOS Live" \
    --memtest none \
    --apt-indices false \
    --cache true \
    --linux-flavours "amd64" \
    --security true \
    --updates true \
    --backports true

# ─── Package Lists ───────────────────────────────────────────────

echo "[*] Setting up package lists..."

# Core system packages
cat > config/package-lists/base.list.chroot << 'PKGEOF'
# Base system
linux-headers-amd64
firmware-linux
firmware-misc-nonfree
firmware-iwlwifi
firmware-realtek
firmware-atheros
firmware-intel-sound

# System utilities
sudo
systemd
dbus
polkitd
udev
networkmanager
network-manager-gnome
bluez
pipewire
pipewire-pulse
wireplumber
libspa-0.2-bluetooth
alsa-utils
pulseaudio-utils
PKGEOF

# Desktop environment (Wayland + labwc)
cat > config/package-lists/desktop.list.chroot << 'PKGEOF'
# Wayland compositor
labwc
xwayland
wlr-randr
swaybg
swaylock
swayidle
grim
slurp
wl-clipboard
mako-notifier
foot
thunar
fuzzel

# GTK4 + libadwaita
libgtk-4-1
libadwaita-1-0
gir1.2-gtk-4.0
gir1.2-adw-1
adwaita-icon-theme

# Display manager
greetd
PKGEOF

# AiOS application dependencies
cat > config/package-lists/aios.list.chroot << 'PKGEOF'
# Python
python3
python3-pip
python3-venv
python3-gi
python3-gi-cairo
python3-numpy
python3-requests
python3-pil

# Audio for voice
portaudio19-dev
python3-sounddevice
libsndfile1

# Build tools for Python packages
python3-dev
build-essential
git
curl
wget
PKGEOF

# Calamares installer
cat > config/package-lists/installer.list.chroot << 'PKGEOF'
calamares
calamares-settings-debian
PKGEOF

# ─── Chroot Hooks ───────────────────────────────────────────────

echo "[*] Setting up hooks..."

mkdir -p config/hooks/live

# Main setup hook — runs inside the chroot during build
cat > config/hooks/live/0100-setup-aios.hook.chroot << 'HOOKEOF'
#!/bin/bash
set -e

echo "[AiOS] Setting up AiOS distribution..."

# Create aios user
useradd -m -G sudo,audio,video,input,render -s /bin/bash aios 2>/dev/null || true
echo "aios:aios" | chpasswd
echo "aios ALL=(ALL) NOPASSWD:ALL" > /etc/sudoers.d/aios

# Install AiOS application
if [ -d /opt/aios-app ]; then
    cd /opt/aios-app
    python3 -m pip install --break-system-packages -e . 2>/dev/null || \
    python3 -m pip install -e . 2>/dev/null || true
fi

# Install Python dependencies not in Debian repos
python3 -m pip install --break-system-packages \
    anthropic openai faster-whisper piper-tts toml markdown 2>/dev/null || \
python3 -m pip install \
    anthropic openai faster-whisper piper-tts toml markdown 2>/dev/null || true

# Create AiOS data directories
mkdir -p /home/aios/.aios/{models/whisper,models/piper,plugins,memory}
chown -R aios:aios /home/aios/.aios

# Default AiOS config
cat > /home/aios/.aios/config.json << 'CFGEOF'
{
  "llm": {
    "provider": "claude",
    "claude_api_key": "",
    "claude_model": "claude-sonnet-4-20250514",
    "openai_api_key": "",
    "openai_model": "gpt-4o",
    "extra_system_prompt": "",
    "max_tool_rounds": 10
  },
  "voice": {
    "stt_enabled": true,
    "stt_model": "medium",
    "stt_language": "",
    "tts_enabled": true,
    "tts_voice": "en_US-amy-medium",
    "tts_gender": "female",
    "tts_rate": 1.0
  },
  "ui": {
    "theme": "dark"
  },
  "system": {
    "keyboard_layout": "us"
  }
}
CFGEOF
chown aios:aios /home/aios/.aios/config.json

# Configure greetd (display manager)
mkdir -p /etc/greetd
cat > /etc/greetd/config.toml << 'GREETEOF'
[terminal]
vt = 7

[default_session]
command = "labwc -s /opt/aios-app/aios-session.sh"
user = "aios"
GREETEOF

# Enable greetd
systemctl enable greetd 2>/dev/null || true

# Configure PipeWire for audio
systemctl --global enable pipewire pipewire-pulse wireplumber 2>/dev/null || true

echo "[AiOS] Setup complete."
HOOKEOF
chmod +x config/hooks/live/0100-setup-aios.hook.chroot

# ─── Chroot Includes ─────────────────────────────────────────────

echo "[*] Setting up chroot includes..."

# Copy AiOS application into the build
mkdir -p config/includes.chroot/opt/aios-app
cp -r "${AIOS_APP_DIR}"/* config/includes.chroot/opt/aios-app/ 2>/dev/null || true

# AiOS session startup script
cat > config/includes.chroot/opt/aios-app/aios-session.sh << 'SESSEOF'
#!/bin/bash
# AiOS session startup — runs inside labwc

# Wait for compositor
sleep 1

# Set wallpaper
swaybg -m fill -i /usr/share/aios/wallpaper.png 2>/dev/null &

# Start notification daemon
mako &

# Start AiOS application
exec python3 -m aios
SESSEOF
chmod +x config/includes.chroot/opt/aios-app/aios-session.sh

# labwc configuration for the live user
mkdir -p config/includes.chroot/etc/skel/.config/labwc

cat > config/includes.chroot/etc/skel/.config/labwc/rc.xml << 'RCEOF'
<?xml version="1.0"?>
<labwc_config>
  <core>
    <gap>0</gap>
  </core>
  <theme>
    <name>AiOS</name>
    <cornerRadius>8</cornerRadius>
    <font name="sans" size="11"/>
  </theme>
  <keyboard>
    <keybind key="A-F4">
      <action name="Close"/>
    </keybind>
    <keybind key="A-Return">
      <action name="Execute"><command>foot</command></action>
    </keybind>
    <keybind key="Super_L">
      <action name="Execute"><command>fuzzel</command></action>
    </keybind>
    <keybind key="A-F11">
      <action name="ToggleFullscreen"/>
    </keybind>
    <keybind key="Print">
      <action name="Execute"><command>grim</command></action>
    </keybind>
  </keyboard>
  <mouse/>
</labwc_config>
RCEOF

cat > config/includes.chroot/etc/skel/.config/labwc/autostart << 'AUTOEOF'
# AiOS autostart
/opt/aios-app/aios-session.sh &
AUTOEOF
chmod +x config/includes.chroot/etc/skel/.config/labwc/autostart

cat > config/includes.chroot/etc/skel/.config/labwc/environment << 'ENVEOF'
XDG_CURRENT_DESKTOP=wlroots
MOZ_ENABLE_WAYLAND=1
QT_QPA_PLATFORM=wayland
GDK_BACKEND=wayland
ENVEOF

# Desktop file for AiOS
mkdir -p config/includes.chroot/usr/share/applications
cat > config/includes.chroot/usr/share/applications/aios.desktop << 'DSKEOF'
[Desktop Entry]
Type=Application
Name=AiOS
Comment=AI-Native Operating System Interface
Exec=python3 -m aios
Icon=system-help
Terminal=false
Categories=Utility;
DSKEOF

# Simple wallpaper (generate a dark gradient placeholder)
mkdir -p config/includes.chroot/usr/share/aios

# MOTD / Login banner
mkdir -p config/includes.chroot/etc
cat > config/includes.chroot/etc/motd << 'MOTDEOF'

    ╔═══════════════════════════════════════╗
    ║           AiOS Linux v2.0             ║
    ║   AI-Native Operating System          ║
    ║                                       ║
    ║   Speak or type to interact with AI   ║
    ╚═══════════════════════════════════════╝

MOTDEOF

# ─── Build ────────────────────────────────────────────────────────

echo "[*] Building AiOS ISO..."
echo "    This may take 15-30 minutes on first run."
echo ""

sudo lb build 2>&1 | tee build.log

# Check if ISO was created
ISO=$(find . -name "*.iso" -type f | head -1)
if [ -n "${ISO}" ]; then
    echo ""
    echo "========================================"
    echo "  Build complete!"
    echo "  ISO: ${BUILD_DIR}/${ISO}"
    SIZE=$(du -h "${ISO}" | cut -f1)
    echo "  Size: ${SIZE}"
    echo ""
    echo "  Boot in QEMU:"
    echo "    qemu-system-x86_64 -enable-kvm -m 4G -cdrom ${ISO}"
    echo ""
    echo "  Write to USB:"
    echo "    sudo dd if=${ISO} of=/dev/sdX bs=4M status=progress"
    echo "========================================"
else
    echo ""
    echo "ERROR: ISO not found. Check build.log for errors."
    exit 1
fi
