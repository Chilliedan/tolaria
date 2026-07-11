---
type: ADR
id: "0147"
title: "Read-only web server phase (tolaria-server, Axum, Docker)"
status: active
date: 2026-06-30
---

## Context

ADR-0146 extracted vault/git/frontmatter/search logic into `tolaria-core`, a
library crate with no Tauri dependency. That crate is the foundation for a
second consumer: a browser-accessible web server that lets users read their vault
from any device without installing the desktop app.

The web client design spec (`docs/superpowers/specs/2026-06-29-web-client-design.md`)
defines five delivery phases. Phase 2 — the subject of this ADR — covers only the
read-only slice: serve the compiled SPA and answer vault-read RPC calls. Auth
(Phase 3), write operations (Phase 4), and git sync + WebSocket push (Phase 5)
are explicitly out of scope here.

Two design constraints shape the approach:

1. **Single source of vault logic.** The web server must not reimplement vault
   scanning, frontmatter handling, search, or caching. It must call `tolaria-core`
   directly, the same way the Tauri desktop app does.

2. **Single web client codebase.** The React/TypeScript frontend should run in
   both desktop (Tauri IPC) and browser (HTTP RPC) environments without
   source-level forking. Only the transport layer may differ.

## Decision

### `tolaria-server` crate (`src-tauri/crates/tolaria-server`)

A new Axum-based binary crate, `tolaria-server`, joins the Cargo workspace as a
sibling of `tolaria-core`. It has no dependency on `tauri` or any Tauri plugin.

**Routing:**

- `POST /api/cmd/:command` — generic RPC endpoint. The request body is a JSON
  args object. The `:command` segment is matched against a read-only whitelist
  inside `handlers::dispatch`. The whitelisted commands for Phase 2 are:
  `list_vault` / `reload_vault`, `get_note_content`, `get_all_content`,
  `reload_vault_entry`, `list_vault_folders`, `search_vault`.
  Any other command name returns HTTP 501 with `{ "error": "… not supported on web" }`.
- Static fallback — all other paths are served from the compiled SPA directory
  (`$TOLARIA_STATIC_DIR`, defaulting to `/app/dist`).

**Configuration** (`ServerConfig`, sourced from environment variables):

| Variable | Default | Description |
|---|---|---|
| `TOLARIA_VAULT_PATH` | *(required)* | Absolute path to the vault on disk |
| `TOLARIA_STATIC_DIR` | `/app/dist` | Directory containing the compiled SPA |
| `TOLARIA_HOST` | `0.0.0.0` | Bind host |
| `TOLARIA_PORT` | `8787` | Bind port |

**Security posture (Phase 2):** The server is unauthenticated by design. It is
intended to run behind a trusted reverse proxy (nginx or similar) that controls
network exposure. The vault mount is read-only at the OS level (Docker `:ro`
volume). No write command is dispatched; any write attempt receives 501 and
degrades silently on the client. Authentication is deferred to Phase 3.

### Web Vite target and transport shim (`vite.config.web.ts`, `src/web/`)

A separate Vite config (`vite.config.web.ts`) merges over the base config and
aliases all `@tauri-apps/api/*` imports to thin shims in `src/web/`:

- `src/web/transport.ts` — replaces `@tauri-apps/api/core`. `invoke(command, args)`
  POSTs to `/api/cmd/${command}` and returns the JSON response. A 501 response
  resolves to `undefined` so unsupported commands degrade gracefully rather than
  throwing. `convertFileSrc` and `Channel` are no-op stubs.
- `src/web/event.ts` — no-op stub for `@tauri-apps/api/event`.
- `src/web/window.ts` — no-op stub for `@tauri-apps/api/window` and
  `@tauri-apps/api/webview`.

No application source files are modified. The aliasing is purely a build-time
concern.

### Docker packaging (`Dockerfile`, `docker-compose.yml`, `nginx.prod.conf`)

A plain two-source-stage Dockerfile builds:

1. **Stage 1 (Node 22):** `pnpm build:web` compiles the SPA to `dist/`.
2. **Stage 2 (rust:1-bookworm):** `cargo build --release -p tolaria-server`
   produces the server binary using a plain `cargo build` (no Cargo Chef layer
   caching). Tauri's system library dependencies are avoided because only the
   `tolaria-server` crate is compiled.
3. **Stage 3 (debian:bookworm-slim):** copies the binary and `dist/` into a
   minimal runtime image and exposes port 8787. Environment variable defaults
   (`TOLARIA_STATIC_DIR`, `TOLARIA_HOST`, `TOLARIA_PORT`) are baked in;
   `TOLARIA_VAULT_PATH` is intentionally **not** baked so that a bare
   `docker run` without a vault mount fails with a clear configuration error
   rather than silently pointing at a non-existent path. `docker-compose.yml`
   supplies `TOLARIA_VAULT_PATH` explicitly.

`docker-compose.yml` defines two services:

- `tolaria-web` — the server container, binding only to `127.0.0.1:8787`. The
  vault directory is mounted read-only (`:ro`).
- `proxy` (profile `docker-edge`) — an optional nginx container that acts as the
  public-facing edge, forwarding all traffic to `tolaria-web`. Users who already
  run a host nginx instance use `nginx.prod.conf` directly and omit this profile.

## Consequences

- The desktop Tauri app is unaffected: `tolaria-server` is a separate binary,
  not linked into the Tauri crate.
- The web client renders the full Tolaria UI from a browser. Features that
  require unsupported commands (write, git, AI agents, updater) silently
  degrade: `invoke` returns `undefined` for 501 responses.
- The single codebase constraint is satisfied: one React app, two Vite entry
  configs, no forked source files.
- Auth, write operations, git sync controls, and WebSocket push events are
  explicitly not implemented and are deferred to Phases 3–5.
- The 501-degrade contract means Phase 3+ can add commands to the server
  whitelist without touching the client transport layer.
