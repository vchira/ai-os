#!/bin/bash
set -euo pipefail

BUILD_DIR="/work/distro/build"
AIOS_APP_DIR="/work/aios-app"
MIRROR="http://ftp.de.debian.org/debian"
MIRROR_SEC="http://security.debian.org/debian-security"

mkdir -p "${BUILD_DIR}"
cd "${BUILD_DIR}"

# Restore cached packages
mkdir -p cache/packages.chroot
if [ -d /cache/packages.chroot ] && [ "$(ls -A /cache/packages.chroot 2>/dev/null)" ]; then
    cp -a /cache/packages.chroot/* cache/packages.chroot/ 2>/dev/null || true
fi

# Force full chroot rebuild if our marker is missing
if [ -d chroot ] && [ ! -f chroot/etc/aios-build-marker ]; then
    echo "[*] Chroot is stale — forcing full rebuild..."
    lb clean --chroot 2>/dev/null || true
    lb clean --binary 2>/dev/null || true
    rm -f config/common 2>/dev/null || true
fi

# Configure (skip if already done)
if [ ! -f config/common ]; then
    echo "[*] Configuring live-build..."

    lb config \
        --mode debian \
        --distribution bookworm \
        --parent-distribution bookworm \
        --mirror-bootstrap "${MIRROR}" \
        --mirror-chroot "${MIRROR}" \
        --mirror-binary "${MIRROR}" \
        --mirror-chroot-security "${MIRROR_SEC}" \
        --mirror-binary-security "${MIRROR_SEC}" \
        --archive-areas "main contrib non-free non-free-firmware" \
        --architectures amd64 \
        --binary-images iso-hybrid \
        --debian-installer none \
        --firmware-binary true \
        --firmware-chroot true \
        --iso-application "AiOS" \
        --iso-publisher "AiOS Project" \
        --iso-volume "AiOS Live" \
        --memtest none \
        --apt-indices false \
        --cache true \
        --linux-flavours "amd64" \
        --backports false \
        --bootappend-live "boot=live components username=aios"
fi

# ─── Package Lists ──────────────────────────────────────────
echo "[*] Setting up package lists..."

cat > config/package-lists/base.list.chroot << 'EOF'
linux-headers-amd64
firmware-linux
firmware-misc-nonfree
firmware-iwlwifi
firmware-realtek
firmware-atheros
firmware-intel-sound
sudo
systemd
dbus
polkitd
udev
network-manager
bluez
pipewire
pipewire-pulse
wireplumber
libspa-0.2-bluetooth
alsa-utils
pulseaudio-utils
ca-certificates
EOF

cat > config/package-lists/desktop.list.chroot << 'EOF'
xwayland
swaybg
grim
slurp
wl-clipboard
foot
thunar
wlr-randr
libgtk-4-1
libadwaita-1-0
gir1.2-gtk-4.0
gir1.2-adw-1
adwaita-icon-theme
xcursor-themes
spice-vdagent
greetd
EOF

cat > config/package-lists/aios.list.chroot << 'EOF'
libsndfile1
curl
wget
git
build-essential
espeak-ng
meson
ninja-build
libwlroots-dev
libwayland-dev
wayland-protocols
libxml2-dev
libcairo2-dev
libpango1.0-dev
libxkbcommon-dev
libinput-dev
libdrm-dev
libgbm-dev
scdoc
hwdata
libseat-dev
EOF

# ─── Hooks ──────────────────────────────────────────────────
echo "[*] Setting up hooks..."
mkdir -p config/hooks/live

# Hook 1: Build labwc from source
cat > config/hooks/live/0050-build-labwc.hook.chroot << 'EOF'
#!/bin/bash
set -e
echo "[AiOS] Building labwc from source..."
cd /tmp
git clone --depth 1 --branch 0.6.6 https://github.com/labwc/labwc.git
cd labwc
meson setup build -Dprefix=/usr
ninja -C build
ninja -C build install
cd /
rm -rf /tmp/labwc
echo "[AiOS] labwc installed successfully"
EOF
chmod +x config/hooks/live/0050-build-labwc.hook.chroot

# Hook 2: Setup AiOS system
cat > config/hooks/live/0100-setup-aios.hook.chroot << 'EOF'
#!/bin/bash
set -e
echo "[AiOS] Setting up AiOS distribution..."

# ── Create user ──
useradd -m -G sudo,audio,video,input,render -s /bin/bash aios 2>/dev/null || true
echo "aios:aios" | chpasswd
echo "aios ALL=(ALL) NOPASSWD:ALL" > /etc/sudoers.d/aios

# ── Install AiOS binary ──
if [ -f /opt/aios-app/aios ]; then
    cp /opt/aios-app/aios /usr/bin/aios
    chmod +x /usr/bin/aios
    echo "[AiOS] Binary installed to /usr/bin/aios"
else
    echo "WARN: AiOS binary not found at /opt/aios-app/aios"
fi

# ── AiOS config (read API keys from .env if available) ──
CLAUDE_KEY=""
OPENAI_KEY=""
if [ -f /work/.env ]; then
    CLAUDE_KEY=$(grep -oP 'CLAUDE_API_KEY\s*=\s*\K\S+' /work/.env 2>/dev/null || true)
    OPENAI_KEY=$(grep -oP 'OPENAI_API_KEY\s*=\s*\K\S+' /work/.env 2>/dev/null || true)
    [ "$OPENAI_KEY" = "your-api-key-here" ] && OPENAI_KEY=""
fi
mkdir -p /home/aios/.aios/{models/whisper,models/piper,plugins,memory}
cat > /home/aios/.aios/config.json << CFGEOF
{
  "llm": {
    "provider": "claude",
    "claude_api_key": "${CLAUDE_KEY}",
    "claude_model": "claude-sonnet-4-20250514",
    "openai_api_key": "${OPENAI_KEY}",
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
  "ui": { "theme": "dark" },
  "system": { "keyboard_layout": "us" }
}
CFGEOF

# ── greetd: display manager that starts labwc ──
mkdir -p /etc/greetd
cat > /etc/greetd/config.toml << 'GREETEOF'
[terminal]
vt = 7

[default_session]
command = "labwc"
user = "aios"
GREETEOF
systemctl enable greetd

# ── labwc config for aios user ──
mkdir -p /home/aios/.config/labwc

cat > /home/aios/.config/labwc/autostart << 'ASEOF'
#!/bin/bash
exec >> /tmp/aios-boot.log 2>&1
echo "=== labwc autostart $(date) ==="
echo "User: $(whoami)"
echo "WAYLAND_DISPLAY: ${WAYLAND_DISPLAY:-unset}"
echo "XDG_RUNTIME_DIR: ${XDG_RUNTIME_DIR:-unset}"

# Start spice-vdagent for clipboard sharing with host
spice-vdagent 2>/dev/null &

# Ensure PipeWire is running for audio
if ! pgrep -x pipewire >/dev/null; then
    pipewire &
    pipewire-pulse &
    wireplumber &
    echo "Started PipeWire audio"
fi

# Unmute audio and set volume
amixer -q set Master 80% unmute 2>/dev/null || true

# Launch AiOS
/opt/aios-app/aios-session.sh &
ASEOF
chmod +x /home/aios/.config/labwc/autostart

cat > /home/aios/.config/labwc/environment << 'ENVEOF'
XDG_CURRENT_DESKTOP=wlroots
MOZ_ENABLE_WAYLAND=1
QT_QPA_PLATFORM=wayland
GDK_BACKEND=wayland
XCURSOR_THEME=Adwaita
XCURSOR_SIZE=24
WLR_NO_HARDWARE_CURSORS=1
ENVEOF

cat > /home/aios/.config/labwc/rc.xml << 'RCEOF'
<?xml version="1.0"?>
<labwc_config>
  <core><gap>0</gap></core>
  <theme>
    <name>AiOS</name>
    <cornerRadius>0</cornerRadius>
    <font name="sans" size="11"/>
  </theme>
  <keyboard>
    <keybind key="A-F4"><action name="Close"/></keybind>
    <keybind key="A-Return"><action name="Execute"><command>foot</command></action></keybind>
    <keybind key="A-F11"><action name="ToggleFullscreen"/></keybind>
    <keybind key="Print"><action name="Execute"><command>grim</command></action></keybind>
    <!-- Super key or Ctrl+Space opens AiOS -->
    <keybind key="Super_L"><action name="Execute"><command>/usr/bin/aios</command></action></keybind>
    <keybind key="C-space"><action name="Execute"><command>/usr/bin/aios</command></action></keybind>
  </keyboard>
  <mouse>
    <context name="TitleBar">
      <mousebind button="Left" action="Drag"><action name="Move"/></mousebind>
      <mousebind button="Left" action="DoubleClick"><action name="ToggleMaximize"/></mousebind>
    </context>
    <context name="Frame">
      <mousebind button="A-Left" action="Drag"><action name="Move"/></mousebind>
      <mousebind button="A-Right" action="Drag"><action name="Resize"/></mousebind>
    </context>
    <context name="Top"><mousebind button="Left" action="Drag"><action name="Resize"/></mousebind></context>
    <context name="Bottom"><mousebind button="Left" action="Drag"><action name="Resize"/></mousebind></context>
    <context name="Left"><mousebind button="Left" action="Drag"><action name="Resize"/></mousebind></context>
    <context name="Right"><mousebind button="Left" action="Drag"><action name="Resize"/></mousebind></context>
    <context name="TLCorner"><mousebind button="Left" action="Drag"><action name="Resize"/></mousebind></context>
    <context name="TRCorner"><mousebind button="Left" action="Drag"><action name="Resize"/></mousebind></context>
    <context name="BLCorner"><mousebind button="Left" action="Drag"><action name="Resize"/></mousebind></context>
    <context name="BRCorner"><mousebind button="Left" action="Drag"><action name="Resize"/></mousebind></context>
    <!-- No desktop right-click menu — AiOS is the desktop -->
    <context name="Root"/>
  </mouse>
</labwc_config>
RCEOF

chown -R aios:aios /home/aios

# ── Enable PipeWire ──
systemctl --global enable pipewire pipewire-pulse wireplumber 2>/dev/null || true

# ── Build marker ──
date > /etc/aios-build-marker

echo "[AiOS] Setup complete. Build marker written."
EOF
chmod +x config/hooks/live/0100-setup-aios.hook.chroot

# ─── Chroot Includes ──────────────────────────────────────────
echo "[*] Setting up chroot includes..."

# Copy the pre-built Rust binary into the ISO
mkdir -p config/includes.chroot/opt/aios-app
if [ -f /work/aios-app-rs/target/release/aios ]; then
    cp /work/aios-app-rs/target/release/aios config/includes.chroot/opt/aios-app/aios
    chmod +x config/includes.chroot/opt/aios-app/aios
    echo "[*] Rust binary copied into ISO"
else
    echo "ERROR: Rust binary not found. Build with: cd aios-app-rs && cargo build --release"
    exit 1
fi

# Session script with full logging
cat > config/includes.chroot/opt/aios-app/aios-session.sh << 'EOF'
#!/bin/bash
LOGFILE="/tmp/aios-boot.log"
log() { echo "[$(date +%H:%M:%S)] [session] $*" >> "$LOGFILE" 2>&1; }

log "=== AiOS Session Starting ==="
log "WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-unset}"
log "XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR:-unset}"
log "HOME=$HOME"

# Dark background
swaybg -m solid_color -c '#1a1a2e' >> "$LOGFILE" 2>&1 &

# Desktop hint — shown when AiOS window is closed
# Creates a small foot terminal with the hint text
show_hint() {
    foot --title="AiOS Hint" -W 60x3 -- bash -c '
        echo ""
        echo "  Press Super or Ctrl+Space to open AiOS"
        echo ""
        read -r
    ' >> "$LOGFILE" 2>&1 &
}

# Launch AiOS in a loop — restart if user closes the window
while true; do
    log "Launching /usr/bin/aios..."
    /usr/bin/aios >> "$LOGFILE" 2>&1
    EXIT_CODE=$?
    log "AiOS exited with code $EXIT_CODE"

    if [ $EXIT_CODE -ne 0 ] && [ $EXIT_CODE -ne 0 ]; then
        # Show error terminal on crash
        log "AiOS crashed, showing debug terminal..."
        foot -e bash -c "cat /tmp/aios-boot.log; echo; echo 'AiOS crashed (exit $EXIT_CODE). Press Enter to restart...'; read" 2>/dev/null
    else
        # Normal close — show hint and wait for relaunch
        log "AiOS closed normally, showing hint..."
        show_hint
        # Wait until aios is launched again (via keybinding)
        while ! pgrep -x aios >/dev/null 2>&1; do
            sleep 1
        done
        # Kill the hint terminal
        pkill -f "AiOS Hint" 2>/dev/null
    fi
done
EOF
chmod +x config/includes.chroot/opt/aios-app/aios-session.sh

# NO system-wide labwc config — per-user only (set in hook above)
rm -rf config/includes.chroot/etc/xdg/labwc 2>/dev/null || true

# Desktop file
mkdir -p config/includes.chroot/usr/share/applications
cat > config/includes.chroot/usr/share/applications/aios.desktop << 'EOF'
[Desktop Entry]
Type=Application
Name=AiOS
Comment=AI-Native Operating System Interface
Exec=python3 -m aios
Icon=system-help
Terminal=false
Categories=Utility;
EOF

mkdir -p config/includes.chroot/usr/share/aios
mkdir -p config/includes.chroot/etc
cat > config/includes.chroot/etc/motd << 'EOF'

    ╔═══════════════════════════════════════╗
    ║           AiOS Linux v2.0             ║
    ║   AI-Native Operating System          ║
    ║                                       ║
    ║   Speak or type to interact with AI   ║
    ╚═══════════════════════════════════════╝

EOF

# ─── Build ────────────────────────────────────────────────────

# ─── Boot Menu Branding (syslinux for BIOS boot) ────────────

# Custom syslinux splash and menu
mkdir -p config/bootloaders/syslinux

cat > config/bootloaders/syslinux/live.cfg.in << 'EOF'
ui vesamenu.c32

menu title AiOS — AI-Native Operating System
menu background splash.png
menu color title  1;37;40 #ffffffff #00000000
menu color border 0;30;40 #00000000 #00000000
menu color sel    7;37;40 #ff00aaff #33000000
menu color unsel  0;37;40 #ffcccccc #00000000
menu color hotkey 0;37;40 #ff00aaff #00000000
menu color tabmsg 0;37;40 #ff999999 #00000000
menu color help   0;37;40 #ff999999 #00000000
menu vshift 12
menu hshift 0
menu width 50
menu margin 10
menu rows 5
menu tabmsgrow 18
menu cmdlinerow 18
menu timeoutrow 20
menu tabmsg Press TAB to edit boot options

timeout 50

label live-aios
    menu label AiOS (normal boot, straight to desktop)
    menu default
    kernel /live/vmlinuz
    append initrd=/live/initrd.img boot=live components username=aios quiet splash

label live-safe
    menu label AiOS Safe Mode (text-only root shell, no graphics)
    kernel /live/vmlinuz
    append initrd=/live/initrd.img boot=live components username=aios single

label live-debug
    menu label AiOS Debug Mode (normal boot with all system messages visible)
    kernel /live/vmlinuz
    append initrd=/live/initrd.img boot=live components username=aios debug
EOF

# Generate a simple branded splash image using Python (available in the builder)
# 640x480 PNG with dark background and AiOS text
python3 -c "
from PIL import Image, ImageDraw, ImageFont
import os
img = Image.new('RGB', (640, 480), (26, 26, 46))
draw = ImageDraw.Draw(img)
# Try to use a nice font, fall back to default
try:
    font_large = ImageFont.truetype('/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf', 36)
    font_small = ImageFont.truetype('/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf', 16)
except:
    font_large = ImageFont.load_default()
    font_small = ImageFont.load_default()
draw.text((640//2, 160), 'AiOS', fill=(255, 255, 255), font=font_large, anchor='mm')
draw.text((640//2, 210), 'AI-Native Operating System', fill=(180, 180, 200), font=font_small, anchor='mm')
draw.text((640//2, 250), 'v2.0', fill=(120, 120, 150), font=font_small, anchor='mm')
img.save('config/bootloaders/syslinux/splash.png')
print('[*] Boot splash generated')
" 2>/dev/null || echo "[*] Splash generation skipped (no PIL)"

rm -f *.iso 2>/dev/null || true

echo "[*] Building AiOS ISO..."
lb build 2>&1 | tee build.log

# Save cache
mkdir -p /cache/packages.chroot
cp -a cache/packages.chroot/* /cache/packages.chroot/ 2>/dev/null || true

ISO=$(find . -maxdepth 1 -name "*.iso" -type f | head -1)
if [ -n "${ISO}" ]; then
    echo ""
    echo "========================================"
    echo "  Build complete!"
    echo "  ISO: /work/distro/build/${ISO}"
    SIZE=$(du -h "${ISO}" | cut -f1)
    echo "  Size: ${SIZE}"
    echo "========================================"
else
    echo "ERROR: ISO not found. Check build.log"
    exit 1
fi
