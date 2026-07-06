# --- Stage 1: build the web bundle (Node 22, matches CI) ---
FROM node:22-bookworm-slim AS web
WORKDIR /app
RUN corepack enable && corepack prepare pnpm@10.18.0 --activate
COPY . .
RUN pnpm install --frozen-lockfile
RUN NODE_OPTIONS=--max-old-space-size=4096 pnpm exec vite build --config vite.config.web.ts

# --- Stage 2: build the server binary ---
FROM rust:1-bookworm AS server
WORKDIR /build
COPY src-tauri ./src-tauri
# Build only the server crate (skips the Tauri app's system deps).
RUN cargo build --release --manifest-path src-tauri/Cargo.toml -p tolaria-server

# --- Stage 3: runtime ---
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends git openssh-client ca-certificates \
  && rm -rf /var/lib/apt/lists/*
# The vault is bind-mounted from the host and is owned by the host UID, not the
# container's root user. Since git 2.35.2 that triggers "detected dubious
# ownership" and git refuses to operate on the repo, which breaks every git
# command the server runs (remote status, autogit commits, push/pull). This is
# a single-purpose container that only ever operates on the mounted vault, so
# trust all repositories at the system level.
RUN git config --system --add safe.directory '*'
WORKDIR /app
COPY --from=server /build/src-tauri/target/release/tolaria-server /usr/local/bin/tolaria-server
COPY --from=web /app/dist /app/dist
ENV TOLARIA_STATIC_DIR=/app/dist \
    TOLARIA_HOST=0.0.0.0 \
    TOLARIA_PORT=8787 \
    TOLARIA_USERS_DB=/app/data/users.db
RUN mkdir -p /app/data
EXPOSE 8787
ENTRYPOINT ["tolaria-server"]
