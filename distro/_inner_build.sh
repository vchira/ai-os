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
avahi-daemon
libnss-mdns
pipewire
pipewire-alsa
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
openssh-server
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

# ── Set hostname to 'aios' (makes it reachable as aios.local via mDNS) ──
echo "aios" > /etc/hostname
echo "127.0.0.1 aios" >> /etc/hosts

# ── Keyboard layout (from .env or default to us) ──
KB_LAYOUT="us"
if [ -f /work/.env ]; then
    KB_LAYOUT=$(grep -oP 'KEYBOARD_LAYOUT\s*=\s*\K\S+' /work/.env 2>/dev/null || echo "us")
fi
[ -z "$KB_LAYOUT" ] && KB_LAYOUT="us"
echo "[AiOS] Keyboard layout: ${KB_LAYOUT}"

# Console keymap (de -> de-latin1, us -> us, etc.)
KB_CONSOLE="${KB_LAYOUT}"
[ "$KB_LAYOUT" = "de" ] && KB_CONSOLE="de-latin1"
[ "$KB_LAYOUT" = "fr" ] && KB_CONSOLE="fr-latin1"

echo "KEYMAP=${KB_CONSOLE}" > /etc/vconsole.conf
mkdir -p /etc/default
cat > /etc/default/keyboard << KBEOF
XKBMODEL="pc105"
XKBLAYOUT="${KB_LAYOUT}"
XKBVARIANT=""
XKBOPTIONS=""
KBEOF
localectl set-keymap "${KB_CONSOLE}" 2>/dev/null || true
localectl set-x11-keymap "${KB_LAYOUT}" 2>/dev/null || true
loadkeys "${KB_CONSOLE}" 2>/dev/null || true

# ── Avahi mDNS — makes http://aios.local work on the LAN ──
systemctl enable avahi-daemon 2>/dev/null || true
mkdir -p /etc/avahi/services
cat > /etc/avahi/services/aios-web.service << AVEOF
<?xml version="1.0" standalone='no'?>
<!DOCTYPE service-group SYSTEM "avahi-service.dtd">
<service-group>
  <name>AiOS Web Interface</name>
  <service>
    <type>_http._tcp</type>
    <port>80</port>
  </service>
</service-group>
AVEOF

# ── SPICE agent for clipboard sharing with host ──
# spice-vdagentd must run as a system service BEFORE the user session starts.
systemctl enable spice-vdagentd 2>/dev/null || true

# ── SSH server — enable for remote access + updates ──
systemctl enable ssh 2>/dev/null || true
# Allow password auth for initial setup (user can add keys later)
sed -i 's/^#PasswordAuthentication yes/PasswordAuthentication yes/' /etc/ssh/sshd_config 2>/dev/null || true

# ── Avahi SSH service advertisement ──
cat > /etc/avahi/services/aios-ssh.service << SSHEOF
<?xml version="1.0" standalone='no'?>
<!DOCTYPE service-group SYSTEM "avahi-service.dtd">
<service-group>
  <name>AiOS SSH</name>
  <service>
    <type>_ssh._tcp</type>
    <port>22</port>
  </service>
</service-group>
SSHEOF

# ── Install AiOS binary ──
if [ -f /opt/aios-app/aios ]; then
    cp /opt/aios-app/aios /usr/bin/aios
    chmod +x /usr/bin/aios
    echo "[AiOS] Binary installed to /usr/bin/aios"
else
    echo "WARN: AiOS binary not found at /opt/aios-app/aios"
fi

# ── AiOS update script (for remote push updates) ──
cat > /usr/bin/aios-update << 'UPDEOF'
#!/bin/bash
# AiOS Remote Update — replaces the binary and restarts the app.
#
# Usage from dev machine:
#   scp target/release/aios aios@aios.local:/tmp/aios-new && \
#   ssh aios@aios.local aios-update /tmp/aios-new
#
# Or with a URL:
#   ssh aios@aios.local aios-update https://example.com/aios
#
# Or from the AiOS chat:
#   /update <url>

set -euo pipefail

SRC="${1:-}"
DEST="/usr/bin/aios"
SERVICE="aios"

if [ -z "${SRC}" ]; then
    echo "Usage: aios-update <path-or-url>"
    echo ""
    echo "Examples:"
    echo "  aios-update /tmp/aios-new      # from local file"
    echo "  aios-update https://...         # from URL"
    exit 1
fi

echo "[AiOS Update] Starting..."

# Download or copy the new binary
if [[ "${SRC}" == http* ]]; then
    echo "[*] Downloading from ${SRC}..."
    curl -fSL -o /tmp/aios-update-new "${SRC}"
    SRC="/tmp/aios-update-new"
fi

if [ ! -f "${SRC}" ]; then
    echo "ERROR: File not found: ${SRC}"
    exit 1
fi

# Verify it's an ELF binary
if ! file "${SRC}" | grep -q "ELF.*executable"; then
    echo "ERROR: Not a valid ELF binary: ${SRC}"
    exit 1
fi

# Backup current binary
if [ -f "${DEST}" ]; then
    cp "${DEST}" "${DEST}.bak"
    echo "[*] Backed up current binary to ${DEST}.bak"
fi

# Replace
cp "${SRC}" "${DEST}"
chmod +x "${DEST}"
echo "[*] Binary replaced: $(du -h "${DEST}" | cut -f1)"

# Restart the AiOS app (if running as a systemd service)
if systemctl is-active --quiet "${SERVICE}" 2>/dev/null; then
    echo "[*] Restarting ${SERVICE}..."
    systemctl restart "${SERVICE}"
    echo "[*] Service restarted"
else
    echo "[*] No systemd service found. Kill and restart manually:"
    echo "    pkill -f /usr/bin/aios; /usr/bin/aios &"
    # Try to restart anyway
    pkill -f "/usr/bin/aios" 2>/dev/null || true
    sleep 1
    nohup /usr/bin/aios &>/dev/null &
    echo "[*] AiOS restarted (PID: $!)"
fi

echo "[AiOS Update] Done!"
UPDEOF
chmod +x /usr/bin/aios-update

# ── Install whisper.cpp for STT ──
# Download pre-built whisper-cpp binary and the tiny.en model for fast local STT.
echo "[AiOS] Installing whisper.cpp for STT..."
WHISPER_VERSION="1.7.4"
WHISPER_URL="https://github.com/ggerganov/whisper.cpp/releases/download/v${WHISPER_VERSION}/whisper-cli-linux-x86_64.tar.gz"
if curl -fsSL "${WHISPER_URL}" -o /tmp/whisper.tar.gz 2>/dev/null; then
    tar xzf /tmp/whisper.tar.gz -C /tmp/
    cp /tmp/whisper-cli /usr/bin/whisper-cpp-cli 2>/dev/null || \
    cp /tmp/whisper.cpp/main /usr/bin/whisper-cpp-cli 2>/dev/null || \
    find /tmp -name 'main' -o -name 'whisper-cli' 2>/dev/null | head -1 | xargs -I{} cp {} /usr/bin/whisper-cpp-cli
    chmod +x /usr/bin/whisper-cpp-cli 2>/dev/null || true
    rm -rf /tmp/whisper*
    echo "[AiOS] whisper-cpp-cli installed"
else
    echo "WARN: Failed to download whisper.cpp — STT will not work"
fi

# Download whisper tiny model (75MB, good for real-time on most hardware)
WHISPER_MODEL_DIR="/home/aios/.aios/models/whisper"
mkdir -p "${WHISPER_MODEL_DIR}"
WHISPER_MODEL_URL="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin"
if [ ! -f "${WHISPER_MODEL_DIR}/ggml-tiny.bin" ]; then
    echo "[AiOS] Downloading whisper tiny model..."
    curl -fsSL "${WHISPER_MODEL_URL}" -o "${WHISPER_MODEL_DIR}/ggml-tiny.bin" 2>/dev/null || \
        echo "WARN: Failed to download whisper model"
fi

# ── Install Piper TTS for high-quality voices ──
echo "[AiOS] Installing Piper TTS..."
PIPER_VERSION="2023.11.14-2"
PIPER_URL="https://github.com/rhasspy/piper/releases/download/${PIPER_VERSION}/piper_linux_x86_64.tar.gz"
if curl -fsSL "${PIPER_URL}" -o /tmp/piper.tar.gz 2>/dev/null; then
    tar xzf /tmp/piper.tar.gz -C /opt/
    ln -sf /opt/piper/piper /usr/bin/piper
    rm -f /tmp/piper.tar.gz
    echo "[AiOS] Piper TTS installed"
else
    echo "WARN: Failed to download Piper — using espeak-ng fallback"
fi

# Download a good English voice for Piper (Amy - medium quality, natural sounding)
PIPER_VOICE_DIR="/home/aios/.aios/models/piper"
mkdir -p "${PIPER_VOICE_DIR}"
VOICE_URL="https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/amy/medium"
if [ ! -f "${PIPER_VOICE_DIR}/en_US-amy-medium.onnx" ]; then
    echo "[AiOS] Downloading Piper voice (en_US-amy-medium)..."
    curl -fsSL "${VOICE_URL}/en_US-amy-medium.onnx" -o "${PIPER_VOICE_DIR}/en_US-amy-medium.onnx" 2>/dev/null || true
    curl -fsSL "${VOICE_URL}/en_US-amy-medium.onnx.json" -o "${PIPER_VOICE_DIR}/en_US-amy-medium.onnx.json" 2>/dev/null || true
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

# Ensure XDG_RUNTIME_DIR is set (required for PipeWire socket)
if [ -z "${XDG_RUNTIME_DIR}" ]; then
    export XDG_RUNTIME_DIR="/run/user/$(id -u)"
    mkdir -p "${XDG_RUNTIME_DIR}"
    chmod 700 "${XDG_RUNTIME_DIR}"
    echo "Set XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR}"
fi

# Start spice-vdagent for clipboard sharing with host
# (spice-vdagentd system daemon is started via systemd)
spice-vdagent 2>/dev/null &
sleep 0.2

# Ensure PipeWire is running for audio
if ! pgrep -x pipewire >/dev/null; then
    pipewire &
    sleep 0.5
    pipewire-pulse &
    sleep 0.3
    wireplumber &
    echo "Started PipeWire audio stack"
    sleep 1
fi

# Unmute audio and set volume (try both ALSA and PipeWire/pactl)
amixer -q set Master 80% unmute 2>/dev/null || true
pactl set-sink-mute @DEFAULT_SINK@ 0 2>/dev/null || true
pactl set-sink-volume @DEFAULT_SINK@ 80% 2>/dev/null || true

# Log audio device status for debugging
echo "Audio devices:"
aplay -l 2>&1 || echo "  aplay: no devices"
echo "PipeWire status:"
pw-cli ls Node 2>&1 | head -20 || echo "  pw-cli: not available"

# Launch AiOS
/opt/aios-app/aios-session.sh &
ASEOF
chmod +x /home/aios/.config/labwc/autostart

cat > /home/aios/.config/labwc/environment << ENVEOF
XDG_CURRENT_DESKTOP=wlroots
MOZ_ENABLE_WAYLAND=1
QT_QPA_PLATFORM=wayland
GDK_BACKEND=wayland
XCURSOR_THEME=Adwaita
XCURSOR_SIZE=24
WLR_NO_HARDWARE_CURSORS=1
PIPEWIRE_RUNTIME_DIR=/run/user/1000
XKB_DEFAULT_LAYOUT=${KB_LAYOUT}
ENVEOF

cat > /home/aios/.config/labwc/rc.xml << RCEOF
<?xml version="1.0"?>
<labwc_config>
  <core><gap>0</gap></core>
  <theme>
    <name>AiOS</name>
    <cornerRadius>0</cornerRadius>
    <font name="sans" size="11"/>
  </theme>
  <keyboard>
    <default layout="${KB_LAYOUT}"/>
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

# ─── Boot Menu Branding (syslinux/isolinux for BIOS boot) ────

# live-build uses config/bootloaders/isolinux/ to override the default
# boot menu. We must provide ALL the files it expects, otherwise it
# falls back to the Debian defaults.

echo "[*] Creating custom boot menu..."

# Copy the stock syslinux/isolinux modules from live-build as a base,
# then override with our branding.
mkdir -p config/bootloaders/isolinux

# Create the main isolinux config that live-build will use.
# This replaces the auto-generated menu entirely.
cat > config/bootloaders/isolinux/isolinux.cfg << 'BOOTEOF'
ui vesamenu.c32
timeout 50
prompt 0

menu title AiOS — AI-Native Operating System
menu background splash.png

menu color screen  0;37;40 #00000000 #00000000 none
menu color border  0;30;40 #00000000 #00000000 none
menu color title   1;37;40 #ff4a9eff #00000000 none
menu color sel     7;37;40 #ffffffff #ff1f2b47 none
menu color unsel   0;37;40 #ffaaaaaa #00000000 none
menu color hotkey  0;37;40 #ff4a9eff #00000000 none
menu color tabmsg  0;37;40 #ff666666 #00000000 none
menu color help    0;37;40 #ff666666 #00000000 none
menu color cmdline 0;37;40 #ff999999 #00000000 none

menu vshift 10
menu hshift 6
menu width 60
menu margin 8
menu rows 5
menu tabmsgrow 18
menu cmdlinerow 19
menu timeoutrow 20
menu tabmsg Press TAB to edit boot options

label live-aios
    menu label ^AiOS
    menu default
    kernel /live/vmlinuz
    append initrd=/live/initrd.img boot=live components username=aios quiet splash

label live-safe
    menu label AiOS ^Safe Mode
    kernel /live/vmlinuz
    append initrd=/live/initrd.img boot=live components username=aios single nomodeset

label live-debug
    menu label AiOS ^Debug Mode
    kernel /live/vmlinuz
    append initrd=/live/initrd.img boot=live components username=aios debug
BOOTEOF

# Also provide live.cfg (some live-build versions include this)
cp config/bootloaders/isolinux/isolinux.cfg config/bootloaders/isolinux/live.cfg

# Generate a branded splash image (640x480 PNG — syslinux requirement)
python3 -c "
from PIL import Image, ImageDraw, ImageFont

W, H = 640, 480
bg = (18, 18, 36)       # Dark navy
accent = (74, 158, 255) # Blue accent
white = (255, 255, 255)
dim = (130, 140, 160)

img = Image.new('RGB', (W, H), bg)
draw = ImageDraw.Draw(img)

# Subtle gradient-ish top bar
for y in range(0, 4):
    draw.line([(0, y), (W, y)], fill=accent)

try:
    font_title = ImageFont.truetype('/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf', 48)
    font_sub = ImageFont.truetype('/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf', 18)
    font_ver = ImageFont.truetype('/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf', 14)
except:
    font_title = ImageFont.load_default()
    font_sub = font_title
    font_ver = font_title

# Title
draw.text((W//2, 100), 'AiOS', fill=white, font=font_title, anchor='mm')

# Subtitle
draw.text((W//2, 150), 'AI-Native Operating System', fill=dim, font=font_sub, anchor='mm')

# Version
draw.text((W//2, 180), 'v2.0', fill=(80, 90, 110), font=font_ver, anchor='mm')

# Bottom accent line
for y in range(H-3, H):
    draw.line([(0, y), (W, y)], fill=(40, 50, 70))

img.save('config/bootloaders/isolinux/splash.png')
print('[*] AiOS boot splash generated')
" 2>/dev/null || echo "[*] Splash generation skipped (no PIL — install python3-pil)"

# Also copy splash to syslinux dir as fallback
mkdir -p config/bootloaders/syslinux
cp config/bootloaders/isolinux/isolinux.cfg config/bootloaders/syslinux/syslinux.cfg 2>/dev/null || true
cp config/bootloaders/isolinux/splash.png config/bootloaders/syslinux/splash.png 2>/dev/null || true

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
