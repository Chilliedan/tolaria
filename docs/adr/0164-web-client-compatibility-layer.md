---
type: ADR
id: "0164"
title: "Web client compatibility layer for reusing the desktop app over HTTP"
status: active
date: 2026-07-03
---

## Context

The web client (ADRs 0146–0149) reuses the **same React application** as the
desktop app, served by `tolaria-server` and talking to it over HTTP instead of
Tauri IPC. The desktop app was written against a full Tauri backend, with an
in-memory mock (`src/mock-tauri`) used only for server-less browser development.

Deploying the real app against the single-vault HTTP server surfaced a class of
gaps that the mock had been silently papering over:

- The app has ~115 `isTauri()` branches. Many are `isTauri() ? invoke :
  mockInvoke`, or `if (!isTauri()) mockInvoke(...)`. In the browser `isTauri()`
  is `false`, so those data operations went to the **in-memory mock** — writes
  (create/rename/delete) never reached the server and vanished on refresh; some
  reads returned fixtures instead of the real vault.
- The app discovers *which vault it has* via commands the read/write handlers
  did not implement (`load_vault_list`, `get_last_vault_path`,
  `check_vault_exists`). Served from the mock, they reported a fake vault path
  (`/Users/mock/demo-vault-v2`), so every note path was built there and the
  server rejected writes; and later the vault was marked unavailable, dropping
  the user into onboarding.
- The transport fingerprints note content with Web Crypto (`crypto.subtle`) for
  the optimistic-concurrency `baseHash`. `crypto.subtle` only exists in **secure
  contexts** (HTTPS or `localhost`); over plain HTTP it is `undefined`, which
  threw on every `get_note_content` and broke note loading.
- Desktop-only local bridges (e.g. the AI UI-action WebSocket on
  `ws://localhost:9711`) tried to connect from the browser and failed in a
  reconnect loop.

These emerged from debugging a live deployment rather than from the planned
phase work, so they are consolidated here.

## Decision

Introduce a **web-build compatibility layer** so the unmodified desktop app runs
correctly against `tolaria-server`, with three cooperating parts.

### 1. Mock bridge (frontend)

The web Vite build redirects imports of `src/mock-tauri` to `src/web/mockBridge.ts`
(via a `resolveId` plugin in `vite.config.web.ts`, because the imports are
relative). The bridge:

- Routes commands the server **implements** (a `SERVER_COMMANDS` allowlist —
  vault list/read/search, all writes, and the vault-registry commands below) to
  the real HTTP transport (`src/web/transport.ts`).
- Falls back to the original in-memory `mockHandlers` for everything else
  (AI, git, clipboard, window/menu, PDF, workspace sessions), so unimplemented
  desktop commands return plausible data instead of `undefined` — which the app
  would otherwise `.map()` and crash on at render time.
- Keeps `isTauri()` returning **`false`**. This is deliberate: the many
  `if (isTauri()) <tauri-plugin-call>` guards must stay off in the browser
  (those plugins are not shimmed and would throw). Making the *fallback* talk to
  the server — rather than flipping `isTauri()` to `true` — fixes data flow
  without enabling native-only paths.
- Makes the mock state mutators (`addMockEntry`, etc.) no-ops — the server owns
  the commands it implements.

### 2. Vault registry over HTTP (server)

`tolaria-server` serves exactly one vault (`vault_root`). It answers the
vault-discovery/availability commands so the app operates on that real vault:

- `load_vault_list` → `{ vaults: [{ label, path }], active_vault, hidden_defaults: [] }`
  with `path`/`active_vault` = `vault_root`.
- `get_last_vault_path` → `vault_root`.
- `check_vault_exists({ path })` → `true` only when the path canonicalizes to
  `vault_root` (else `false`).
- `set_last_vault_path` / `save_vault_list` → no-op (`null`): a single-vault
  server does not let clients reconfigure which vault is served.

### 3. Insecure-context (HTTP) degradation

- The transport's `sha256Hex` returns `null` when `crypto.subtle` is
  unavailable, so content-version tracking is **skipped** rather than throwing.
  No `baseHash` is sent, so the server's optimistic-concurrency check does not
  run — conflict protection degrades to best-effort over plain HTTP.
- The session cookie's `Secure` attribute is controlled by
  `TOLARIA_COOKIE_SECURE` (ADR 0148); it must be `false` for non-TLS testing.

Desktop-only local WebSocket bridges (the AI UI-action bridge on
`ws://localhost:9711`) are gated behind `isTauri()` so the web build never
attempts them.

## Consequences

- The unmodified desktop app now creates/opens/edits/renames/deletes notes over
  HTTP against the real vault, and shows the real vault contents.
- **HTTPS is required for full functionality.** Over plain HTTP two things
  degrade: optimistic-concurrency conflict detection is off (no `baseHash`), and
  `TOLARIA_COOKIE_SECURE` must be `false`. Both are restored under TLS on the
  nginx edge. HTTP is acceptable only for single-user testing, not multi-user
  use.
- Features backed by commands the server does not implement (AI, git status,
  PDF, workspace sessions) show **mock/placeholder** data on web. Making those
  real (e.g. `scan_views`, favorites, and git status/commit — the latter is
  Phase 5) requires implementing them server-side and adding them to
  `SERVER_COMMANDS`.
- Known follow-ups (from code review of this change):
  - `SERVER_COMMANDS` is a hand-maintained allowlist duplicating the server's
    supported set; a server command added without updating it silently routes to
    the mock. A server-advertised command manifest would remove the drift.
  - The mock-bridge fallback still sends an unknown, unmocked command to the
    server (501 → `undefined`); a command missing from `mockHandlers` can
    reintroduce the `undefined` crash class this layer exists to prevent.
  - The server should log a clear warning when its configured vault path does
    not exist (today `check_vault_exists` just returns `false`, silently
    dropping the user into onboarding).
