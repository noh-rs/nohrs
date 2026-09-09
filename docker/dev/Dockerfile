FROM debian:trixie

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential pkg-config curl ca-certificates \
    libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev \
    libxcb-shape0-dev libxcb-xfixes0-dev libxcb1-dev libx11-dev \
    libfontconfig1-dev libfreetype6-dev \
    libgl1-mesa-dev libegl1-mesa-dev libvulkan-dev mesa-vulkan-drivers \
    libssl-dev libsqlite3-dev \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /nohrs
COPY rust-toolchain.toml .
RUN rustup show

CMD ["bash"]
