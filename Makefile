# AiOS — AI-Native Linux Distribution
# ====================================
#
# AiOS v2.0: Linux-based AI-native operating system
# Built on Debian Bookworm with Wayland (labwc) and GTK4
#
# Targets:
#   make app       — Install the AiOS application locally
#   make run       — Run the AiOS app (development mode)
#   make selftest  — Run the self-test suite (/selftest)
#   make test      — Run unit tests
#   make iso       — Build the bootable/installable ISO
#   make qemu      — Build ISO and launch in QEMU
#   make clean     — Clean build artifacts

.PHONY: app run selftest test iso qemu clean help

PYTHON ?= python3
PIP ?= $(PYTHON) -m pip

help:
	@echo "AiOS — AI-Native Linux Distribution v2.0"
	@echo ""
	@echo "Application:"
	@echo "  make app       Install AiOS app and dependencies"
	@echo "  make run       Run AiOS in development mode"
	@echo "  make selftest  Run /selftest (simulated conversation test)"
	@echo "  make test      Run unit tests with pytest"
	@echo ""
	@echo "Distribution:"
	@echo "  make iso       Build the AiOS Linux ISO (requires sudo + live-build)"
	@echo "  make qemu      Build ISO and test in QEMU"
	@echo ""
	@echo "Other:"
	@echo "  make clean     Clean all build artifacts"
	@echo "  make help      Show this help"

# ─── Application ──────────────────────────────────────────────────

app:
	cd aios-app && $(PIP) install -e ".[dev]"

run:
	cd aios-app && $(PYTHON) -m aios

selftest:
	cd aios-app && $(PYTHON) -c "from aios.config.manager import ConfigManager; from aios.selftest.runner import SelfTestRunner; c = ConfigManager(); r = SelfTestRunner(c, None); print(r.run_all())"

test:
	cd aios-app && $(PYTHON) -m pytest tests/ -v

# ─── Distribution ────────────────────────────────────────────────

iso:
	cd distro && sudo ./build.sh

qemu:
	cd distro && ./run-qemu.sh

# ─── Cleanup ─────────────────────────────────────────────────────

clean:
	rm -rf distro/build
	rm -rf aios-app/build aios-app/dist aios-app/*.egg-info
	find . -type d -name __pycache__ -exec rm -rf {} + 2>/dev/null || true
	find . -type f -name "*.pyc" -delete 2>/dev/null || true
