# Ambiente de desenvolvimento do MyHarness (Tauri 2 + Rust + React), para rodar no WSL com Docker.
# Debian bookworm traz o webkit2gtk-4.1 que o Tauri 2 pede no Linux.
FROM rust:1-bookworm

ARG NODE_MAJOR=22
ARG USER_UID=1000
ARG USER_GID=1000

ENV DEBIAN_FRONTEND=noninteractive

# Dependências de sistema do Tauri 2 no Linux + git + cofre de credenciais (Secret Service) para o keyring
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential curl wget file ca-certificates git pkg-config \
      libwebkit2gtk-4.1-dev libxdo-dev libssl-dev \
      libayatana-appindicator3-dev librsvg2-dev \
      dbus dbus-x11 gnome-keyring libsecret-1-dev \
      xauth fonts-noto-color-emoji \
    && rm -rf /var/lib/apt/lists/*

# Node + pnpm (via corepack)
RUN curl -fsSL https://deb.nodesource.com/setup_${NODE_MAJOR}.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/* \
    && corepack enable

# Claude CLI (os agentes são processos `claude -p`)
RUN npm install -g @anthropic-ai/claude-code

# Ferramentas Rust de qualidade
RUN rustup component add rustfmt clippy

# Usuário sem root, com o mesmo UID do usuário do WSL (arquivos do volume não ficam de root)
RUN groupadd --gid ${USER_GID} dev \
    && useradd --uid ${USER_UID} --gid ${USER_GID} -m -s /bin/bash dev \
    && mkdir -p /workspace /home/dev/.cargo-cache /home/dev/.pnpm-store \
    && chown -R dev:dev /workspace /home/dev /usr/local/cargo

USER dev
ENV CARGO_HOME=/usr/local/cargo \
    CARGO_TARGET_DIR=/workspace/target \
    PNPM_HOME=/home/dev/.pnpm-store \
    PATH=/usr/local/cargo/bin:/home/dev/.pnpm-store:$PATH

WORKDIR /workspace
CMD ["sleep", "infinity"]
