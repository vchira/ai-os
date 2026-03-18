FROM debian:bookworm

# ─── Build tools (Rust, live-build, GTK dev libs) ─────────────
RUN apt-get update -qq && \
    apt-get install -y -qq live-build debian-archive-keyring \
    curl build-essential pkg-config \
    libgtk-4-dev libadwaita-1-dev libasound2-dev libssl-dev \
    python3-pil fonts-dejavu-core \
    squashfs-tools xorriso isolinux >/dev/null 2>&1 && \
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable && \
    apt-get clean
ENV PATH="/root/.cargo/bin:${PATH}"
RUN cargo install mdbook 2>/dev/null || true

# ─── Pre-download all ISO packages into a cache dir ───────────
# This layer is cached by Docker. Only rebuilds when packages.list changes.
COPY packages.list /tmp/packages.list
RUN mkdir -p /cache/prefetch && \
    cd /cache/prefetch && \
    apt-get update -qq && \
    apt-get download $(cat /tmp/packages.list | grep -v '^#' | grep -v '^$' | tr '\n' ' ') 2>/dev/null || true && \
    apt-get clean
