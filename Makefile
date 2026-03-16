# AiOS — AI-Native Linux Distribution
# ====================================
#
# AiOS v2.0: Linux-based AI-native operating system
# Built on Debian Bookworm with Wayland (labwc) + Rust (GTK4/libadwaita)
#
# Targets:
#   make build     — Build the Rust application (release)
#   make dev       — Build debug + run tests
#   make test      — Run all workspace tests
#   make check     — Check compilation (fast, no codegen)
#   make iso       — Build the bootable/installable ISO
#   make qemu      — Build ISO and launch in QEMU
#   make clean     — Clean build artifacts

.PHONY: build dev test check iso qemu clean help

help:
	@echo "AiOS — AI-Native Linux Distribution v2.0"
	@echo ""
	@echo "Development:"
	@echo "  make build     Build release binary"
	@echo "  make dev       Build debug + run tests"
	@echo "  make test      Run all workspace tests"
	@echo "  make check     Fast compilation check (no codegen)"
	@echo ""
	@echo "Distribution:"
	@echo "  make iso       Build the AiOS Linux ISO (requires Docker)"
	@echo "  make qemu      Build ISO and test in QEMU"
	@echo ""
	@echo "Other:"
	@echo "  make clean     Clean all build artifacts"
	@echo "  make help      Show this help"

# ─── Development ────────────────────────────────────────────────

build:
	cd aios-app-rs && cargo build --release

dev: check test

test:
	cd aios-app-rs && cargo test --workspace

check:
	cd aios-app-rs && cargo check --workspace

# ─── Distribution ────────────────────────────────────────────────

iso:
	cd distro && sudo ./build.sh

qemu:
	cd distro && ./run-vm.sh

# ─── Cleanup ─────────────────────────────────────────────────────

clean:
	rm -rf distro/build
	cd aios-app-rs && cargo clean
