# Web Client — Phase 4: Write Path + Optimistic Concurrency Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the authenticated read-only web server into an editor: enable note save/create/rename/delete and frontmatter writes over `tolaria-core`, made concurrency-safe with an optimistic version token (per-path compare-and-set → `409` on stale writes) and protected against CSRF with a double-submit token — all persisted to disk (git commit/push stays Phase 5).

**Architecture:** A new async write path in `tolaria-server`: `command_route` routes write commands to `write_handlers::dispatch_write` (async, under a per-path lock from a `PathLocks` registry in `AppState`), while reads keep the existing sync `handlers::dispatch`. `save_note_content` carries a `baseHash` (sha256 of the content the client last read); the server compares it to the sha256 of the current on-disk content under the lock and rejects a mismatch with `409 {currentContent}`. The web transport tracks per-path sha256 transparently (so the React app is unchanged), sends `baseHash` on save, sends a double-submit `X-CSRF-Token` header on every `/api/cmd` call, and on `409` warns + reloads. All write paths reuse the Phase-2 vault-containment guard.

**Tech Stack:** Rust/Axum 0.7 (`tolaria-server`), `sha2` (content version), `tokio::sync::Mutex` (per-path locks), existing `tolaria-core` write functions; Web Crypto `crypto.subtle` (client sha256).

**Spec:** `docs/superpowers/specs/2026-06-29-web-client-design.md` (§7 concurrency, §13 Phase 4)

## Global Constraints

- Rust edition `2021`, `rust-version = "1.77.2"`. `tolaria-server` uses `tolaria-core` only for vault behavior — no reimplementation.
- **Files-only:** Phase 4 writes/renames/deletes files on disk. NO git commit/push/pull (Phase 5). Do not call `git_commit` etc.
- **Concurrency safety:** every note save is a per-path compare-and-set. A stale `baseHash` → `409` with the current content; the write is rejected (never silently overwrites). Saves to different paths run concurrently; saves to the same path serialize.
- **Content version = lowercase hex sha256 of the UTF-8 content bytes**, computed identically on client (Web Crypto `SHA-256`) and server (`sha2::Sha256`).
- **CSRF:** double-submit. A non-HttpOnly `tolaria_csrf` cookie (SameSite=Lax, Secure per `cookie_secure`, Path=/) is set at login; the client echoes it in the `X-CSRF-Token` header on every `/api/cmd/*` request; the server rejects `/api/cmd/*` requests whose header ≠ cookie with `403`.
- **Vault containment (Phase 2) applies to every write** — canonicalize the target path and require it inside the vault root; reject otherwise.
- Auth (Phase 3) unchanged — writes are behind `require_auth` like all `/api/cmd/*`.
- Coverage/CodeScene/Codacy run via the pre-push sidecar; per-task gates are `cargo test`/`clippy`/`vitest`. **⛔ NEVER `--no-verify`, lower thresholds, or add `#[allow(...)]`/`as any`.**
- Commit after every task.

## Carried-forward prerequisites (from Phase 3 final review — MUST land in this phase)
- CSRF protection for state-changing requests (Task 4 — the core of this phase).
- `useradd` must require a non-empty git email (Task 6).

## Current seam (verified)
- `handlers::dispatch(vault_root: &Path, command: &str, args: Value) -> Result<Value, RpcError>` (sync, reads). `contained_note_path(vault_root, requested) -> Result<PathBuf, RpcError>` is private in `handlers.rs`.
- `rpc::command_route(State<AppState>, Path<String>, Option<Json<Value>>)` calls `handlers::dispatch(&state.vault_root, ...)`. `AppState { vault_root, users, sessions, cookie_secure }`.
- `auth_routes::login` sets the `tolaria_session` cookie. `SESSION_COOKIE` const there.
- `users::run_useradd(users, username, password, git_name, git_email)` currently guards only username/password.
- tolaria-core writes: `vault::{save_note_content(path:&str,content:&str), create_note_content, note_content_matches, delete_note(path:&str), rename_note(RenameNoteRequest), rename_note_filename(RenameNoteFilenameRequest)}`, `frontmatter::{update_frontmatter(path,key,value), delete_frontmatter_property(path,key), FrontmatterValue}`.
- Docker compose currently mounts the vault `:ro` — MUST become read-write (Task 7).

## File structure (target)
```
src-tauri/crates/tolaria-server/
  Cargo.toml            # + sha2
  src/
    locks.rs            # NEW: PathLocks (per-path async mutex registry)
    version.rs          # NEW: content_version(&str) -> String (sha256 hex)
    write_handlers.rs   # NEW: is_write_command, dispatch_write (async), optimistic save + create/rename/delete/frontmatter
    csrf.rs             # NEW: CSRF_COOKIE, X-CSRF-Token, token gen + verify
    handlers.rs         # contained_note_path -> pub(crate); (reads unchanged)
    rpc.rs              # AppState + PathLocks; command_route: CSRF check + route writes vs reads
    auth_routes.rs      # login also sets the csrf cookie
    users.rs            # run_useradd requires git_email
    lib.rs              # module decls
src/web/transport.ts    # csrf header + per-path version tracking + baseHash on save + 409 handling
Dockerfile / docker-compose.yml   # vault mounted read-write
docs/adr/0149-web-server-write-path-optimistic-concurrency.md
docs/ARCHITECTURE.md
```

---

### Task 1: `sha2` dep, content version, per-path lock registry

**Files:**
- Modify: `Cargo.toml` (+ `sha2`)
- Create: `src/version.rs`, `src/locks.rs`
- Modify: `src/lib.rs` (`pub mod version; pub mod locks;`)

**Interfaces:**
- Produces: `version::content_version(content: &str) -> String` (lowercase hex sha256 of `content.as_bytes()`). `locks::PathLocks` (`Clone`, `Default`) with `async fn lock(&self, path: &Path) -> tokio::sync::OwnedMutexGuard<()>` returning a held guard; the same path returns the same underlying mutex, different paths are independent.

- [ ] **Step 1: Add sha2**

In `Cargo.toml [dependencies]`: `sha2 = "0.10"`.

- [ ] **Step 2: content version (TDD)**

Create `src/version.rs`:

```rust
use sha2::{Digest, Sha256};

/// Lowercase hex sha256 of the content's UTF-8 bytes. Must match the client's
/// Web Crypto SHA-256 over the same bytes.
pub fn content_version(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_and_lowercase_hex_64() {
        let v = content_version("hello");
        assert_eq!(v, content_version("hello"));
        assert_eq!(v.len(), 64);
        assert!(v.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn differs_on_change() {
        assert_ne!(content_version("a"), content_version("b"));
    }

    #[test]
    fn known_vector() {
        // sha256("") = e3b0c442...
        assert_eq!(&content_version("")[..8], "e3b0c442");
    }
}
```

- [ ] **Step 3: per-path locks (TDD)**

Create `src/locks.rs`:

```rust
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

/// Registry of per-path async mutexes so writes to the same file serialize
/// while writes to different files run concurrently. Cheap to clone.
#[derive(Clone, Default)]
pub struct PathLocks {
    inner: Arc<Mutex<HashMap<PathBuf, Arc<AsyncMutex<()>>>>>,
}

impl PathLocks {
    pub fn new() -> Self {
        Self::default()
    }

    fn mutex_for(&self, path: &Path) -> Arc<AsyncMutex<()>> {
        let mut map = self.inner.lock().expect("path locks poisoned");
        map.entry(path.to_path_buf())
            .or_insert_with(|| Arc::new(AsyncMutex::new(())))
            .clone()
    }

    /// Acquire the lock for `path`, awaiting if another writer holds it.
    pub async fn lock(&self, path: &Path) -> OwnedMutexGuard<()> {
        self.mutex_for(path).lock_owned().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_path_shares_one_mutex() {
        let locks = PathLocks::new();
        let a = locks.mutex_for(Path::new("/v/n.md"));
        let b = locks.mutex_for(Path::new("/v/n.md"));
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn different_paths_independent() {
        let locks = PathLocks::new();
        let a = locks.mutex_for(Path::new("/v/a.md"));
        let b = locks.mutex_for(Path::new("/v/b.md"));
        assert!(!Arc::ptr_eq(&a, &b));
    }

    #[tokio::test]
    async fn lock_then_release_allows_reacquire() {
        let locks = PathLocks::new();
        let g = locks.lock(Path::new("/v/n.md")).await;
        drop(g);
        let _g2 = locks.lock(Path::new("/v/n.md")).await; // must not deadlock
    }
}
```

Add `pub mod version;` and `pub mod locks;` to `src/lib.rs`.

- [ ] **Step 4: test + commit**

Run: `cargo test --manifest-path /Users/daniel/Projects/tolaria/src-tauri/Cargo.toml -p tolaria-server version::` then `... locks::`
Expected: PASS.
```bash
git add src-tauri/crates/tolaria-server/Cargo.toml src-tauri/crates/tolaria-server/src/version.rs src-tauri/crates/tolaria-server/src/locks.rs src-tauri/crates/tolaria-server/src/lib.rs
git commit -m "feat(server): content-version (sha256) + per-path lock registry"
```

---

### Task 2: AppState gains PathLocks; optimistic `save_note_content`

**Files:**
- Modify: `src/rpc.rs` (AppState + `command_route` routes writes), `src/handlers.rs` (`contained_note_path` → `pub(crate)`)
- Create: `src/write_handlers.rs`
- Modify: `src/lib.rs` (`pub mod write_handlers;`)

**Interfaces:**
- Produces: `AppState { vault_root, users, sessions, cookie_secure, locks: PathLocks }` + updated `AppState::new(...)` (adds `locks`); `write_handlers::is_write_command(&str) -> bool`; `write_handlers::dispatch_write(state: &AppState, command: &str, args: Value) -> Result<Value, RpcError>` (async). `handlers::contained_note_path` becomes `pub(crate)`.
- Save protocol: `save_note_content { path, content, baseHash? }`. Under the per-path lock: if the file exists and `baseHash` is `Some` and `content_version(current) != baseHash` → `RpcError` with status `409 CONFLICT` and body `{ "error": "conflict", "currentContent": <string> }`. Else write via `tolaria_core::vault::save_note_content` and return `{ "version": content_version(content) }`.

- [ ] **Step 1: Make containment reusable**

In `handlers.rs`, change `fn contained_note_path` to `pub(crate) fn contained_note_path`.

- [ ] **Step 2: AppState + locks**

In `rpc.rs`, add `pub locks: crate::locks::PathLocks` to `AppState` and to `AppState::new` (append a `locks: PathLocks` param; construct callers pass `PathLocks::new()`). Add a `409` helper to `RpcError`:

```rust
impl RpcError {
    pub fn conflict_with_current(current: &str) -> Self {
        Self { status: StatusCode::CONFLICT, message: format!("__CONFLICT__{current}") }
    }
}
```

> Simpler: give `RpcError` an optional structured body. To keep it minimal, add a dedicated conflict response in `write_handlers` instead (next step) rather than overloading `RpcError`'s string. Implement conflict as a direct `Response` in `dispatch_write`'s caller — see Step 4.

- [ ] **Step 3: Write handler (TDD)**

Create `src/write_handlers.rs`:

```rust
use crate::handlers::contained_note_path;
use crate::rpc::{AppState, RpcError};
use crate::version::content_version;
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use tolaria_core::vault;

pub fn is_write_command(command: &str) -> bool {
    matches!(
        command,
        "save_note_content"
            | "create_note"
            | "create_note_content"
            | "rename_note"
            | "rename_note_filename"
            | "delete_note"
            | "update_frontmatter"
            | "delete_frontmatter_property"
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveArgs {
    path: PathBuf,
    content: String,
    base_hash: Option<String>,
}

fn parse<T: serde::de::DeserializeOwned>(args: Value) -> Result<T, RpcError> {
    serde_json::from_value(args).map_err(|e| RpcError::bad_request(format!("bad arguments: {e}")))
}

pub async fn dispatch_write(state: &AppState, command: &str, args: Value) -> Result<Value, RpcError> {
    match command {
        "save_note_content" => save_note_content(state, args).await,
        // other write arms added in Task 3
        other => Err(crate::rpc::unsupported(other)),
    }
}

async fn save_note_content(state: &AppState, args: Value) -> Result<Value, RpcError> {
    let a: SaveArgs = parse(args)?;
    let safe = contained_note_path_for_write(&state.vault_root, &a.path)?;
    let _guard = state.locks.lock(&safe).await;

    if let Some(base) = &a.base_hash {
        if safe.exists() {
            let current = vault::get_note_content(&safe).map_err(RpcError::internal)?;
            if &content_version(&current) != base {
                return Err(RpcError {
                    status: StatusCode::CONFLICT,
                    message: serde_json::to_string(&json!({ "error": "conflict", "currentContent": current }))
                        .unwrap_or_else(|_| "{\"error\":\"conflict\"}".to_string()),
                });
            }
        }
    }
    let path_str = safe.to_string_lossy().to_string();
    vault::save_note_content(&path_str, &a.content).map_err(RpcError::internal)?;
    Ok(json!({ "version": content_version(&a.content) }))
}

/// For writes the file may not exist yet (create/first save), so canonicalize
/// the PARENT and confine, rather than requiring the file itself to exist.
pub(crate) fn contained_note_path_for_write(vault_root: &std::path::Path, requested: &std::path::Path) -> Result<PathBuf, RpcError> {
    let root = std::fs::canonicalize(vault_root)
        .map_err(|e| RpcError::internal(format!("vault root error: {e}")))?;
    let parent = requested.parent().unwrap_or(requested);
    let canon_parent = std::fs::canonicalize(parent)
        .map_err(|_| RpcError::bad_request("path parent is invalid"))?;
    if !canon_parent.starts_with(&root) {
        return Err(RpcError::bad_request("path is outside the vault"));
    }
    let file_name = requested.file_name().ok_or_else(|| RpcError::bad_request("invalid file name"))?;
    Ok(canon_parent.join(file_name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn state_for(dir: &std::path::Path) -> AppState {
        AppState::new(
            dir.to_path_buf(),
            crate::users::UsersDb::open(&dir.join("u.db")).unwrap(),
            crate::session::SessionStore::new(std::time::Duration::from_secs(60)),
            false,
            crate::locks::PathLocks::new(),
        )
    }

    #[tokio::test]
    async fn save_without_base_hash_writes_and_returns_version() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("n.md");
        fs::write(&p, "old").unwrap();
        let out = save_note_content(&state_for(dir.path()), json!({ "path": p, "content": "new" })).await.unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "new");
        assert_eq!(out["version"], json!(content_version("new")));
    }

    #[tokio::test]
    async fn save_with_matching_base_hash_succeeds() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("n.md");
        fs::write(&p, "old").unwrap();
        let base = content_version("old");
        let out = save_note_content(&state_for(dir.path()), json!({ "path": p, "content": "new", "baseHash": base })).await.unwrap();
        assert_eq!(out["version"], json!(content_version("new")));
        assert_eq!(fs::read_to_string(&p).unwrap(), "new");
    }

    #[tokio::test]
    async fn save_with_stale_base_hash_conflicts_and_does_not_write() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("n.md");
        fs::write(&p, "server-changed").unwrap();
        let err = save_note_content(
            &state_for(dir.path()),
            json!({ "path": p, "content": "my-edit", "baseHash": content_version("what-i-loaded") }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.status, StatusCode::CONFLICT);
        assert!(err.message.contains("server-changed"));
        assert_eq!(fs::read_to_string(&p).unwrap(), "server-changed"); // NOT overwritten
    }

    #[tokio::test]
    async fn save_outside_vault_rejected() {
        let dir = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let p = outside.path().join("evil.md");
        let err = save_note_content(&state_for(dir.path()), json!({ "path": p, "content": "x" })).await.unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }
}
```

> Note: `save_note_content` uses `contained_note_path_for_write` (parent-based) so first-time saves/creates to a not-yet-existing file are allowed while still confined to the vault. The read path keeps using `handlers::contained_note_path` (requires the file to exist).

- [ ] **Step 4: Route writes in `command_route`, with the conflict body**

In `rpc.rs`, update `command_route` so write commands go through the async write dispatch and a `409` renders its JSON body correctly:

```rust
pub async fn command_route(
    State(state): State<AppState>,
    AxumPath(command): AxumPath<String>,
    body: Option<Json<Value>>,
) -> Response {
    let args = body.map(|Json(v)| v).unwrap_or(Value::Null);
    let result = if crate::write_handlers::is_write_command(&command) {
        crate::write_handlers::dispatch_write(&state, &command, args).await
    } else {
        crate::handlers::dispatch(&state.vault_root, &command, args)
    };
    match result {
        Ok(value) => ok_json(value),
        Err(err) if err.status == StatusCode::CONFLICT => {
            // message is the JSON body {"error":"conflict","currentContent":...}
            let body: Value = serde_json::from_str(&err.message)
                .unwrap_or_else(|_| json!({ "error": "conflict" }));
            (StatusCode::CONFLICT, axum::Json(body)).into_response()
        }
        Err(err) => err.into_response(),
    }
}
```

Add `pub mod write_handlers;` to `lib.rs`. Update every `AppState::new(...)` call site (build_router in lib.rs, and test helpers in rpc/auth tests) to pass `PathLocks::new()`.

- [ ] **Step 5: build + full server suite + commit**

Run: `cargo test --manifest-path /Users/daniel/Projects/tolaria/src-tauri/Cargo.toml -p tolaria-server`
Expected: PASS — new save tests + all prior tests (with the extra AppState field).
```bash
git add -A src-tauri/crates/tolaria-server
git commit -m "feat(server): optimistic save_note_content with per-path lock + 409 conflict"
```

---

### Task 3: Remaining write commands (create, rename, delete, frontmatter)

**Files:**
- Modify: `src/write_handlers.rs` (add arms + tests)

**Interfaces:**
- Produces: `dispatch_write` arms for `create_note_content`/`create_note` (`{path, content?}`), `delete_note` (`{path}`), `rename_note` / `rename_note_filename` (map to `tolaria_core::vault::RenameNoteRequest` / `RenameNoteFilenameRequest`), `update_frontmatter` (`{path, key, value}`), `delete_frontmatter_property` (`{path, key}`). All confine paths (`contained_note_path_for_write` for create; `contained_note_path` for existing-file ops) and take the per-path lock.

- [ ] **Step 1: Inspect the exact rename/frontmatter arg shapes the frontend sends**

Run:
```bash
grep -rhnE "invoke[^)]*(create_note|rename_note|rename_note_filename|delete_note|update_frontmatter|delete_frontmatter_property)" /Users/daniel/Projects/tolaria/src | head -30
grep -nA6 "pub struct RenameNoteRequest|pub struct RenameNoteFilenameRequest" /Users/daniel/Projects/tolaria/src-tauri/crates/tolaria-core/src/vault/rename.rs
```
Record the JS arg keys (camelCase) and the core request-struct fields so the arms deserialize correctly.

- [ ] **Step 2: Add the arms (TDD)**

For each command add a `dispatch_write` arm + a temp-vault test. Concrete examples for the two simplest; follow the same shape (confine → lock → core call → JSON result) for the rest using the arg shapes from Step 1:

```rust
        "create_note_content" | "create_note" => {
            #[derive(serde::Deserialize)]
            struct CreateArgs { path: std::path::PathBuf, #[serde(default)] content: String }
            let a: CreateArgs = parse(args)?;
            let safe = contained_note_path_for_write(&state.vault_root, &a.path)?;
            let _g = state.locks.lock(&safe).await;
            vault::create_note_content(&safe.to_string_lossy(), &a.content).map_err(RpcError::internal)?;
            Ok(json!({ "path": safe.to_string_lossy(), "version": content_version(&a.content) }))
        }
        "delete_note" => {
            #[derive(serde::Deserialize)]
            struct DelArgs { path: std::path::PathBuf }
            let a: DelArgs = parse(args)?;
            let safe = contained_note_path(&state.vault_root, &a.path)?;
            let _g = state.locks.lock(&safe).await;
            let removed = vault::delete_note(&safe.to_string_lossy()).map_err(RpcError::internal)?;
            Ok(json!(removed))
        }
```

For `rename_note` / `rename_note_filename` build the core request struct from the deserialized args (confine both the old path and, for renames producing a new path, the resulting path's parent). For `update_frontmatter` / `delete_frontmatter_property` confine the path, lock, then call `tolaria_core::frontmatter::{update_frontmatter, delete_frontmatter_property}` (note `update_frontmatter` takes a `FrontmatterValue` — deserialize the JS `value` into it). Add one temp-vault test per arm asserting the on-disk effect (created file exists; deleted file gone; rename moved content; frontmatter key present/absent).

- [ ] **Step 3: test + commit**

Run: `cargo test --manifest-path /Users/daniel/Projects/tolaria/src-tauri/Cargo.toml -p tolaria-server write_handlers`
Expected: PASS — all write arms covered.
```bash
git add src-tauri/crates/tolaria-server/src/write_handlers.rs
git commit -m "feat(server): create/rename/delete/frontmatter write commands"
```

---

### Task 4: CSRF double-submit token

**Files:**
- Create: `src/csrf.rs`
- Modify: `src/auth_routes.rs` (login sets the csrf cookie), `src/rpc.rs` (`command_route` enforces CSRF), `src/lib.rs` (`pub mod csrf;`)

**Interfaces:**
- Produces: `csrf::{CSRF_COOKIE: &str = "tolaria_csrf", CSRF_HEADER: &str = "x-csrf-token", generate_token() -> String, csrf_cookie(value, secure) -> Cookie}` and `csrf::verify(jar: &CookieJar, headers: &HeaderMap) -> bool` (true iff both present and equal).
- Behavior: login sets a fresh `tolaria_csrf` cookie (non-HttpOnly, SameSite=Lax, Secure per `cookie_secure`, Path=/). `command_route` rejects any `/api/cmd/*` request whose `X-CSRF-Token` header is missing or ≠ the `tolaria_csrf` cookie with `403 {"error":"csrf"}`.

- [ ] **Step 1: csrf module (TDD)**

Create `src/csrf.rs`:

```rust
use axum::http::HeaderMap;
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use rand::rngs::OsRng;
use rand::RngCore;

pub const CSRF_COOKIE: &str = "tolaria_csrf";
pub const CSRF_HEADER: &str = "x-csrf-token";

pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Non-HttpOnly so the browser JS can read it and echo it in the header.
pub fn csrf_cookie(value: String, secure: bool) -> Cookie<'static> {
    Cookie::build((CSRF_COOKIE, value))
        .http_only(false)
        .same_site(SameSite::Lax)
        .secure(secure)
        .path("/")
        .build()
}

pub fn verify(jar: &CookieJar, headers: &HeaderMap) -> bool {
    let cookie = jar.get(CSRF_COOKIE).map(|c| c.value().to_string());
    let header = headers.get(CSRF_HEADER).and_then(|h| h.to_str().ok()).map(str::to_string);
    match (cookie, header) {
        (Some(c), Some(h)) => !c.is_empty() && c == h,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn jar_with(token: &str) -> CookieJar {
        CookieJar::new().add(Cookie::new(CSRF_COOKIE, token.to_string()))
    }
    fn headers_with(token: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(CSRF_HEADER, HeaderValue::from_str(token).unwrap());
        h
    }

    #[test]
    fn matching_passes() { assert!(verify(&jar_with("abc"), &headers_with("abc"))); }
    #[test]
    fn mismatch_fails() { assert!(!verify(&jar_with("abc"), &headers_with("xyz"))); }
    #[test]
    fn missing_header_fails() { assert!(!verify(&jar_with("abc"), &HeaderMap::new())); }
    #[test]
    fn missing_cookie_fails() { assert!(!verify(&CookieJar::new(), &headers_with("abc"))); }
    #[test]
    fn token_is_64_hex() { let t = generate_token(); assert_eq!(t.len(), 64); }
}
```

Add `pub mod csrf;` to `lib.rs`.

- [ ] **Step 2: login sets the csrf cookie**

In `auth_routes.rs` `login`, on the success branch, add the csrf cookie alongside the session cookie:

```rust
            let jar = jar
                .add(session_cookie(token, state.cookie_secure))
                .add(crate::csrf::csrf_cookie(crate::csrf::generate_token(), state.cookie_secure));
```

- [ ] **Step 3: enforce CSRF in `command_route` (TDD via HTTP)**

In `rpc.rs`, add `jar: CookieJar` and `headers: HeaderMap` to `command_route`'s args and check before dispatch:

```rust
pub async fn command_route(
    State(state): State<AppState>,
    AxumPath(command): AxumPath<String>,
    jar: axum_extra::extract::cookie::CookieJar,
    headers: axum::http::HeaderMap,
    body: Option<Json<Value>>,
) -> Response {
    if !crate::csrf::verify(&jar, &headers) {
        return (StatusCode::FORBIDDEN, axum::Json(json!({ "error": "csrf" }))).into_response();
    }
    // ... existing routing ...
}
```

Add an integration test in `tests/rpc_http.rs`: an authenticated `POST /api/cmd/list_vault` WITHOUT the csrf cookie+header → `403`; WITH matching csrf cookie + `X-CSRF-Token` header → `200`. (Update the existing authed test helper to also set a csrf cookie + header so it keeps passing.)

- [ ] **Step 4: test + commit**

Run: `cargo test --manifest-path /Users/daniel/Projects/tolaria/src-tauri/Cargo.toml -p tolaria-server`
Expected: PASS — csrf unit tests + updated/added HTTP tests.
```bash
git add -A src-tauri/crates/tolaria-server
git commit -m "feat(server): double-submit CSRF protection on /api/cmd"
```

---

### Task 5: Frontend transport — CSRF header, version tracking, 409 handling

**Files:**
- Modify: `src/web/transport.ts`, `src/web/transport.test.ts`

**Interfaces:**
- Behavior: transport reads the `tolaria_csrf` cookie and sends it as `X-CSRF-Token` on every `/api/cmd/*` request. It maintains `Map<path, sha256hex>`: on `get_note_content` responses (string) it stores `await sha256(content)`; on `save_note_content` it injects `baseHash` from the map (if known) and, on `200`, updates the map to `await sha256(savedContent)`. On `409` it warns the user and reloads (discarding the stale edit).

- [ ] **Step 1: Write the failing tests**

Add to `src/web/transport.test.ts`:

```ts
  it('sends X-CSRF-Token header from the tolaria_csrf cookie', async () => {
    vi.stubGlobal('document', { cookie: 'tolaria_csrf=tok123; other=x' } as unknown as Document)
    const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify(['a']), { status: 200 }))
    vi.stubGlobal('fetch', fetchMock)
    await invoke('list_vault', { path: '/v' })
    const init = fetchMock.mock.calls[0][1] as RequestInit
    expect((init.headers as Record<string, string>)['X-CSRF-Token']).toBe('tok123')
  })

  it('sends baseHash on save from a prior get_note_content and reloads on 409', async () => {
    vi.stubGlobal('document', { cookie: 'tolaria_csrf=t' } as unknown as Document)
    // First: read content so the transport records its hash.
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify('hello'), { status: 200 })))
    await invoke('get_note_content', { path: '/v/n.md' })
    // Then a 409 save → warns + reloads, returns undefined.
    const reload = vi.fn()
    vi.stubGlobal('window', { location: { reload, assign: vi.fn() }, alert: vi.fn() })
    const saveFetch = vi.fn().mockResolvedValue(new Response(JSON.stringify({ error: 'conflict', currentContent: 'server' }), { status: 409 }))
    vi.stubGlobal('fetch', saveFetch)
    const result = await invoke('save_note_content', { path: '/v/n.md', content: 'mine' })
    const body = JSON.parse((saveFetch.mock.calls[0][1] as RequestInit).body as string)
    expect(body.baseHash).toBeDefined()
    expect(result).toBeUndefined()
    expect(reload).toHaveBeenCalled()
  })
```

- [ ] **Step 2: Implement**

In `src/web/transport.ts` add helpers and wire them into `invoke`:

```ts
const noteVersions = new Map<string, string>()

async function sha256Hex(text: string): Promise<string> {
  const buf = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text))
  return Array.from(new Uint8Array(buf)).map((b) => b.toString(16).padStart(2, '0')).join('')
}

function csrfToken(): string {
  if (typeof document === 'undefined') return ''
  const m = document.cookie.match(/(?:^|;\s*)tolaria_csrf=([^;]+)/)
  return m ? m[1] : ''
}
```

In `invoke`, build headers with the CSRF token; for `save_note_content` inject `baseHash` from `noteVersions`; after decoding responses, maintain the map and handle `409`. The core of `invoke` becomes:

```ts
export async function invoke<T = unknown>(command: string, args?: Record<string, unknown>): Promise<T> {
  const path = typeof args?.path === 'string' ? (args.path as string) : undefined
  let body = { ...(args ?? {}) }
  if (command === 'save_note_content' && path && noteVersions.has(path)) {
    body = { ...body, baseHash: noteVersions.get(path) }
  }
  const res = await fetch(`/api/cmd/${command}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', 'X-CSRF-Token': csrfToken() },
    body: JSON.stringify(body),
  })
  if (res.status === 501) return undefined as T
  if (res.status === 401) { if (typeof window !== 'undefined') window.location.assign('/login'); return undefined as T }
  if (res.status === 409) {
    if (typeof window !== 'undefined') {
      window.alert('This note changed on the server. Reloading the latest version — your last change was not saved.')
      window.location.reload()
    }
    return undefined as T
  }
  if (!res.ok) {
    const detail = await res.json().catch(() => ({ error: res.statusText }))
    throw new Error(typeof (detail as { error?: string })?.error === 'string' ? (detail as { error: string }).error : `invoke ${command} failed`)
  }
  const data = (await res.json()) as T
  if (command === 'get_note_content' && path && typeof data === 'string') {
    noteVersions.set(path, await sha256Hex(data))
  }
  if (command === 'save_note_content' && path) {
    noteVersions.set(path, await sha256Hex(String((args as { content?: unknown }).content ?? '')))
  }
  return data
}
```

> The reload-on-409 is deliberately simple (warn + `location.reload()` discards the stale edit — safe, matches the chosen "reject stale + reload" behavior). A future refinement can swap the full reload for an in-place editor refresh using `currentContent` from the 409 body.

- [ ] **Step 3: test + commit**

Run: `pnpm vitest run src/web/transport.test.ts`
Expected: PASS — all transport cases (200, 501, 401, csrf header, save baseHash + 409 reload).
```bash
git add src/web/transport.ts src/web/transport.test.ts
git commit -m "feat(web): CSRF header, optimistic save baseHash, 409 reload"
```

---

### Task 6: `useradd` requires a non-empty git email (carried prerequisite)

**Files:**
- Modify: `src/users.rs` (`run_useradd` validation + test)

- [ ] **Step 1: Tighten validation (TDD)**

In `users.rs` `run_useradd`, extend the empty check to reject an empty `git_email`:

```rust
    if username.trim().is_empty() || password.is_empty() || git_email.trim().is_empty() {
        return Err("username, password, and git email must not be empty".to_string());
    }
```

Add a test:

```rust
    #[test]
    fn run_useradd_rejects_empty_git_email() {
        let db = UsersDb::open_in_memory().unwrap();
        assert!(run_useradd(&db, "eve", "pw", "Eve", "").is_err());
    }
```

- [ ] **Step 2: test + commit**

Run: `cargo test --manifest-path /Users/daniel/Projects/tolaria/src-tauri/Cargo.toml -p tolaria-server run_useradd`
Expected: PASS.
```bash
git add src-tauri/crates/tolaria-server/src/users.rs
git commit -m "fix(server): useradd requires non-empty git email"
```

---

### Task 7: Docker vault read-write, ADR, docs, gates

**Files:**
- Modify: `docker-compose.yml` (vault mount `:ro` → read-write)
- Create: `docs/adr/0149-web-server-write-path-optimistic-concurrency.md`
- Modify: `docs/ARCHITECTURE.md`

- [ ] **Step 1: Vault mount read-write**

In `docker-compose.yml`, change the vault volume from `:/vault:ro` to `:/vault` (read-write) so writes persist. (Keep the users-DB volume as-is.) Add a one-line comment that Phase 4 needs write access.

- [ ] **Step 2: ADR 0149**

Create `docs/adr/0149-web-server-write-path-optimistic-concurrency.md` (frontmatter matching `0148`: `type: ADR`, `id: "0149"`, `status: active`, `date: 2026-07-02`). Record: write commands enabled over `tolaria-core` (save/create/rename/delete/frontmatter); optimistic concurrency via sha256 content version + per-path lock + `409 {currentContent}` (reject-stale-and-reload; no silent overwrite); transport-transparent version tracking (app unchanged); double-submit CSRF (`tolaria_csrf` cookie + `X-CSRF-Token` header) on `/api/cmd/*`; vault mounted read-write; and that git commit/push/pull + remote conflict resolution remain Phase 5 (files are written but not committed).

- [ ] **Step 3: ARCHITECTURE update**

Add a "Write path & concurrency (Phase 4)" subsection: write command set, the version-token/409 protocol, per-path locking, CSRF, and the read-write vault mount. Concise.

- [ ] **Step 4: Gates**

Run:
```bash
cargo clippy --manifest-path /Users/daniel/Projects/tolaria/src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path /Users/daniel/Projects/tolaria/src-tauri/Cargo.toml -p tolaria-server
pnpm vitest run src/web/transport.test.ts
```
Expected: PASS. (`cargo llvm-cov`, CodeScene, Codacy via pre-push sidecar; Docker build on the server.)

- [ ] **Step 5: commit**

```bash
git add docker-compose.yml docs/
git commit -m "feat(docker,docs): vault read-write; ADR 0149 + write-path docs"
```

---

## Self-Review

**Spec coverage (§7, §13 Phase 4):**
- Write path over core (save/create/rename/delete/frontmatter) → Tasks 2, 3.
- Optimistic concurrency: version token + compare-and-set under per-path lock + `409` with current content → Tasks 1, 2.
- Reject-stale-and-reload (chosen) → Task 2 (server 409) + Task 5 (client reload).
- Per-path locks → Task 1, applied Tasks 2–3.
- CSRF (chosen: double-submit; carried prerequisite) → Task 4 + Task 5.
- useradd git-email prerequisite → Task 6.
- Vault writable for persistence → Task 7.
- Git commit/push explicitly deferred to Phase 5 → Global Constraints + ADR.
- Vault containment preserved on all writes → Task 2 (`contained_note_path_for_write`) + Task 3.

**Placeholder scan:** Task 3 intentionally directs the implementer to discover exact rename/frontmatter arg shapes (Step 1) then follow the two shown arm patterns — the method and the two concrete examples are given, not vague. No TBDs elsewhere.

**Type consistency:** `AppState::new(vault_root, users, sessions, cookie_secure, locks)` (Task 2) — every call site updated (build_router + test helpers). `dispatch_write(state, command, args)` async (Task 2), extended in Task 3, called from `command_route` (Task 2). `content_version` (Task 1) used in Tasks 2/5-parity. `contained_note_path` made `pub(crate)` (Task 2) reused in Task 3; `contained_note_path_for_write` defined in Task 2 used in Tasks 2/3. `csrf::{CSRF_COOKIE, CSRF_HEADER, verify, csrf_cookie, generate_token}` (Task 4) used in rpc + auth_routes. Client/server sha256 agree (Global Constraints).

**Security:** all writes stay behind `require_auth` + vault containment + CSRF; concurrency prevents lost updates; no git means no push credential exposure this phase. Deferred to Phase 5: git commit authorship (per-user), push/pull, remote conflict UI.
