# syntax=docker/dockerfile:1.4
# Stage 1: Build Frontend Assets (Vite SPA)
FROM node:22-bookworm-slim AS frontend-builder
WORKDIR /build
RUN corepack enable

COPY apps/bob-app/package.json apps/bob-app/pnpm-lock.yaml ./apps/bob-app/
WORKDIR /build/apps/bob-app
RUN pnpm install --frozen-lockfile

COPY apps/bob-app /build/apps/bob-app
RUN pnpm build

# Stage 2: Build Rust bob-node Server Binary
FROM rust:1.85-bookworm AS rust-builder
WORKDIR /build

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN cargo build --release -p bob-node

# Stage 3: Minimal Production Runtime Image (Debian + Chromium headless + bob-node, Zero Node.js)
FROM debian:bookworm-slim AS runtime

# Install CA certificates, curl for healthchecks, CJK/Emoji/Liberation fonts, and Chromium for bob-browser engine
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    chromium \
    curl \
    fontconfig \
    fonts-liberation \
    fonts-noto-cjk \
    fonts-noto-color-emoji \
    && fc-cache -f \
    && rm -rf /var/lib/apt/lists/*

# Create non-root bob user and secure private state directories (0700 per AEAD vault & Chromium isolation)
RUN groupadd -g 10001 bob && \
    useradd -u 10001 -g bob -m -s /bin/bash bob && \
    mkdir -p /var/lib/bob-node/browser_profile /data/browser_profile /usr/share/bob-app/dist && \
    chown -R bob:bob /var/lib/bob-node /data && \
    chmod 700 /var/lib/bob-node /var/lib/bob-node/browser_profile && \
    chmod 700 /data /data/browser_profile

# Copy compiled Rust server binary
COPY --from=rust-builder /build/target/release/bob-node /usr/local/bin/bob-node

# Copy compiled frontend SPA
COPY --from=frontend-builder /build/apps/bob-app/dist /usr/share/bob-app/dist

# Default runtime configuration
ENV LANG=C.UTF-8
ENV BOB_CHROMIUM_BIN=/usr/bin/chromium
ENV CHROME_BIN=/usr/bin/chromium
ENV BOB_NODE_BIND=0.0.0.0:8787 \
    BOB_NODE_DATA_DIR=/var/lib/bob-node \
    BOB_NODE_STATIC_DIR=/usr/share/bob-app/dist

USER bob
WORKDIR /var/lib/bob-node

EXPOSE 8787

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD curl -f http://127.0.0.1:8787/api/capabilities || exit 1

ENTRYPOINT ["/usr/local/bin/bob-node"]
CMD ["--bind", "0.0.0.0:8787", "--data-dir", "/var/lib/bob-node", "--static-dir", "/usr/share/bob-app/dist"]
