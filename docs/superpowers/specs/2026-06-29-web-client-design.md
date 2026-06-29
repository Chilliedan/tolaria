# Tolaria Web Client — Design

- **Date:** 2026-06-29
- **Status:** Approved (design phase)
- **Author:** Daniel Howorth (with Claude Code)

## 1. Goal

Provide a web-served version of Tolaria that runs on a remote server and lets
multiple authenticated team members interact with a **single shared vault** from
a browser. The team mostly works on separate notes; concurrent edits to the same
note are rare but **must never corrupt data**.

The web server is treated as **just another Tolaria client**: it holds its own
git clone of the vault and syncs against a shared remote (a git peer), exactly
like a desktop client does today.

## 2. Context

Tolaria today is a Tauri (Rust + React) desktop app:

- Filesystem is the source of truth; the vault is a flat-ish folder of Markdown
  files (ADR 0002, ADR 0006).
- Git provides versioning and sync against a remote (ADR 0014, ADR 0034,
  ADR 0021 push-to-main workflow, ADR 0067 autogit checkpoints).
- The React frontend talks to a Rust core through ~190 `#[tauri::command]`
  functions invoked via `invoke()` from `@tauri-apps/api/core`. `invoke` is
  imported directly in ~66 frontend files.
- A partial HTTP seam already exists for browser dev: `src/mock-tauri/vault-api.ts`
  routes a subset of commands (list, content, search, save, rename, delete) to
  `/api/vault/*`, and `scripts/serve-demo.mjs` is a no-auth, single-user Node
  demo server that serves the built frontend and answers those routes. This
  proves the React app already runs in a plain browser against an HTTP backend,
  but it is demo-grade only.
- Network-aware UI gating already exists to hide features that require
  connectivity or desktop capabilities (ADR 0060).

## 3. Decisions captured during brainstorming

| Question | Decision |
|---|---|
| Core scenario | Small **team**, mostly **separate notes**; concurrent same-note edits rare but must not corrupt data. |
| Source-of-truth model | Server is a **git peer**: own clone, push/pull against a shared remote. |
| v1 feature scope | Core note editing · search & views · git sync controls. Rich blocks deferred. |
| Auth | **Built-in users + sessions** (server user store, hashed passwords), each user mapped to a git author identity. |
| Implementation approach | **A — Rust server reusing an extracted vault core; same React app on a web transport.** |

## 4. Architecture

Three components, maximizing reuse:

```
                     ┌─────────────────────────────┐
   Browser  ─HTTP/WS─▶│  tolaria-server (Axum)     │
 (same React app,    │  auth · RPC dispatch · WS   │
  web transport)     │  vault watcher · git lock   │
                    └──────────────┬──────────────┘
                                   │ depends on
                    ┌──────────────▼──────────────┐
                    │  tolaria-core (new crate)    │◀── also used by
                    │  vault scan · frontmatter ·  │    the Tauri desktop app
                    │  search · git ops · folders  │
                    └──────────────────────────────┘
```

### 4.1 `tolaria-core` (new Rust crate)

Extract the transport-agnostic logic currently embedded inside
`#[tauri::command]` functions into a plain Rust library crate with **no Tauri
dependency**:

- Vault scanning / entry model
- Frontmatter parsing & editing
- Keyword search
- Git operations (status, commit, push, pull, conflict detection)
- Folder operations

Both the existing Tauri desktop app and the new server depend on this crate.
Tauri command functions become thin wrappers that call into `tolaria-core` and
adapt `AppHandle`/`State`/event emission. This guarantees a **single source of
truth for vault behavior** — no parallel reimplementation.

This extraction is the main upfront cost and is the first implementation phase.

### 4.2 `tolaria-server` (new Axum binary)

Responsibilities:

- Serve the static web build.
- Authenticate users and manage sessions.
- Expose a uniform RPC endpoint that dispatches to `tolaria-core`.
- Push real-time change events to connected clients via WebSocket.
- Own the single vault git clone and serialize git/working-tree operations.

### 4.3 Frontend (same React app, web transport)

The browser loads the **same React application**. A web build target aliases the
Tauri modules to web shims (see §6) so `invoke()` and event subscriptions go over
HTTP/WS instead of Tauri IPC.

## 5. Server design

### 5.1 Routes

| Route | Method | Purpose |
|---|---|---|
| `/api/auth/login` | POST | Validate credentials, set session cookie. |
| `/api/auth/logout` | POST | Invalidate session. |
| `/api/auth/me` | GET | Current user (for bootstrapping the UI). |
| `/api/cmd/:command` | POST | Authenticated RPC dispatch to `tolaria-core`. JSON args in body. |
| `/api/events` | GET (WS) | Per-connection event stream (vault changed, git status, conflict). |
| `/*` (fallback) | GET | Serve the web build (SPA). |

### 5.2 Authentication

- Built-in user store in a small file or embedded SQLite. Passwords hashed with
  **argon2**. No plaintext at rest.
- Login issues a server-side session keyed by an **HttpOnly + Secure + SameSite**
  cookie. Sessions expire and can be revoked.
- Each user record carries a **git author identity** (`name`, `email`) used to
  attribute commits.
- Account provisioning is an admin/config task for v1 (no self-service signup).

### 5.3 RPC dispatch

- A single `POST /api/cmd/:command` endpoint, authenticated, maps a command name
  to a `tolaria-core` handler. This generalizes the existing `/api/vault/*`
  convention into a uniform RPC surface.
- Only the **v1 command set** is wired:
  vault read / write / list / reload / search / rename / rename-filename /
  delete, frontmatter read/write, folder operations, and git
  status / commit / push / pull.
- Any command not in the v1 set returns a structured **"unsupported on web"**
  error so the frontend can gate gracefully.

### 5.4 Real-time events

- The server runs the existing notify-based vault watcher against the vault path.
- Filesystem changes (and git status changes) are broadcast over `/api/events`
  to all connected clients, so note lists, open notes, and git status refresh
  without manual reload.
- The frontend `listen()` shim subscribes to this stream.

## 6. Frontend transport seam

`invoke` is imported directly in ~66 files; events/window/menu are also imported
directly. Rather than edit every call site, the **web build aliases the Tauri
modules** (via Vite alias) to web shims:

- `@tauri-apps/api/core` → shim where `invoke(cmd, args)` performs
  `POST /api/cmd/:command` with JSON args and the session cookie, returning the
  decoded result or throwing on error (including the `409` conflict case, §7).
- `@tauri-apps/api/event` → shim where `listen(event, cb)` subscribes to the
  WebSocket stream and `emit` is a no-op (or a server round-trip where needed).
- Desktop-only modules (`menu`, `window`, `deep-link`, `process`, `updater`,
  `dialog`) → shims that no-op or map to web equivalents.

Existing **network-aware UI gating (ADR 0060)** hides surfaces that don't apply
on web (native menus, CLI AI agents, deep links, updater).

Build-target selection: the web bundle is produced by a dedicated Vite
configuration/mode so the desktop (Tauri) build is unchanged.

## 7. Concurrency safety

The "must not corrupt" requirement is met with **optimistic concurrency**, not
locking the user out:

- Every note read returns a **version token** (content hash).
- A save sends the base version token it was edited from. The server performs a
  **compare-and-set under a per-path async mutex**:
  - If the on-disk version matches the base token → write succeeds, new token
    returned.
  - If it differs (someone else saved in between) → respond **`409 Conflict`**
    with the current on-disk content. The frontend opens the **existing conflict
    resolution UI** (the same one used for git conflicts). No silent overwrite.
- A **per-path async mutex** serializes concurrent saves to the same file within
  the server process.
- A **repo-level lock** serializes git index operations (commit / pull / push)
  so the single shared working tree never races.

## 8. Git sync model (one working tree, many users)

- Autosave writes the file to the working tree, then a **debounced commit**
  authored by the **acting user** (reuses the autogit concept, ADR 0067). The
  committer may be a fixed server identity; the **author** is always the user.
- Saves to *different* files are concurrent-safe via per-path locks; saves to the
  *same* file are caught by optimistic concurrency (§7).
- Pull/push run **behind the repo-level lock** against the shared remote using
  the **server's single push credential**. Per-user attribution is preserved
  through commit authorship.
- Remote merge conflicts (changes pulled from other peers) are surfaced through
  the existing conflict resolver and resolved server-side / via the web UI.
- The web "git controls" surface exposes commit / push / pull / status and
  conflict resolution.

## 9. Out of scope for v1

Deferred and **shimmed or hidden** on web:

- Whiteboards (tldraw), sheets (IronCalc), PDF export
- CLI AI agents, MCP server
- Native menus, deep links, auto-updater

**Hard constraint:** notes that *contain* tldraw/sheet/other rich blocks must
**round-trip safely** — render as-is or as a read-only placeholder and **never
mangle the Markdown** on save. This is a correctness requirement, not a nicety.

Mobile/responsive layout is not a v1 goal (target is the desktop browser).

## 10. Testing & gates

- **Rust:** existing `tolaria-core` logic tests carry over; new `tolaria-server`
  tests for auth/session, RPC dispatch, the `409` optimistic-concurrency path,
  and git locking. `cargo llvm-cov … --fail-under-lines 85`.
- **Frontend:** unit-test the web transport shim (invoke→HTTP, listen→WS,
  conflict propagation); reuse existing component tests. Frontend coverage ≥70%.
- **E2E:** a **new Playwright web configuration** driving the server for core
  flows — login → open/create/edit/save → search → commit/push — kept separate
  from the Tauri smoke lane.
- All AGENTS.md release gates apply: CodeScene ratchet (each touched file leaves
  ≥ its starting score; new scorable files reach 10.0), Codacy (no new
  Critical/High), localization for any new UI copy (`en.json` + `pnpm
  l10n:translate` + `pnpm l10n:validate`), PostHog events for meaningful new
  user actions (e.g. web login, web save, web push).

## 11. Deployment

- Single server binary + static web assets.
- Configuration via env/file: vault path, remote URL, listen address, session
  secret. TLS terminated by a reverse proxy.
- The **single server-side push credential** to the remote is supplied at
  **Docker image build time** (build arg / baked deploy key or token), so the
  running container can push without per-user remote credentials. Per-user
  attribution is still preserved through commit authorship (§8).
- Docker image for self-hosting.

## 12. ADRs to create during implementation

These are architecturally significant and warrant ADRs (created in the same
commits as the code):

1. Web client architecture: extracted `tolaria-core` + Axum server + shared
   React frontend over an HTTP/WS transport.
2. Web transport seam: Tauri-module aliasing for the web build.
3. Multi-user auth & session model for the web server.
4. Optimistic-concurrency save protocol (version token + `409` → conflict UI).
5. Single-working-tree git model for a multi-user web peer.

## 13. Phased implementation

1. **Extract `tolaria-core`** from `src-tauri`; desktop app keeps passing all
   gates on the refactored core.
2. **Minimal server + transport shim** for read-only browse (list/content/
   search) — proves the seam end-to-end in a real browser.
3. **Auth** (user store, login/logout, session cookie, per-user git identity).
4. **Write path + optimistic concurrency** (save with version token, `409` →
   conflict UI, per-path locks).
5. **Git sync controls** (commit/push/pull/status, repo lock, conflict
   resolution, autogit-style debounced commits).

Each phase is independently shippable and testable, and each gets its own
implementation-plan slice.
