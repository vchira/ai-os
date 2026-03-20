#!/bin/bash
set -euo pipefail

BUILD_DIR="/work/distro/build"
AIOS_APP_DIR="/work/aios-app"
MIRROR="http://ftp.de.debian.org/debian"
MIRROR_SEC="http://security.debian.org/debian-security"

mkdir -p "${BUILD_DIR}"
cd "${BUILD_DIR}"

# Restore cached packages — check Docker image prefetch, then volume cache
mkdir -p cache/packages.chroot
if [ -d /cache/prefetch ] && [ "$(ls -A /cache/prefetch 2>/dev/null)" ]; then
    echo "[*] Restoring pre-downloaded packages from Docker image..."
    cp -n /cache/prefetch/*.deb cache/packages.chroot/ 2>/dev/null || true
fi
if [ -d /cache/packages.chroot ] && [ "$(ls -A /cache/packages.chroot 2>/dev/null)" ]; then
    cp -n /cache/packages.chroot/*.deb cache/packages.chroot/ 2>/dev/null || true
fi

# Restore bootstrap cache from volume
if [ ! -d cache/bootstrap ] && [ -f /cache/bootstrap.tar ]; then
    echo "[*] Restoring bootstrap cache from volume..."
    mkdir -p cache
    tar xf /cache/bootstrap.tar -C cache/ 2>/dev/null || true
fi

# Restore cached externals into includes.chroot/ — live-build copies these
# into the chroot BEFORE hooks run, so hooks can skip building/downloading.
if [ -d /cache/external ]; then
    echo "[*] Restoring cached externals into includes.chroot/..."
    mkdir -p config/includes.chroot
    if [ -f /cache/external/labwc ]; then
        mkdir -p config/includes.chroot/usr/bin
        cp /cache/external/labwc config/includes.chroot/usr/bin/labwc
        chmod +x config/includes.chroot/usr/bin/labwc
        if [ -f /cache/external/labwc-share.tar ]; then
            mkdir -p config/includes.chroot/usr/share
            tar xf /cache/external/labwc-share.tar -C config/includes.chroot/usr/share 2>/dev/null || true
        fi
    fi
    if [ -f /cache/external/whisper-cpp-cli ]; then
        mkdir -p config/includes.chroot/usr/bin
        cp /cache/external/whisper-cpp-cli config/includes.chroot/usr/bin/whisper-cpp-cli
        chmod +x config/includes.chroot/usr/bin/whisper-cpp-cli
    fi
    if [ -f /cache/external/ggml-tiny.bin ]; then
        mkdir -p config/includes.chroot/home/aios/.aios/models/whisper
        cp /cache/external/ggml-tiny.bin config/includes.chroot/home/aios/.aios/models/whisper/
    fi
    if [ -f /cache/external/piper.tar ]; then
        mkdir -p config/includes.chroot/opt
        tar xf /cache/external/piper.tar -C config/includes.chroot/opt 2>/dev/null || true
        mkdir -p config/includes.chroot/usr/bin
        ln -sf /opt/piper/piper config/includes.chroot/usr/bin/piper
    fi
    if [ -f /cache/external/piper-voices.tar ]; then
        mkdir -p config/includes.chroot/home/aios/.aios/models
        tar xf /cache/external/piper-voices.tar -C config/includes.chroot/home/aios/.aios/models 2>/dev/null || true
    fi
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
        --bootappend-live "boot=live components username=aios noeject"
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
libcap2-bin
bc
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
squashfs-tools
sqlite3
gdisk
dosfstools
file
grub-efi-amd64-bin
grub-pc-bin
EOF

# ─── Hooks ──────────────────────────────────────────────────
echo "[*] Setting up hooks..."
mkdir -p config/hooks/live

# Hook 1: Build labwc from source (skip if already installed)
cat > config/hooks/live/0050-build-labwc.hook.chroot << 'EOF'
#!/bin/bash
set -e
if command -v labwc &>/dev/null && labwc --version 2>&1 | grep -q "0.6.6"; then
    echo "[AiOS] labwc 0.6.6 already installed — skipping build"
    exit 0
fi
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

# Pre-compute values from autoconfig.json (preferred) or .env (legacy)
_KB_LAYOUT="us"
_CLAUDE_KEY=""
_OPENAI_KEY=""
_AIOS_VERSION="${AIOS_VERSION:-dev}"
_HAS_AUTOCONFIG="false"

if [ -f /work/autoconfig.json ]; then
    echo "[*] Reading from autoconfig.json..."
    _KB_LAYOUT=$(python3 -c "import json; c=json.load(open('/work/autoconfig.json')); print(c.get('system',{}).get('keyboard','us'))" 2>/dev/null || echo "us")
    _CLAUDE_KEY=$(python3 -c "import json; c=json.load(open('/work/autoconfig.json')); print(c.get('provider',{}).get('claude_api_key',''))" 2>/dev/null || true)
    _OPENAI_KEY=$(python3 -c "import json; c=json.load(open('/work/autoconfig.json')); print(c.get('provider',{}).get('openai_api_key',''))" 2>/dev/null || true)
    _HAS_AUTOCONFIG="true"
elif [ -f /work/.env ]; then
    echo "[*] Reading from .env (legacy)..."
    _KB_LAYOUT=$(grep -oP 'KEYBOARD_LAYOUT\s*=\s*\K\S+' /work/.env 2>/dev/null || echo "us")
    _CLAUDE_KEY=$(grep -oP 'CLAUDE_API_KEY\s*=\s*\K\S+' /work/.env 2>/dev/null || true)
    _OPENAI_KEY=$(grep -oP 'OPENAI_API_KEY\s*=\s*\K\S+' /work/.env 2>/dev/null || true)
    [ "$_OPENAI_KEY" = "your-api-key-here" ] && _OPENAI_KEY=""
fi
[ -z "$_KB_LAYOUT" ] && _KB_LAYOUT="us"
echo "[*] Build config: keyboard=${_KB_LAYOUT} version=${_AIOS_VERSION} autoconfig=${_HAS_AUTOCONFIG} claude_key=$([ -n "$_CLAUDE_KEY" ] && echo 'set' || echo 'empty')"

# Write build config where the chroot can read it
mkdir -p config/includes.chroot/opt/aios-app
cat > config/includes.chroot/opt/aios-app/build-config << BUILDCFG
KB_LAYOUT=${_KB_LAYOUT}
CLAUDE_KEY=${_CLAUDE_KEY}
OPENAI_KEY=${_OPENAI_KEY}
AIOS_VERSION=${_AIOS_VERSION}
BUILDCFG

# Copy autoconfig.json into ISO if it exists (for unattended setup)
if [ -f /work/autoconfig.json ]; then
    cp /work/autoconfig.json config/includes.chroot/opt/aios-app/autoconfig.json
    echo "[*] Autoconfig baked into ISO"
fi

# Hook 2: Setup AiOS system
cat > config/hooks/live/0100-setup-aios.hook.chroot << 'EOF'
#!/bin/bash
set -e

# Read build config (written by _inner_build.sh outside chroot)
if [ -f /opt/aios-app/build-config ]; then
    source /opt/aios-app/build-config
    echo "[AiOS] Build config loaded: keyboard=${KB_LAYOUT} version=${AIOS_VERSION}"
else
    echo "[AiOS] WARNING: No build config found, using defaults"
    KB_LAYOUT="us"
    CLAUDE_KEY=""
    OPENAI_KEY=""
    AIOS_VERSION="dev"
fi

echo "[AiOS] Setting up AiOS distribution..."

# ── Create user ──
useradd -m -G sudo,audio,video,input,render -s /bin/bash aios 2>/dev/null || true
echo "aios:aios" | chpasswd
echo "aios ALL=(ALL) NOPASSWD:ALL" > /etc/sudoers.d/aios

# ── Set hostname to 'assistant' (makes it reachable as assistant.local via mDNS) ──
echo "assistant" > /etc/hostname
echo "127.0.0.1 assistant" >> /etc/hosts

# ── Keyboard layout (from build config injected by _inner_build.sh) ──
# KB_LAYOUT is already set from /tmp/aios-build-config (sourced above)
[ -z "${KB_LAYOUT:-}" ] && KB_LAYOUT="us"
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

# Publish the web service so it's discoverable.
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

# Add a CNAME alias so http://aios.local always works, regardless of the
# actual hostname (which may be "assistant", user-chosen, etc.).
# Uses avahi-publish-cname via a small systemd service.
cat > /etc/systemd/system/avahi-alias-aios.service << 'ALIASEOF'
[Unit]
Description=Publish aios.local mDNS CNAME alias
After=avahi-daemon.service
Requires=avahi-daemon.service

[Service]
Type=simple
ExecStart=/usr/bin/avahi-publish -a -R aios.local $(hostname -I | awk '{print $1}')
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
ALIASEOF
systemctl enable avahi-alias-aios.service 2>/dev/null || true

# ── SPICE agent for clipboard sharing with host ──
# spice-vdagentd must run as a system service BEFORE the user session starts.
# Create the service file if it doesn't exist (some Debian versions don't include it)
if [ ! -f /etc/systemd/system/spice-vdagentd.service ] && [ ! -f /lib/systemd/system/spice-vdagentd.service ]; then
    cat > /etc/systemd/system/spice-vdagentd.service << 'SVCEOF'
[Unit]
Description=SPICE guest agent daemon
After=network.target

[Service]
Type=simple
ExecStart=/usr/sbin/spice-vdagentd -f
Restart=on-failure
RestartSec=2

[Install]
WantedBy=multi-user.target
SVCEOF
fi
# Override stock service: remove ConditionPathExists (device may not be ready at
# service start on live systems) and add restart so it retries
mkdir -p /etc/systemd/system/spice-vdagentd.service.d
cat > /etc/systemd/system/spice-vdagentd.service.d/override.conf << 'OVEOF'
[Unit]
ConditionVirtualization=
ConditionPathExists=

[Service]
Restart=on-failure
RestartSec=2
OVEOF
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
  <name>Assistant SSH</name>
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
    # Allow binding to port 80 without root
    setcap cap_net_bind_service=+ep /usr/bin/aios 2>/dev/null || true
    echo "net.ipv4.ip_unprivileged_port_start=80" > /etc/sysctl.d/99-aios-ports.conf
    # Systemd service to guarantee sysctl is applied at boot
    # (sysctl.d may not be loaded reliably on live systems)
    cat > /etc/systemd/system/aios-ports.service << 'PORTSEOF'
[Unit]
Description=Allow AiOS to bind port 80
DefaultDependencies=no
Before=greetd.service
After=systemd-sysctl.service

[Service]
Type=oneshot
ExecStart=/sbin/sysctl -w net.ipv4.ip_unprivileged_port_start=80
RemainAfterExit=yes

[Install]
WantedBy=sysinit.target
PORTSEOF
    systemctl enable aios-ports.service
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

# ── System selftest script ──
cat > /usr/bin/aios-test << 'TESTEOF'
#!/bin/bash
# AiOS System Self-Test
# Run: aios-test          (all tests)
# Run: aios-test audio    (audio only)
# Run: aios-test mic      (microphone only)
# Run: aios-test network  (network only)

set -euo pipefail

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'
BOLD='\033[1m'

PASS=0
FAIL=0
WARN=0

pass() { echo -e "  ${GREEN}✅ PASS${NC}: $1"; PASS=$((PASS+1)); }
fail() { echo -e "  ${RED}❌ FAIL${NC}: $1"; FAIL=$((FAIL+1)); }
warn() { echo -e "  ${YELLOW}⚠️  WARN${NC}: $1"; WARN=$((WARN+1)); }
header() { echo -e "\n${BOLD}${CYAN}── $1 ──${NC}"; }

FILTER="${1:-all}"

echo -e "${BOLD}╔═══════════════════════════════════════╗${NC}"
echo -e "${BOLD}║       AiOS System Self-Test           ║${NC}"
echo -e "${BOLD}╚═══════════════════════════════════════╝${NC}"
echo ""

# ── Audio Output ──
if [ "$FILTER" = "all" ] || [ "$FILTER" = "audio" ]; then
    header "Audio Output"

    if command -v aplay &>/dev/null; then
        pass "aplay available"
    else
        fail "aplay not found"
    fi

    if aplay -l 2>/dev/null | grep -q "card"; then
        CARDS=$(aplay -l 2>/dev/null | grep "^card" | head -3)
        pass "Sound card detected:"
        echo "        $CARDS"
    else
        fail "No sound card detected"
    fi

    if pgrep -x pipewire &>/dev/null; then
        pass "PipeWire running"
    else
        fail "PipeWire not running"
    fi

    if pgrep -x wireplumber &>/dev/null; then
        pass "WirePlumber running"
    else
        warn "WirePlumber not running"
    fi

    if command -v pactl &>/dev/null && pactl info &>/dev/null; then
        SINK=$(pactl info 2>/dev/null | grep "Default Sink" || echo "none")
        pass "PulseAudio/PipeWire-pulse working"
        echo "        $SINK"
    else
        warn "pactl not responding"
    fi

    if command -v piper &>/dev/null; then
        pass "Piper TTS installed"
    else
        warn "Piper TTS not installed (using espeak-ng fallback)"
    fi

    if command -v espeak-ng &>/dev/null; then
        pass "espeak-ng available"
    else
        fail "espeak-ng not found"
    fi

    if [ -f /home/aios/.aios/models/piper/en_US-amy-medium.onnx ]; then
        pass "Piper voice model: en_US-amy-medium"
    else
        warn "No Piper voice model found"
    fi

    # Play test tone
    echo -e "\n  ${CYAN}Playing test tone (1 second)...${NC}"
    if speaker-test -t sine -f 440 -l 1 -p 1 &>/dev/null; then
        pass "Audio playback works"
    else
        fail "Audio playback failed"
    fi

    # Test TTS
    echo -e "  ${CYAN}Testing TTS...${NC}"
    if espeak-ng "Test" &>/dev/null; then
        pass "TTS (espeak-ng) works"
    else
        fail "TTS failed"
    fi
fi

# ── Microphone / Audio Input ──
if [ "$FILTER" = "all" ] || [ "$FILTER" = "mic" ]; then
    header "Microphone / Audio Input"

    if arecord -l 2>/dev/null | grep -q "card"; then
        CARDS=$(arecord -l 2>/dev/null | grep "^card" | head -3)
        pass "Recording device detected:"
        echo "        $CARDS"
    else
        fail "No recording device detected (mic will not work)"
    fi

    echo -e "\n  ${CYAN}Recording 2 seconds of audio...${NC}"
    if timeout 3 arecord -d 2 -f S16_LE -r 16000 -q /tmp/aios-mic-test.wav 2>/dev/null; then
        SIZE=$(stat -c%s /tmp/aios-mic-test.wav 2>/dev/null || echo 0)
        if [ "$SIZE" -gt 1000 ]; then
            # Check if there's actual audio (not just silence)
            ENERGY=$(python3 -c "
import wave, struct, math
w = wave.open('/tmp/aios-mic-test.wav','r')
frames = w.readframes(w.getnframes())
samples = struct.unpack('<' + 'h' * w.getnframes(), frames)
rms = math.sqrt(sum(s*s for s in samples) / len(samples))
print(f'{rms:.1f}')
" 2>/dev/null || echo "0")
            if [ "$(echo "$ENERGY > 100" | bc 2>/dev/null || echo 0)" = "1" ]; then
                pass "Microphone recording works (energy: $ENERGY)"
                echo -e "  ${CYAN}Playing back recording...${NC}"
                aplay -q /tmp/aios-mic-test.wav 2>/dev/null || true
            else
                warn "Recording captured but only silence (energy: $ENERGY)"
                echo "        Mic may not be connected or SPICE doesn't forward audio input"
            fi
        else
            fail "Recording file too small ($SIZE bytes)"
        fi
        rm -f /tmp/aios-mic-test.wav
    else
        # Recording device detected but arecord failed — likely a VM without mic passthrough
        if systemd-detect-virt -q 2>/dev/null; then
            warn "arecord failed — VM detected, mic input not available (expected)"
        else
            fail "arecord failed — no microphone available"
        fi
    fi

    if command -v whisper-cpp-cli &>/dev/null; then
        pass "whisper-cpp-cli installed (STT ready)"
    else
        fail "whisper-cpp-cli not found (STT will not work)"
    fi

    if [ -f /home/aios/.aios/models/whisper/ggml-tiny.bin ]; then
        SIZE=$(du -h /home/aios/.aios/models/whisper/ggml-tiny.bin | cut -f1)
        pass "Whisper model: ggml-tiny.bin ($SIZE)"
    else
        fail "No Whisper model found"
    fi
fi

# ── Network ──
if [ "$FILTER" = "all" ] || [ "$FILTER" = "network" ]; then
    header "Network"

    if ip addr show | grep -q "inet " | grep -v "127.0.0.1"; then
        IP=$(hostname -I 2>/dev/null | awk '{print $1}')
        pass "Network interface up (IP: ${IP:-unknown})"
    else
        IP=$(hostname -I 2>/dev/null | awk '{print $1}')
        if [ -n "$IP" ]; then
            pass "Network up (IP: $IP)"
        else
            fail "No network interface"
        fi
    fi

    if ping -c 1 -W 2 8.8.8.8 &>/dev/null; then
        pass "Internet connectivity (ping 8.8.8.8)"
    else
        fail "No internet connectivity"
    fi

    if ping -c 1 -W 2 google.com &>/dev/null; then
        pass "DNS resolution working"
    else
        warn "DNS resolution failed"
    fi

    if pgrep -x avahi-daemon &>/dev/null; then
        pass "Avahi mDNS running (aios.local)"
    else
        warn "Avahi not running — aios.local won't resolve"
    fi

    if ss -tlnp 2>/dev/null | grep -q ":80 "; then
        pass "Web server listening on port 80"
    elif ss -tlnp 2>/dev/null | grep -q ":8080 "; then
        pass "Web server listening on port 8080"
    else
        fail "Web server not listening"
    fi

    if ss -tlnp 2>/dev/null | grep -q ":22 "; then
        pass "SSH server running"
    else
        warn "SSH server not running"
    fi
fi

# ── System ──
if [ "$FILTER" = "all" ] || [ "$FILTER" = "system" ]; then
    header "System"

    if [ -f /usr/bin/aios ]; then
        SIZE=$(du -h /usr/bin/aios | cut -f1)
        pass "AiOS binary installed ($SIZE)"
    else
        fail "AiOS binary not found"
    fi

    KB=$(cat /etc/default/keyboard 2>/dev/null | grep XKBLAYOUT | cut -d'"' -f2 || echo "unknown")
    pass "Keyboard layout: $KB"

    HOSTNAME=$(cat /etc/hostname 2>/dev/null || echo "unknown")
    pass "Hostname: $HOSTNAME"

    TZ=$(cat /etc/timezone 2>/dev/null || echo "unknown")
    pass "Timezone: $TZ"

    MEM=$(free -h 2>/dev/null | awk '/^Mem:/{print $2}')
    pass "RAM: $MEM"

    CPUS=$(nproc 2>/dev/null || echo "?")
    pass "CPUs: $CPUS"

    if pgrep -x spice-vdagentd &>/dev/null; then
        pass "SPICE agent daemon running (clipboard sharing)"
    else
        # Try starting it — socket activation may not have triggered yet
        sudo systemctl start spice-vdagentd.service 2>/dev/null
        sleep 0.5
        if pgrep -x spice-vdagentd &>/dev/null; then
            pass "SPICE agent daemon running (started on demand)"
        else
            warn "spice-vdagentd not running — clipboard won't work"
        fi
    fi

    if pgrep -x spice-vdagentd &>/dev/null; then
        pass "SPICE agent daemon running (clipboard via virtio-serial)"
    elif [ -e /dev/virtio-ports/com.redhat.spice.0 ]; then
        warn "spice-vdagentd not running — clipboard sharing may not work"
    else
        pass "No SPICE device — running on real hardware"
    fi

    if [ -f /home/aios/.aios/vault.enc ]; then
        pass "Vault exists (API keys stored)"
    else
        warn "No vault — run first-boot setup"
    fi
fi

# ── Summary ──
echo ""
echo -e "${BOLD}═══════════════════════════════════════${NC}"
echo -e "  ${GREEN}Passed: $PASS${NC}  ${RED}Failed: $FAIL${NC}  ${YELLOW}Warnings: $WARN${NC}"
if [ $FAIL -eq 0 ]; then
    echo -e "  ${GREEN}${BOLD}All critical tests passed!${NC}"
else
    echo -e "  ${RED}${BOLD}$FAIL test(s) failed — see above${NC}"
fi
echo -e "${BOLD}═══════════════════════════════════════${NC}"
TESTEOF
chmod +x /usr/bin/aios-test

# ── Hostname collision check script ──
cat > /usr/bin/aios-hostname-check << 'HCEOF'
#!/bin/bash
# Check if our hostname collides with another machine on the LAN.
HOSTNAME=$(cat /etc/hostname 2>/dev/null || echo "assistant")

# Wait for network to be ready
sleep 2

# Try to resolve <hostname>.local — if it resolves to a different IP, conflict
OUR_IPS=$(hostname -I 2>/dev/null | tr ' ' '\n' | grep -v '^$')
RESOLVED=$(avahi-resolve -n "${HOSTNAME}.local" -4 2>/dev/null | awk '{print $2}')

if [ -n "$RESOLVED" ]; then
    for ip in $OUR_IPS; do
        [ "$ip" = "$RESOLVED" ] && exit 0
    done
    # Conflict detected
    echo "$HOSTNAME" > /tmp/aios-name-conflict
    exit 1
fi
exit 0
HCEOF
chmod +x /usr/bin/aios-hostname-check

# ── Hostname collision check service ──
cat > /etc/systemd/system/aios-hostname-check.service << 'HCSEOF'
[Unit]
Description=Check AiOS hostname for mDNS conflicts
After=avahi-daemon.service network-online.target
Wants=network-online.target

[Service]
Type=oneshot
ExecStart=/usr/bin/aios-hostname-check
RemainAfterExit=yes

[Install]
WantedBy=multi-user.target
HCSEOF
systemctl enable aios-hostname-check.service

# ── VM graphics driver auto-detection ──
# Load VM GPU modules early via modules-load.d (runs before any display manager).
# On real hardware these modules simply won't exist and are silently skipped.
mkdir -p /etc/modules-load.d
cat > /etc/modules-load.d/aios-vm-gpu.conf << 'MODEOF'
# VM GPU drivers — each is silently skipped if not applicable
vboxvideo
vmwgfx
virtio-gpu
MODEOF

# Ensure video group has DRM access
cat > /etc/udev/rules.d/70-aios-drm.rules << 'UDEVEOF'
SUBSYSTEM=="drm", GROUP="video", MODE="0660"
UDEVEOF

# ── Install whisper.cpp for STT (build from source) ──
echo "[AiOS] Building whisper.cpp from source..."
WHISPER_VERSION="1.7.4"
if [ ! -f /usr/bin/whisper-cpp-cli ]; then
    # whisper.cpp v1.6+ requires CMake
    apt-get install -y -qq cmake >/dev/null 2>&1
    cd /tmp
    if curl -fsSL "https://github.com/ggerganov/whisper.cpp/archive/refs/tags/v${WHISPER_VERSION}.tar.gz" -o whisper-src.tar.gz 2>/dev/null && \
       tar xzf whisper-src.tar.gz && \
       cd "whisper.cpp-${WHISPER_VERSION}" && \
       cmake -B build -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=OFF 2>/dev/null && \
       cmake --build build --config Release -j"$(nproc)" 2>/dev/null; then
        # Find the built binary (whisper-cli in v1.7+)
        BUILT_BIN=$(find build -name 'whisper-cli' -type f 2>/dev/null | head -1)
        if [ -z "$BUILT_BIN" ]; then
            BUILT_BIN=$(find build -name 'main' -type f -executable 2>/dev/null | head -1)
        fi
        if [ -n "$BUILT_BIN" ]; then
            cp "$BUILT_BIN" /usr/bin/whisper-cpp-cli
            chmod +x /usr/bin/whisper-cpp-cli
            echo "[AiOS] whisper-cpp-cli built and installed"
        else
            echo "WARN: whisper.cpp built but binary not found — STT will not work"
        fi
    else
        echo "WARN: Failed to build whisper.cpp — STT will not work"
    fi
    cd /
    rm -rf /tmp/whisper*
    apt-get remove -y -qq cmake cmake-data >/dev/null 2>&1 || true
    apt-get autoremove -y -qq >/dev/null 2>&1 || true
fi

# Download whisper tiny model (75MB, good for real-time on most hardware)
WHISPER_MODEL_DIR="/home/aios/.aios/models/whisper"
mkdir -p "${WHISPER_MODEL_DIR}"
if [ ! -f "${WHISPER_MODEL_DIR}/ggml-tiny.bin" ]; then
    echo "[AiOS] Downloading whisper tiny model..."
    curl -fsSL "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin" \
        -o "${WHISPER_MODEL_DIR}/ggml-tiny.bin" 2>/dev/null || \
        echo "WARN: Failed to download whisper model"
fi

# ── Install Piper TTS for high-quality voices ──
echo "[AiOS] Installing Piper TTS..."
PIPER_VERSION="2023.11.14-2"
PIPER_URL="https://github.com/rhasspy/piper/releases/download/${PIPER_VERSION}/piper_linux_x86_64.tar.gz"
if [ -f /opt/piper/piper ]; then
    echo "[AiOS] Piper TTS already installed — skipping download"
elif curl -fsSL "${PIPER_URL}" -o /tmp/piper.tar.gz 2>/dev/null; then
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

# ── ONNX Runtime shared library (for KWS) ──
echo "[AiOS] Downloading ONNX Runtime shared library..."
mkdir -p /opt/aios-app/lib
ORT_VERSION="1.19.2"
wget -q "https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VERSION}/onnxruntime-linux-x64-${ORT_VERSION}.tgz" \
    -O /tmp/ort.tgz
tar -xzf /tmp/ort.tgz -C /tmp/
cp /tmp/onnxruntime-linux-x64-${ORT_VERSION}/lib/libonnxruntime.so.${ORT_VERSION} /opt/aios-app/lib/libonnxruntime.so
rm -rf /tmp/ort.tgz /tmp/onnxruntime-linux-x64-*

# ── KWS (Keyword Spotting) models ──
echo "[AiOS] Setting up KWS models..."
mkdir -p /opt/aios-app/models/kws/infrastructure
mkdir -p /opt/aios-app/models/kws/pretrained

# Download openWakeWord v0.5.1 infrastructure models (embedding + mel + VAD)
OWW_BASE="https://github.com/dscripka/openWakeWord/releases/download/v0.5.1"
wget -q -O /opt/aios-app/models/kws/infrastructure/embedding_model.onnx \
    "${OWW_BASE}/embedding_model.onnx" 2>/dev/null || \
    echo "WARN: Failed to download embedding_model.onnx"
wget -q -O /opt/aios-app/models/kws/infrastructure/melspectrogram.onnx \
    "${OWW_BASE}/melspectrogram.onnx" 2>/dev/null || \
    echo "WARN: Failed to download melspectrogram.onnx"
wget -q -O /opt/aios-app/models/kws/infrastructure/silero_vad.onnx \
    "${OWW_BASE}/silero_vad.onnx" 2>/dev/null || \
    echo "WARN: Failed to download silero_vad.onnx"

# Download official openWakeWord wake word model
wget -q -O /opt/aios-app/models/kws/pretrained/hey_jarvis.onnx \
    "${OWW_BASE}/hey_jarvis_v0.1.onnx" 2>/dev/null || \
    echo "WARN: Failed to download hey_jarvis.onnx"

# Download community wake word models from home-assistant-wakewords-collection
HA_WW_BASE="https://raw.githubusercontent.com/fwartner/home-assistant-wakewords-collection/main/en"
wget -q -O /opt/aios-app/models/kws/pretrained/computer.onnx "$HA_WW_BASE/computer/computer_v2.onnx" 2>/dev/null || true
wget -q -O /opt/aios-app/models/kws/pretrained/ok_computer.onnx "$HA_WW_BASE/ok_computer/ok_computer.onnx" 2>/dev/null || true
wget -q -O /opt/aios-app/models/kws/pretrained/hey_friday.onnx "$HA_WW_BASE/hey_friday/hey_friday!.onnx" 2>/dev/null || true
wget -q -O /opt/aios-app/models/kws/pretrained/jarvis.onnx "$HA_WW_BASE/jarvis/jarvis_v2.onnx" 2>/dev/null || true
wget -q -O /opt/aios-app/models/kws/pretrained/ok_jarvis.onnx "$HA_WW_BASE/ok_jarvis/ok_jarvis.onnx" 2>/dev/null || true
wget -q -O /opt/aios-app/models/kws/pretrained/skynet.onnx "$HA_WW_BASE/skynet/Skynet.onnx" 2>/dev/null || true
wget -q -O /opt/aios-app/models/kws/pretrained/terminator.onnx "$HA_WW_BASE/terminator/Terminator.onnx" 2>/dev/null || true
wget -q -O /opt/aios-app/models/kws/pretrained/hey_house.onnx "$HA_WW_BASE/hey_house/hey_house.onnx" 2>/dev/null || true
wget -q -O /opt/aios-app/models/kws/pretrained/ok_home.onnx "$HA_WW_BASE/ok_home/ok_home.onnx" 2>/dev/null || true
wget -q -O /opt/aios-app/models/kws/pretrained/home_assistant.onnx "$HA_WW_BASE/home_assistant/Home_assistant.onnx" 2>/dev/null || true
wget -q -O /opt/aios-app/models/kws/pretrained/mr_anderson.onnx "$HA_WW_BASE/mr_anderson/Mr._Anderson.onnx" 2>/dev/null || true
wget -q -O /opt/aios-app/models/kws/pretrained/mr_smith.onnx "$HA_WW_BASE/mr_smith/mr_smith.onnx" 2>/dev/null || true
wget -q -O /opt/aios-app/models/kws/pretrained/hey_dick_head.onnx "$HA_WW_BASE/hey_dick_head/hey_dick_head.onnx" 2>/dev/null || true
wget -q -O /opt/aios-app/models/kws/pretrained/oi_fuckwhit.onnx "$HA_WW_BASE/oi_fuckwhit/oi_fuckwhit_v2.onnx" 2>/dev/null || true
wget -q -O /opt/aios-app/models/kws/pretrained/yo_homie.onnx "$HA_WW_BASE/yo_homie/yo_homie.onnx" 2>/dev/null || true

# Copy pre-trained default wake word (built on dev machine and baked into ISO)
if [ -f /opt/aios-app/models/kws/pretrained/hey_assistant.onnx ]; then
    echo "[AiOS] hey_assistant.onnx already present"
else
    echo "INFO: No hey_assistant.onnx in ISO — user can train via /wake train"
fi

# First-boot: copy KWS models to user home directory (app handles this at startup)
# mkdir -p ~/.aios/models/kws/{infrastructure,pretrained,custom}
# cp -n /opt/aios-app/models/kws/infrastructure/* ~/.aios/models/kws/infrastructure/
# cp -n /opt/aios-app/models/kws/pretrained/* ~/.aios/models/kws/pretrained/
echo "[AiOS] KWS models setup complete."

# ── AiOS config (API keys from build config, injected before chroot) ──
# CLAUDE_KEY and OPENAI_KEY are already set from /tmp/aios-build-config
[ -z "${CLAUDE_KEY:-}" ] && CLAUDE_KEY=""
[ -z "${OPENAI_KEY:-}" ] && OPENAI_KEY=""
[ "$OPENAI_KEY" = "your-api-key-here" ] && OPENAI_KEY=""
echo "[AiOS] Config: claude_key=$([ -n "$CLAUDE_KEY" ] && echo 'set' || echo 'empty') openai_key=$([ -n "$OPENAI_KEY" ] && echo 'set' || echo 'empty')"
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
  "system": { "keyboard_layout": "${KB_LAYOUT}" },
  "channels": {
    "web": {
      "enabled": true,
      "port": 80
    }
  }
}
CFGEOF

# ── greetd: display manager that starts labwc ──
mkdir -p /etc/greetd
cat > /etc/greetd/config.toml << 'GREETEOF'
[terminal]
vt = 7

[default_session]
command = "/usr/bin/aios-start-compositor"
user = "aios"
GREETEOF
systemctl enable greetd
systemctl mask getty@tty1

# Compositor launcher — detects GPU and configures labwc accordingly
cat > /usr/bin/aios-start-compositor << 'COMPEOF'
#!/bin/bash
export LIBSEAT_BACKEND=logind

# If no render node exists (VirtualBox vboxvideo), use pixman renderer
if [ -e /dev/dri/card0 ] && ! ls /dev/dri/renderD* >/dev/null 2>&1; then
    export WLR_RENDERER=pixman
    export WLR_DRM_NO_MODIFIERS=1
fi

# No DRM device at all
if [ ! -e /dev/dri/card0 ]; then
    export WLR_RENDERER=pixman
    export LIBGL_ALWAYS_SOFTWARE=1
fi

exec labwc 2>/tmp/labwc-error.log
COMPEOF
chmod +x /usr/bin/aios-start-compositor

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

# Start SPICE clipboard agent (daemon via systemd + user agent)
if [ -e /dev/virtio-ports/com.redhat.spice.0 ]; then
    # Start the system daemon via systemd (uses socket activation)
    sudo systemctl start spice-vdagentd.service 2>/dev/null || true
    sleep 0.5
    spice-vdagent 2>/dev/null &
    echo "SPICE agent started"
else
    echo "No SPICE device — skipping vdagent"
fi

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
  <core>
    <decoration>server</decoration>
    <gap>0</gap>
  </core>
  <theme>
    <name>AiOS</name>
    <cornerRadius>6</cornerRadius>
    <font name="sans" size="11"/>
    <!-- Title bar buttons: close only (no minimize/maximize) -->
    <windowButton>close</windowButton>
  </theme>
  <keyboard>
    <default layout="${KB_LAYOUT}"/>
    <keybind key="A-F4"><action name="Close"/></keybind>
    <keybind key="A-Return"><action name="Execute"><command>foot --title "System Prompt"</command></action></keybind>
    <keybind key="A-F11"><action name="ToggleFullscreen"/></keybind>
    <keybind key="Print"><action name="Execute"><command>grim</command></action></keybind>
    <!-- Super key or Ctrl+Space opens AiOS -->
    <keybind key="Super_L"><action name="Execute"><command>/usr/bin/aios</command></action></keybind>
    <keybind key="C-space"><action name="Execute"><command>/usr/bin/aios</command></action></keybind>
    <!-- Alt-Tab window switcher -->
    <keybind key="A-Tab"><action name="NextWindow"/></keybind>
    <keybind key="A-S-Tab"><action name="PreviousWindow"/></keybind>
  </keyboard>
  <windowSwitcher show="yes" preview="yes" outlines="yes"/>
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
  <windowRules>
    <!-- AiOS main window: no decoration (it IS the desktop), skip taskbar -->
    <windowRule identifier="dev.aios.app" serverDecoration="no" skipTaskbar="yes" />
    <!-- System Prompt terminal: server-side decorations (title bar + close button) -->
    <windowRule title="System Prompt" serverDecoration="yes" />
  </windowRules>
</labwc_config>
RCEOF

# labwc theme override — dark window borders matching AiOS dark theme.
# This overrides the default openbox-3 themerc for the "AiOS" theme.
cat > /home/aios/.config/labwc/themerc-override << 'THEMEEOF'
# AiOS labwc theme — dark, minimal window decorations

# Active window (focused)
window.active.border.color: #3584e4
window.active.title.bg.color: #1a1a2e
window.active.label.text.color: #e6edf3

# Inactive window (unfocused)
window.inactive.border.color: #30363d
window.inactive.title.bg.color: #161b22
window.inactive.label.text.color: #8b949e

# OSD (Alt-Tab window switcher, menus)
osd.bg.color: #161b22
osd.border.color: #30363d
osd.label.text.color: #e6edf3

# Minimal decorations
border.width: 1
padding.height: 4

# No bottom handle, no client padding
window.handle.width: 0
window.client.padding.width: 0
THEMEEOF

# Foot terminal configuration.
mkdir -p /home/aios/.config/foot
cat > /home/aios/.config/foot/foot.ini << 'FOOTEOF'
[main]
title=System Prompt
font=monospace:size=11

[colors]
background=0d1117
foreground=e6edf3
FOOTEOF
chown -R aios:aios /home/aios/.config/foot

# ALSA config: use PipeWire for playback, direct ALSA for capture.
# PipeWire 0.3.65 has a bug where it holds the HDA capture device
# but doesn't return data to clients.
cat > /home/aios/.asoundrc << 'ALSA_EOF'
pcm.!default {
    type asym
    playback.pcm "pipewire"
    capture.pcm "plughw:0,0"
}
pcm.pipewire {
    type pipewire
}
ALSA_EOF

chown -R aios:aios /home/aios

# ── Enable PipeWire ──
systemctl --global enable pipewire pipewire-pulse wireplumber 2>/dev/null || true

# ── Clean up build config (contains API keys) ──
rm -f /opt/aios-app/build-config

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

# Copy KWS trainer scripts into ISO
mkdir -p config/includes.chroot/opt/aios-app/kws-trainer
if [ -f /work/distro/kws-trainer/train.py ]; then
    cp /work/distro/kws-trainer/train.py config/includes.chroot/opt/aios-app/kws-trainer/
    echo "[*] KWS train.py copied into ISO"
else
    echo "WARN: kws-trainer/train.py not found — skipping"
fi
if [ -f /work/distro/kws-trainer/requirements.txt ]; then
    cp /work/distro/kws-trainer/requirements.txt config/includes.chroot/opt/aios-app/kws-trainer/
    echo "[*] KWS requirements.txt copied into ISO"
else
    echo "WARN: kws-trainer/requirements.txt not found — skipping"
fi

# Copy pre-trained hey_assistant wake word model into ISO (if built on dev machine)
if [ -f /work/distro/models/kws/hey_assistant.onnx ]; then
    mkdir -p config/includes.chroot/opt/aios-app/models/kws/pretrained
    cp /work/distro/models/kws/hey_assistant.onnx \
        config/includes.chroot/opt/aios-app/models/kws/pretrained/hey_assistant.onnx
    echo "[*] hey_assistant.onnx baked into ISO"
else
    echo "[*] No hey_assistant.onnx in distro/models/kws/ — skipping (user can train via /wake train)"
fi

# Build and include user documentation (mdBook HTML)
if [ -f /work/docs/user-guide/book.toml ]; then
    if command -v mdbook >/dev/null 2>&1; then
        echo "[*] Building user documentation..."
        mdbook build /work/docs/user-guide 2>/dev/null
    fi
    if [ -d /work/docs/user-guide/book ]; then
        mkdir -p config/includes.chroot/usr/share/aios/docs
        cp -r /work/docs/user-guide/book/* config/includes.chroot/usr/share/aios/docs/
        echo "[*] User documentation included in ISO"
    fi
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

# Launch AiOS — restart on crash, exit cleanly on normal close
CRASH_COUNT=0
while true; do
    log "Launching /usr/bin/aios..."
    export ORT_DYLIB_PATH="/opt/aios-app/lib/libonnxruntime.so"
    /usr/bin/aios >> "$LOGFILE" 2>&1
    EXIT_CODE=$?
    log "AiOS exited with code $EXIT_CODE"

    if [ $EXIT_CODE -eq 0 ]; then
        # Normal close — just restart it after a short pause
        log "AiOS closed normally, restarting in 1s..."
        CRASH_COUNT=0
        sleep 1
    else
        # Crash — increment counter, restart with backoff
        CRASH_COUNT=$((CRASH_COUNT + 1))
        log "AiOS crashed (exit $EXIT_CODE), crash count: $CRASH_COUNT"

        if [ $CRASH_COUNT -ge 5 ]; then
            # Too many crashes — show debug terminal and stop
            log "Too many crashes, showing debug terminal"
            foot -e bash -c "echo 'AiOS crashed $CRASH_COUNT times.'; echo; tail -50 /tmp/aios-boot.log; echo; echo 'Press Enter to retry...'; read" 2>/dev/null
            CRASH_COUNT=0
        else
            # Brief pause before restart
            sleep 2
        fi
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
timeout 30
prompt 0

menu title AiOS
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

# ─── GRUB EFI Boot Config (for UEFI live boot) ─────────────────
# live-build uses config/bootloaders/grub-pc/ for the GRUB EFI menu.
mkdir -p config/bootloaders/grub-pc

cat > config/bootloaders/grub-pc/grub.cfg << 'GRUBEOF'
# AiOS GRUB EFI boot menu for live ISO

set timeout=3
set default=0

# Dark theme colors (no theme file needed for live — inline colors)
set menu_color_normal=light-gray/black
set menu_color_highlight=white/dark-gray
set color_normal=light-gray/black
set color_highlight=white/dark-gray

# Use graphics mode
if loadfont /boot/grub/fonts/unicode.pf2; then
    set gfxmode=auto
    insmod all_video
    insmod gfxterm
    terminal_output gfxterm
fi

# Load AiOS GRUB theme if available
if [ -f /boot/grub/themes/aios/theme.txt ]; then
    set theme=/boot/grub/themes/aios/theme.txt
fi

menuentry "AiOS" {
    linux /live/vmlinuz boot=live components username=aios quiet splash
    initrd /live/initrd.img
}

menuentry "AiOS Safe Mode" {
    linux /live/vmlinuz boot=live components username=aios single nomodeset
    initrd /live/initrd.img
}

menuentry "AiOS Debug Mode" {
    linux /live/vmlinuz boot=live components username=aios debug
    initrd /live/initrd.img
}
GRUBEOF

rm -f *.iso 2>/dev/null || true

echo "[*] Building AiOS ISO..."
lb build 2>&1 | tee build.log

# Save caches
mkdir -p /cache/packages.chroot
cp -a cache/packages.chroot/* /cache/packages.chroot/ 2>/dev/null || true

# Save bootstrap cache (the debootstrap tarball — avoids re-downloading base system)
if [ -d cache/bootstrap ] && [ ! -f /cache/bootstrap.tar ]; then
    echo "[*] Saving bootstrap cache..."
    tar cf /cache/bootstrap.tar -C cache bootstrap 2>/dev/null || true
fi

# Save external downloads + compiled binaries for next clean build
mkdir -p /cache/external
if [ -f chroot/home/aios/.aios/models/whisper/ggml-tiny.bin ]; then
    cp chroot/home/aios/.aios/models/whisper/ggml-tiny.bin /cache/external/ 2>/dev/null || true
fi
if [ -d chroot/opt/piper ]; then
    tar cf /cache/external/piper.tar -C chroot/opt piper 2>/dev/null || true
fi
if [ -d chroot/home/aios/.aios/models/piper ]; then
    tar cf /cache/external/piper-voices.tar -C chroot/home/aios/.aios/models piper 2>/dev/null || true
fi
if [ -f chroot/usr/bin/labwc ]; then
    cp chroot/usr/bin/labwc /cache/external/ 2>/dev/null || true
    tar cf /cache/external/labwc-share.tar -C chroot/usr/share labwc 2>/dev/null || true
fi
if [ -f chroot/usr/bin/whisper-cpp-cli ]; then
    cp chroot/usr/bin/whisper-cpp-cli /cache/external/ 2>/dev/null || true
fi

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
