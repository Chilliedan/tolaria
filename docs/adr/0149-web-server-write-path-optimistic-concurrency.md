---
type: ADR
id: "0149"
title: "Write path and optimistic concurrency for the web server (tolaria-server, Phase 4)"
status: active
date: 2026-07-02
---

## Context

ADR-0147 shipped a read-only `tolaria-server`; ADR-0148 added built-in
authentication in front of it. The web client design spec's Phase 4 requires
the server to accept note edits from the browser: saving content, creating
and deleting notes, renaming notes, and editing frontmatter — the same
mutations the desktop app performs locally through `tolaria-core`.

Two problems are new to a networked, multi-request write path that don't
exist for a single desktop process with one editor:

1. **Lost updates.** Two browser tabs (or two collaborators) can load the
   same note, edit concurrently, and save; without a check, the second save
   silently clobbers the first with no warning.
2. **Cross-site request forgery.** Once the server accepts state-changing
   POSTs from an authenticated session cookie, any page the user's browser
   loads could forge a write request unless the server can distinguish
   same-origin requests from cross-site ones.

Constraints carried over from Phase 2/3:

1. **`tolaria-core` stays the single source of vault logic.** Write commands
   call the same `tolaria_core::vault` / `tolaria_core::frontmatter`
   functions the desktop app uses; `tolaria-server` does not reimplement
   file-mutation logic.
2. **Vault containment is non-negotiable.** Every write path must reject
   requests whose resolved path escapes the configured vault root, exactly
   as the read path already does.
3. **Single web client codebase.** The React SPA's save/create/rename/delete
   flows must work unmodified against the web transport; the web-specific
   concurrency and CSRF handling live in the transport shim, not in
   component code.

## Decision

### Write command set

`write_handlers::dispatch_write` is a new async dispatcher, separate from
the existing synchronous read-only `handlers::dispatch`. `command_route`
checks `is_write_command(command)` and routes accordingly. The whitelisted
write commands are: `save_note_content`, `create_note` /
`create_note_content`, `rename_note`, `rename_note_filename`, `delete_note`,
`update_frontmatter`, and `delete_frontmatter_property`. Any other command
is still rejected (`501`, per ADR-0147). Every handler resolves its target
path against `AppState::vault_root` via `contained_note_path` (existing
files) or `contained_note_path_for_write` (parent-confined, for paths that
don't exist yet, i.e. create/first save) before touching the filesystem —
the client-supplied path is never trusted directly.

### Optimistic concurrency: sha256 version + per-path lock + `409`

Rather than server-side record locking or CRDT merge, saves use
compare-and-set on a content hash:

- **Version token**: `content_version(content)` is the lowercase-hex SHA-256
  of the note's UTF-8 bytes, computed identically on the server
  (`version.rs`, the `sha2` crate) and on the client (Web Crypto
  `SHA-256`). Either side can compute the current version without a
  round-trip, and the two implementations are verified to agree.
- **Compare-and-set**: `save_note_content` accepts an optional `baseHash` —
  the version the client last loaded. If the file exists and its current
  on-disk content hash no longer equals `baseHash`, the write is rejected.
- **Reject-stale-and-reload, never silent overwrite**: a stale `baseHash`
  returns HTTP `409 Conflict` with a JSON body
  `{ "error": "conflict", "currentContent": <current on-disk content> }`.
  The rejected content is never written. The client is expected to show the
  conflict and let the user reconcile against `currentContent`, rather than
  the server attempting an automatic merge.
- **Per-path lock**: `locks::PathLocks` hands out one `tokio::sync::Mutex`
  per canonicalized path from a registry behind a `std::sync::Mutex`-guarded
  `HashMap`. Every write handler acquires that note's lock before checking
  `baseHash` and holds it through the write, so the check-then-write is
  atomic per file and concurrent writes to *different* files never block
  each other.
- **Transport-transparent**: the React app and its save/create/rename/delete
  call sites are unchanged. Version tracking and conflict handling live in
  the web transport shim (`src/web/`) and the corresponding Tauri command
  surface, not in feature components — the same component code runs
  against either backend.

This was chosen over pessimistic locking (would require a lock-acquire/
release protocol and timeout/lease handling for a browser client that can
disconnect at any time) and over automatic merge (out of scope for
free-form Markdown notes; conflict resolution for *git* history is
explicitly deferred to Phase 5 and follows a different, git-native model).

### CSRF: double-submit cookie

`POST /api/cmd/*` now performs state-changing writes for authenticated
sessions, so it needs CSRF protection beyond the `SameSite=Lax` session
cookie alone. `csrf.rs` implements the double-submit pattern: login sets a
non-HttpOnly `tolaria_csrf` cookie (readable by same-origin JS, unlike the
`HttpOnly` session cookie); the client reads it and echoes it in the
`X-CSRF-Token` request header on every `/api/cmd/*` call. `command_route`
calls `csrf::verify`, which requires the cookie and header to both be
present, non-empty, and equal, and rejects the request with `403` otherwise.
A cross-site page can trigger the cookie to be sent automatically but cannot
read its value to populate the header, which is what makes the pattern
effective without server-side per-request token storage.

### Vault mount: read-write

`docker-compose.yml`'s `tolaria-web` service now mounts the vault volume
read-write (previously `:ro`, per ADR-0147/Phase 2) since write commands
must persist to the same files the read path serves.

## Consequences

- The web client can now save, create, rename, delete notes, and edit
  frontmatter — feature parity with the desktop app's local file mutations,
  modulo git.
- Concurrent edits to the same note are detected and surfaced to the user
  as a conflict instead of being silently lost; concurrent edits to
  *different* notes are unaffected by each other's locks.
- `PathLocks` is in-memory and per-process: it does not protect against a
  second `tolaria-server` process (e.g. a second container) writing the
  same mounted vault. Single-process deployment remains an assumption
  carried from earlier phases.
- CSRF protection depends on the `tolaria_csrf` cookie being deliverable and
  readable by JS on the same origin; this is unaffected by the `HttpOnly`
  session cookie's own protections and does not weaken them.
- **Git commit, push, pull, and remote conflict resolution remain out of
  scope.** Files written through this path land on disk inside the mounted
  vault but are **not** committed to git by the server. Attribution
  (`git_name`/`git_email`, stored since ADR-0148) stays inert until Phase 5
  wires up commits; until then, persisting and pushing changes to the vault
  repository is the operator's responsibility (e.g. running git manually
  against the mounted volume, or Phase 5 landing this automatically).
