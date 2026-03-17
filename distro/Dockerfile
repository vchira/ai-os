FROM debian:bookworm
RUN apt-get update -qq && \
    apt-get install -y -qq live-build debian-archive-keyring \
    curl build-essential pkg-config \
    libgtk-4-dev libadwaita-1-dev libasound2-dev libssl-dev \
    python3-pil fonts-dejavu-core \
    squashfs-tools xorriso isolinux >/dev/null 2>&1 && \
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable && \
    apt-get clean
ENV PATH="/root/.cargo/bin:${PATH}"
