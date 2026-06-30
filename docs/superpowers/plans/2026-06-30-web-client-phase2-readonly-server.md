# Web Client — Phase 2: Read-Only Axum Server + Docker Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up a real `tolaria-server` (Axum) binary that serves the existing React frontend and answers the read-only vault command set over HTTP using `tolaria-core`, packaged as a runnable multi-stage Docker image — so the app can be browsed from a browser on a server instead of the 8 GB dev machine.

**Architecture:** A new `tolaria-server` workspace crate exposes one generic RPC endpoint, `POST /api/cmd/:command`, that dispatches a whitelisted read-only command set (`list_vault`, `list_vault_folders`, `get_note_content`, `get_all_content`, `reload_vault_entry`, `search_vault`) to `tolaria-core`, returns a structured "unsupported on web" error for everything else, and serves the built SPA for all other routes. The browser runs the **same React app**, built for a `web` target whose Vite config aliases `@tauri-apps/api/*` to thin HTTP/no-op shims so `invoke()` reaches `/api/cmd/*`. A multi-stage Dockerfile builds the frontend (Node 22) and the server (Rust), then runs the server over a mounted vault volume.

**Tech Stack:** Rust (edition 2021), Axum 0.7, Tokio, tower-http (static/fs + trace), serde/serde_json, `tolaria-core`; Vite 7 / React 19 (web build target); Docker multi-stage (node:22-bookworm-slim + rust:1-bookworm → debian:bookworm-slim).

**Spec:** `docs/superpowers/specs/2026-06-29-web-client-design.md` (§4.2, §5.1, §5.3, §6, §9, §11, §13 Phase 2)

## Global Constraints

- Rust edition `2021`, `rust-version = "1.77.2"` — `tolaria-server` MUST match the workspace.
- `tolaria-server` depends ONLY on `tolaria-core` for vault behavior — it MUST NOT reimplement vault/git/search/frontmatter logic.
- **Phase 2 is read-only and unauthenticated.** No write/rename/delete/git commands are wired. The image MUST be documented as "deploy behind a trusted network / authenticating reverse proxy." Auth is Phase 3.
- The web build is a SEPARATE Vite target; the Tauri/desktop build MUST remain unchanged.
- Coverage gate: `cargo llvm-cov --manifest-path src-tauri/Cargo.toml --no-clean --fail-under-lines 85` (workspace-wide) and frontend ≥70% must still pass.
- CodeScene: every touched/new scorable file leaves ≥ its starting score; new files reach `10.0` (or zero findings). Codacy: no new Critical/High.
- **⛔ NEVER `--no-verify`, NEVER lower `.codescene-thresholds`, NEVER add `#[allow(...)]` / `as any` / lint-disables.**
- Localization: any NEW user-facing copy goes through `src/lib/locales/en.json` + `pnpm l10n:translate`. (Phase 2 adds no new in-app copy beyond an optional connection-error string — localize it if added.)
- Commit after every task. `feat:` for new server/transport code, `test:`, `docs:`, `chore:` for Docker/config.

## Background facts (verified against the code)

- Frontend Tauri imports to shim: `@tauri-apps/api/core` (66 files, `invoke`), `@tauri-apps/api/event` (3, `listen`), `@tauri-apps/api/window` (5), `@tauri-apps/api/webview` (1).
- `serve-demo.mjs` already proves the React app renders in a browser using only the read subset + a permissive fallback for unsupported commands. Phase 2 replicates that contract with the real Rust core.
- Command arg shapes the frontend sends (Tauri converts JS camelCase → Rust): `list_vault {path}`, `list_vault_folders {path}`, `get_note_content {path, vaultPath?}`, `get_all_content {path}`, `reload_vault_entry {path}`, `search_vault {vaultPath, query, limit?, excludeFrontmatter?}`.
- `tolaria-core` read API: `vault::scan_vault_cached(&Path) -> Result<Vec<VaultEntry>, String>`, `vault::get_note_content(&Path) -> Result<String, String>`, `search::search_vault_with_options(SearchOptions) -> Result<SearchResponse, String>`, `vault::parse_md_file`, and the visibility filters in `vault::{filter_gitignored_entries, ...}`. The desktop `list_vault` returns *visible* entries (scan + gitignore/visibility filter); the server replicates that.

## File structure (target)

```
src-tauri/
  Cargo.toml                              # workspace: add crates/tolaria-server member
  crates/
    tolaria-server/
      Cargo.toml                          # new binary crate
      src/
        main.rs                           # bootstrap: config -> router -> serve
        config.rs                         # ServerConfig from env
        rpc.rs                            # POST /api/cmd/:command dispatch + error type
        handlers.rs                       # read-only command handlers -> tolaria-core
        static_files.rs                   # serve dist/ with SPA fallback
src/
  web/
    transport.ts                          # invoke() over HTTP -> /api/cmd/:command
    event.ts                              # listen()/emit() no-op stubs for web
    window.ts                             # window/webview API no-op stubs for web
vite.config.web.ts                        # web build target (aliases @tauri-apps/api/*)
Dockerfile                                # multi-stage frontend+server build
.dockerignore
docker-compose.yml                        # vault volume + port mapping example
```

---

### Task 1: Add `tolaria-server` crate to the workspace

**Files:**
- Modify: `src-tauri/Cargo.toml` (workspace members)
- Create: `src-tauri/crates/tolaria-server/Cargo.toml`
- Create: `src-tauri/crates/tolaria-server/src/main.rs`

**Interfaces:**
- Produces: a buildable `tolaria-server` binary (prints a startup line, exits). No HTTP yet.

- [ ] **Step 1: Add the member to the workspace**

In `src-tauri/Cargo.toml`, change the `members` line to include the server:

```toml
[workspace]
members = [".", "crates/tolaria-core", "crates/tolaria-server"]
resolver = "2"
```

- [ ] **Step 2: Create the crate manifest**

Create `src-tauri/crates/tolaria-server/Cargo.toml`:

```toml
[package]
name = "tolaria-server"
version = "0.1.0"
edition = "2021"
rust-version = "1.77.2"
license = "AGPL-3.0-or-later"

[[bin]]
name = "tolaria-server"
path = "src/main.rs"

[dependencies]
tolaria-core = { path = "../tolaria-core" }
axum = "0.7"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net", "signal"] }
tower-http = { version = "0.6", features = ["fs", "trace"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

[dev-dependencies]
tempfile = "3"
tower = { version = "0.5", features = ["util"] }
http-body-util = "0.1"
```

- [ ] **Step 3: Minimal main**

Create `src-tauri/crates/tolaria-server/src/main.rs`:

```rust
//! Tolaria web server: serves the SPA and a read-only vault RPC over tolaria-core.

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "tolaria_server=info,tower_http=info".into()),
        )
        .init();
    tracing::info!("tolaria-server starting (bootstrap stub)");
}
```

- [ ] **Step 4: Build**

Run: `cargo build --manifest-path /Users/daniel/Projects/tolaria/src-tauri/Cargo.toml -p tolaria-server`
Expected: PASS — `tolaria-server` compiles.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/crates/tolaria-server
git commit -m "feat(server): scaffold tolaria-server workspace crate"
```

---

### Task 2: `ServerConfig` from environment

**Files:**
- Create: `src-tauri/crates/tolaria-server/src/config.rs`
- Modify: `src-tauri/crates/tolaria-server/src/main.rs` (declare `mod config;`)

**Interfaces:**
- Produces: `config::ServerConfig { vault_path: PathBuf, static_dir: PathBuf, listen_addr: SocketAddr }` and `ServerConfig::from_env() -> Result<ServerConfig, String>`.

- [ ] **Step 1: Write the failing test**

Create `src-tauri/crates/tolaria-server/src/config.rs`:

```rust
use std::net::SocketAddr;
use std::path::PathBuf;

/// Runtime configuration, sourced from environment variables.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub vault_path: PathBuf,
    pub static_dir: PathBuf,
    pub listen_addr: SocketAddr,
}

impl ServerConfig {
    /// Build config from a key lookup function (env in production, a map in tests).
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let vault_path = get("TOLARIA_VAULT_PATH")
            .ok_or_else(|| "TOLARIA_VAULT_PATH is required".to_string())?
            .into();
        let static_dir = get("TOLARIA_STATIC_DIR").unwrap_or_else(|| "/app/dist".into()).into();
        let host = get("TOLARIA_HOST").unwrap_or_else(|| "0.0.0.0".into());
        let port = get("TOLARIA_PORT").unwrap_or_else(|| "8787".into());
        let listen_addr = format!("{host}:{port}")
            .parse()
            .map_err(|e| format!("invalid listen address {host}:{port}: {e}"))?;
        Ok(Self { vault_path, static_dir, listen_addr })
    }

    pub fn from_env() -> Result<Self, String> {
        Self::from_lookup(|k| std::env::var(k).ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn lookup(map: &HashMap<&str, &str>) -> impl Fn(&str) -> Option<String> + '_ {
        move |k| map.get(k).map(|v| v.to_string())
    }

    #[test]
    fn requires_vault_path() {
        let map = HashMap::new();
        assert!(ServerConfig::from_lookup(lookup(&map)).is_err());
    }

    #[test]
    fn defaults_host_port_static_dir() {
        let map = HashMap::from([("TOLARIA_VAULT_PATH", "/vault")]);
        let cfg = ServerConfig::from_lookup(lookup(&map)).unwrap();
        assert_eq!(cfg.vault_path, PathBuf::from("/vault"));
        assert_eq!(cfg.static_dir, PathBuf::from("/app/dist"));
        assert_eq!(cfg.listen_addr.to_string(), "0.0.0.0:8787");
    }

    #[test]
    fn rejects_bad_address() {
        let map = HashMap::from([("TOLARIA_VAULT_PATH", "/vault"), ("TOLARIA_PORT", "notaport")]);
        assert!(ServerConfig::from_lookup(lookup(&map)).is_err());
    }
}
```

In `main.rs`, add `mod config;` below the doc comment.

- [ ] **Step 2: Run tests to verify they pass (logic written with test)**

Run: `cargo test --manifest-path /Users/daniel/Projects/tolaria/src-tauri/Cargo.toml -p tolaria-server config`
Expected: PASS — 3 config tests pass.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/crates/tolaria-server/src
git commit -m "feat(server): ServerConfig from environment with defaults"
```

---

### Task 3: RPC error type + dispatch skeleton

**Files:**
- Create: `src-tauri/crates/tolaria-server/src/rpc.rs`
- Modify: `src-tauri/crates/tolaria-server/src/main.rs` (`mod rpc;`)

**Interfaces:**
- Produces:
  - `rpc::RpcError` implementing `axum::response::IntoResponse` (serializes `{ "error": String }` with an HTTP status).
  - `rpc::unsupported(command: &str) -> RpcError` → 501 with message `"command '<command>' is not supported on web"`.
  - `rpc::ok_json<T: Serialize>(value: T) -> axum::response::Response`.

- [ ] **Step 1: Write the failing test**

Create `src-tauri/crates/tolaria-server/src/rpc.rs`:

```rust
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::json;

/// An RPC failure rendered as `{ "error": "..." }` with a status code.
#[derive(Debug)]
pub struct RpcError {
    pub status: StatusCode,
    pub message: String,
}

impl RpcError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self { status: StatusCode::BAD_REQUEST, message: message.into() }
    }
    pub fn internal(message: impl Into<String>) -> Self {
        Self { status: StatusCode::INTERNAL_SERVER_ERROR, message: message.into() }
    }
}

/// Marker for a command the read-only web server does not implement.
pub fn unsupported(command: &str) -> RpcError {
    RpcError {
        status: StatusCode::NOT_IMPLEMENTED,
        message: format!("command '{command}' is not supported on web"),
    }
}

impl IntoResponse for RpcError {
    fn into_response(self) -> Response {
        (self.status, axum::Json(json!({ "error": self.message }))).into_response()
    }
}

/// Serialize a value as a 200 JSON response.
pub fn ok_json<T: Serialize>(value: T) -> Response {
    axum::Json(value).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_is_501_with_message() {
        let err = unsupported("save_note_content");
        assert_eq!(err.status, StatusCode::NOT_IMPLEMENTED);
        assert!(err.message.contains("save_note_content"));
        assert!(err.message.contains("not supported on web"));
    }
}
```

In `main.rs`, add `mod rpc;`.

- [ ] **Step 2: Run test**

Run: `cargo test --manifest-path /Users/daniel/Projects/tolaria/src-tauri/Cargo.toml -p tolaria-server rpc`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/crates/tolaria-server/src/rpc.rs src-tauri/crates/tolaria-server/src/main.rs
git commit -m "feat(server): RPC error type and unsupported-command marker"
```

---

### Task 4: Read-only command handlers over `tolaria-core`

**Files:**
- Create: `src-tauri/crates/tolaria-server/src/handlers.rs`
- Modify: `src-tauri/crates/tolaria-server/src/main.rs` (`mod handlers;`)

**Interfaces:**
- Consumes: `tolaria_core::vault::{scan_vault_cached, get_note_content, VaultEntry}`, `tolaria_core::search::{search_vault_with_options, SearchOptions, SearchResponse}`.
- Produces: `handlers::dispatch(command: &str, args: serde_json::Value) -> Result<serde_json::Value, rpc::RpcError>` — the pure dispatch function (no HTTP), returning JSON or an `RpcError`. Read commands handled: `list_vault`, `get_note_content`, `get_all_content`, `reload_vault_entry`, `search_vault`. Everything else → `rpc::unsupported`.

> Note: the frontend sends camelCase keys. Use `#[serde(rename_all = "camelCase")]` on arg structs so `vaultPath`/`excludeFrontmatter` deserialize.

- [ ] **Step 1: Write the failing test (against a temp vault)**

Create `src-tauri/crates/tolaria-server/src/handlers.rs`:

```rust
use crate::rpc::{self, RpcError};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use tolaria_core::search::{search_vault_with_options, SearchOptions};
use tolaria_core::vault::{get_note_content, scan_vault_cached};

#[derive(Deserialize)]
struct PathArgs {
    path: PathBuf,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchArgs {
    vault_path: String,
    query: String,
    limit: Option<usize>,
    exclude_frontmatter: Option<bool>,
}

fn parse_args<T: serde::de::DeserializeOwned>(args: Value) -> Result<T, RpcError> {
    serde_json::from_value(args).map_err(|e| RpcError::bad_request(format!("bad arguments: {e}")))
}

/// Dispatch a command name + JSON args to a read-only handler.
pub fn dispatch(command: &str, args: Value) -> Result<Value, RpcError> {
    match command {
        "list_vault" | "reload_vault" => {
            let a: PathArgs = parse_args(args)?;
            let entries = scan_vault_cached(&a.path).map_err(RpcError::internal)?;
            Ok(serde_json::to_value(entries).map_err(|e| RpcError::internal(e.to_string()))?)
        }
        "get_note_content" => {
            let a: PathArgs = parse_args(args)?;
            let content = get_note_content(&a.path).map_err(RpcError::internal)?;
            Ok(json!(content))
        }
        "search_vault" => {
            let a: SearchArgs = parse_args(args)?;
            let resp = search_vault_with_options(SearchOptions {
                vault_path: &a.vault_path,
                query: &a.query,
                mode: "keyword",
                limit: a.limit.unwrap_or(20),
                hide_gitignored_files: true,
                exclude_frontmatter: a.exclude_frontmatter.unwrap_or(false),
            })
            .map_err(RpcError::internal)?;
            Ok(serde_json::to_value(resp).map_err(|e| RpcError::internal(e.to_string()))?)
        }
        other => Err(rpc::unsupported(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn list_vault_returns_entries() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("note.md"), "# Hello\n\nbody\n").unwrap();
        let out = dispatch("list_vault", json!({ "path": dir.path() })).unwrap();
        assert!(out.is_array(), "list_vault returns a JSON array");
        assert!(!out.as_array().unwrap().is_empty(), "the note is listed");
    }

    #[test]
    fn get_note_content_returns_text() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("n.md");
        fs::write(&p, "# Title\n\nhello\n").unwrap();
        let out = dispatch("get_note_content", json!({ "path": p })).unwrap();
        assert_eq!(out.as_str().unwrap(), "# Title\n\nhello\n");
    }

    #[test]
    fn search_vault_finds_keyword() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.md"), "# A\n\nfindme please\n").unwrap();
        let out = dispatch(
            "search_vault",
            json!({ "vaultPath": dir.path(), "query": "findme" }),
        )
        .unwrap();
        assert!(out.get("results").is_some(), "search returns a results field");
    }

    #[test]
    fn unsupported_command_errors_501() {
        let err = dispatch("save_note_content", json!({})).unwrap_err();
        assert_eq!(err.status, axum::http::StatusCode::NOT_IMPLEMENTED);
    }
}
```

In `main.rs`, add `mod handlers;`.

> If `scan_vault_cached` returns hidden/gitignored entries that the desktop filters out, wrap the result with `tolaria_core::vault::filter_gitignored_entries` to match desktop `list_vault` visibility. Verify by reading `src-tauri/src/commands/vault/file_cmds.rs` `scan_visible_vault_entries` and mirroring its filter calls here.

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test --manifest-path /Users/daniel/Projects/tolaria/src-tauri/Cargo.toml -p tolaria-server handlers`
Expected: PASS — 4 handler tests pass. If `get_all_content`/`reload_vault_entry` are needed by the app boot (see Task 7), add them here mirroring the existing desktop command bodies and add matching tests.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/crates/tolaria-server/src
git commit -m "feat(server): read-only command dispatch over tolaria-core"
```

---

### Task 5: HTTP RPC route + static SPA serving + router

**Files:**
- Create: `src-tauri/crates/tolaria-server/src/static_files.rs`
- Modify: `src-tauri/crates/tolaria-server/src/rpc.rs` (add the axum handler)
- Modify: `src-tauri/crates/tolaria-server/src/main.rs` (assemble + serve the router)

**Interfaces:**
- Produces: `rpc::router(config) -> axum::Router` exposing `POST /api/cmd/:command` (body = JSON args) and a static fallback serving `config.static_dir` with `index.html` SPA fallback.

- [ ] **Step 1: Add the axum command handler to `rpc.rs`**

Append to `src-tauri/crates/tolaria-server/src/rpc.rs`:

```rust
use axum::extract::Path as AxumPath;
use axum::Json;
use serde_json::Value;

/// `POST /api/cmd/:command` — body is the JSON args object.
pub async fn command_route(
    AxumPath(command): AxumPath<String>,
    body: Option<Json<Value>>,
) -> Response {
    let args = body.map(|Json(v)| v).unwrap_or(Value::Null);
    match crate::handlers::dispatch(&command, args) {
        Ok(value) => ok_json(value),
        Err(err) => err.into_response(),
    }
}
```

- [ ] **Step 2: Static serving with SPA fallback**

Create `src-tauri/crates/tolaria-server/src/static_files.rs`:

```rust
use axum::http::StatusCode;
use std::path::PathBuf;
use tower_http::services::{ServeDir, ServeFile};

/// Serve `static_dir` with `index.html` as the SPA fallback for unknown paths.
pub fn service(static_dir: PathBuf) -> ServeDir<ServeFile> {
    let index = static_dir.join("index.html");
    ServeDir::new(static_dir).not_found_service(ServeFile::new(index))
}

/// 404 JSON for unmatched API routes (so the SPA fallback never swallows them).
pub async fn api_not_found() -> (StatusCode, &'static str) {
    (StatusCode::NOT_FOUND, "not found")
}
```

- [ ] **Step 3: Assemble the router in `main.rs`**

Replace `main.rs` body so it builds the router and serves:

```rust
//! Tolaria web server: serves the SPA and a read-only vault RPC over tolaria-core.

mod config;
mod handlers;
mod rpc;
mod static_files;

use axum::routing::post;
use axum::Router;

fn build_router(cfg: &config::ServerConfig) -> Router {
    Router::new()
        .route("/api/cmd/:command", post(rpc::command_route))
        .fallback_service(static_files::service(cfg.static_dir.clone()))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "tolaria_server=info,tower_http=info".into()),
        )
        .init();

    let cfg = match config::ServerConfig::from_env() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("configuration error: {e}");
            std::process::exit(1);
        }
    };
    tracing::info!("serving vault {:?} on {}", cfg.vault_path, cfg.listen_addr);

    let app = build_router(&cfg);
    let listener = tokio::net::TcpListener::bind(cfg.listen_addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

- [ ] **Step 4: Write an HTTP integration test for the RPC route**

Create `src-tauri/crates/tolaria-server/tests/rpc_http.rs`:

```rust
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::json;
use std::fs;
use tower::ServiceExt;

// Re-declare a tiny router using the crate's public router builder is not exposed;
// instead test the command route through a minimal Router mirroring main.rs.
#[tokio::test]
async fn list_vault_over_http_returns_200_array() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("n.md"), "# A\n\nbody\n").unwrap();

    let app = axum::Router::new().route(
        "/api/cmd/:command",
        axum::routing::post(tolaria_server::rpc::command_route),
    );

    let req = Request::builder()
        .method("POST")
        .uri("/api/cmd/list_vault")
        .header("content-type", "application/json")
        .body(Body::from(json!({ "path": dir.path() }).to_string()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(v.is_array());
}
```

> To make `rpc::command_route` and `handlers::dispatch` reachable from the integration test, add a `src-tauri/crates/tolaria-server/src/lib.rs` that re-exports the modules (`pub mod config; pub mod handlers; pub mod rpc; pub mod static_files;`) and have `main.rs` `use tolaria_server::{...}`. Update `Cargo.toml` with a `[lib] name = "tolaria_server"` section. Adjust `main.rs` `mod` declarations to `use tolaria_server::...` accordingly.

- [ ] **Step 5: Build + test**

Run: `cargo test --manifest-path /Users/daniel/Projects/tolaria/src-tauri/Cargo.toml -p tolaria-server`
Expected: PASS — unit + HTTP integration tests pass.

- [ ] **Step 6: Manual smoke (optional, needs a built dist/)**

```bash
TOLARIA_VAULT_PATH="$PWD/demo-vault-v2" TOLARIA_STATIC_DIR="$PWD/dist" TOLARIA_PORT=8787 \
  cargo run --manifest-path src-tauri/Cargo.toml -p tolaria-server &
sleep 2
curl -s -XPOST localhost:8787/api/cmd/list_vault -H 'content-type: application/json' \
  -d "{\"path\":\"$PWD/demo-vault-v2\"}" | head -c 200
```
Expected: a JSON array of entries.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/crates/tolaria-server
git commit -m "feat(server): HTTP RPC route, SPA static serving, router assembly"
```

---

### Task 6: Web transport shim + `web` Vite build target

**Files:**
- Create: `src/web/transport.ts`
- Create: `src/web/event.ts`
- Create: `src/web/window.ts`
- Create: `vite.config.web.ts`
- Test: `src/web/transport.test.ts`

**Interfaces:**
- Consumes: nothing from the server crate (runtime HTTP contract only).
- Produces: a `web` Vite build that aliases `@tauri-apps/api/core` → `src/web/transport.ts` (exports `invoke`), `@tauri-apps/api/event` → `src/web/event.ts` (exports `listen`, `emit`, `once`), `@tauri-apps/api/window` + `@tauri-apps/api/webview` → `src/web/window.ts` (no-op stubs). Output goes to `dist/`.

> The shim mirrors `src/mock-tauri`'s contract: forward every `invoke` to `/api/cmd/:command`; on a `501` (unsupported) return a benign default so the app still renders read-only, matching how `serve-demo.mjs` + the mock fallback already let the app boot.

- [ ] **Step 1: Write the failing test for the transport**

Create `src/web/transport.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { invoke } from './transport'

describe('web invoke transport', () => {
  beforeEach(() => { vi.restoreAllMocks() })

  it('POSTs to /api/cmd/:command and returns parsed JSON', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify(['a', 'b']), { status: 200, headers: { 'content-type': 'application/json' } }),
    )
    vi.stubGlobal('fetch', fetchMock)
    const result = await invoke<string[]>('list_vault', { path: '/v' })
    expect(fetchMock).toHaveBeenCalledWith('/api/cmd/list_vault', expect.objectContaining({ method: 'POST' }))
    expect(result).toEqual(['a', 'b'])
  })

  it('returns undefined for a 501 unsupported command instead of throwing', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ error: 'unsupported' }), { status: 501 }),
    ))
    const result = await invoke('save_note_content', { path: '/v/n.md', content: 'x' })
    expect(result).toBeUndefined()
  })
})
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm vitest run src/web/transport.test.ts`
Expected: FAIL — `./transport` does not exist.

- [ ] **Step 3: Implement the transport**

Create `src/web/transport.ts`:

```ts
/**
 * Web transport: replaces Tauri's `invoke` for the browser build.
 * Forwards each command to the server's generic RPC endpoint. Unsupported
 * (501) commands resolve to `undefined` so the read-only app still renders.
 */
export async function invoke<T = unknown>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  const res = await fetch(`/api/cmd/${command}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(args ?? {}),
  })
  if (res.status === 501) {
    // Command not implemented on web (read-only phase) — degrade gracefully.
    return undefined as T
  }
  if (!res.ok) {
    const detail = await res.json().catch(() => ({ error: res.statusText }))
    throw new Error(typeof detail?.error === 'string' ? detail.error : `invoke ${command} failed`)
  }
  return (await res.json()) as T
}
```

- [ ] **Step 4: Implement event + window stubs**

Create `src/web/event.ts`:

```ts
/** Web stubs for @tauri-apps/api/event. Phase 2 has no live events; listen is a no-op. */
export type UnlistenFn = () => void
export async function listen(): Promise<UnlistenFn> {
  return () => {}
}
export async function once(): Promise<UnlistenFn> {
  return () => {}
}
export async function emit(): Promise<void> {}
```

Create `src/web/window.ts`:

```ts
/** Web stubs for @tauri-apps/api/window and /webview. No native window on web. */
export function getCurrentWindow() {
  return {
    listen: async () => () => {},
    onCloseRequested: async () => () => {},
    setTitle: async () => {},
    show: async () => {},
    hide: async () => {},
  }
}
export function getCurrentWebview() {
  return { listen: async () => () => {} }
}
```

- [ ] **Step 5: Create the web Vite config with aliases**

Create `vite.config.web.ts`:

```ts
import { defineConfig, mergeConfig } from 'vite'
import base from './vite.config'
import { resolve } from 'node:path'

// Web build: same app, Tauri APIs aliased to HTTP/no-op shims, output to dist/.
export default mergeConfig(base, defineConfig({
  resolve: {
    alias: {
      '@tauri-apps/api/core': resolve(__dirname, 'src/web/transport.ts'),
      '@tauri-apps/api/event': resolve(__dirname, 'src/web/event.ts'),
      '@tauri-apps/api/window': resolve(__dirname, 'src/web/window.ts'),
      '@tauri-apps/api/webview': resolve(__dirname, 'src/web/window.ts'),
    },
  },
  build: { outDir: 'dist' },
}))
```

> Verify `vite.config.ts` exports a usable default config object for `mergeConfig`. If it exports a function (`defineConfig(({ command }) => ...)`, common with Tauri), adapt by calling it or by duplicating only the plugins needed for a web build. Read `vite.config.ts` top-level export before writing this file and adjust accordingly.

- [ ] **Step 6: Run the transport test**

Run: `pnpm vitest run src/web/transport.test.ts`
Expected: PASS.

- [ ] **Step 7: Build the web bundle**

Run: `pnpm exec vite build --config vite.config.web.ts`
Expected: PASS — `dist/` produced. (On the 8 GB dev machine this may need `NODE_OPTIONS=--max-old-space-size=4096`; in Docker it runs with full RAM.)

- [ ] **Step 8: Commit**

```bash
git add src/web vite.config.web.ts
git commit -m "feat(web): HTTP invoke transport, Tauri API stubs, web Vite target"
```

---

### Task 7: Make the app boot read-only (answer/whitelist boot commands)

The app issues many `invoke`s on startup (settings, vault list, git status, AI status, etc.). With the 501→`undefined` degrade (Task 6) plus the real read handlers, the app should render as `serve-demo.mjs` already demonstrates. This task verifies that and fills any boot-blocking gaps.

**Files:**
- Modify: `src-tauri/crates/tolaria-server/src/handlers.rs` (add any boot-critical read commands discovered)
- Test: `src-tauri/crates/tolaria-server/src/handlers.rs` (tests for additions)

**Interfaces:**
- Produces: handler coverage for whatever boot commands return data the UI cannot render without (candidates: `get_all_content`, `reload_vault_entry`, `scan_views`/`list_vault_folders`). Each added command mirrors the desktop command body using `tolaria-core`.

- [ ] **Step 1: Enumerate the boot invoke calls**

Run:
```bash
grep -rhoE "invoke<?[^(]*\(['\"][a-z_]+['\"]" /Users/daniel/Projects/tolaria/src/App.tsx /Users/daniel/Projects/tolaria/src/hooks/useVaultLoader*.ts | grep -oE "['\"][a-z_]+['\"]" | tr -d "\"'" | sort -u
```
Record the command names the app calls during initial vault load.

- [ ] **Step 2: Serve the demo build and load it in a browser**

```bash
pnpm exec vite build --config vite.config.web.ts
TOLARIA_VAULT_PATH="$PWD/demo-vault-v2" TOLARIA_STATIC_DIR="$PWD/dist" TOLARIA_PORT=8787 \
  cargo run --manifest-path src-tauri/Cargo.toml -p tolaria-server &
sleep 2
```
Open `http://localhost:8787`. Expected: the vault list renders and notes open read-only.

- [ ] **Step 3: For each boot command that returns a render-blocking 501, add a read handler**

For example, if `get_all_content` is required, add to `dispatch` in `handlers.rs`:

```rust
        "get_all_content" => {
            let a: PathArgs = parse_args(args)?;
            let entries = scan_vault_cached(&a.path).map_err(RpcError::internal)?;
            let mut out = serde_json::Map::new();
            for entry in entries {
                if let Ok(content) = get_note_content(std::path::Path::new(&entry.path)) {
                    out.insert(entry.path.clone(), json!(content));
                }
            }
            Ok(Value::Object(out))
        }
```

Add a matching test mirroring the `list_vault_returns_entries` style. Only add commands that actually block rendering — leave the rest as 501→degrade (YAGNI).

- [ ] **Step 4: Re-verify the browser render + run tests**

Run: `cargo test --manifest-path /Users/daniel/Projects/tolaria/src-tauri/Cargo.toml -p tolaria-server`
Expected: PASS. Browser shows the read-only vault.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/crates/tolaria-server/src/handlers.rs
git commit -m "feat(server): handle boot-critical read commands for browser render"
```

---

### Task 8: Multi-stage Dockerfile + compose

**Files:**
- Create: `Dockerfile`
- Create: `.dockerignore`
- Create: `docker-compose.yml`

**Interfaces:**
- Produces: a `tolaria-server` image that builds the web bundle and the server, and runs the server serving a mounted vault.

- [ ] **Step 1: `.dockerignore`**

Create `.dockerignore`:

```
node_modules
dist
src-tauri/target
target
.git
demo-vault
demo-vault-v2
e2e
tests
*.log
```

- [ ] **Step 2: Multi-stage Dockerfile**

Create `Dockerfile`:

```dockerfile
# --- Stage 1: build the web bundle (Node 22, matches CI) ---
FROM node:22-bookworm-slim AS web
WORKDIR /app
RUN corepack enable && corepack prepare pnpm@10.18.0 --activate
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY patches ./patches
RUN pnpm install --frozen-lockfile
COPY . .
RUN NODE_OPTIONS=--max-old-space-size=4096 pnpm exec vite build --config vite.config.web.ts

# --- Stage 2: build the server binary ---
FROM rust:1-bookworm AS server
WORKDIR /build
COPY src-tauri ./src-tauri
# Build only the server crate (skips the Tauri app's system deps).
RUN cargo build --release --manifest-path src-tauri/Cargo.toml -p tolaria-server

# --- Stage 3: runtime ---
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends git ca-certificates \
  && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=server /build/src-tauri/target/release/tolaria-server /usr/local/bin/tolaria-server
COPY --from=web /app/dist /app/dist
ENV TOLARIA_STATIC_DIR=/app/dist \
    TOLARIA_VAULT_PATH=/vault \
    TOLARIA_HOST=0.0.0.0 \
    TOLARIA_PORT=8787
EXPOSE 8787
ENTRYPOINT ["tolaria-server"]
```

> `tolaria-server` depends only on `tolaria-core` (no Tauri/GTK/webkit system libs), so the Rust stage does NOT need the Tauri build dependencies. If `cargo build -p tolaria-server` tries to compile the `tolaria` app crate, add `--no-default-features` is not enough — instead confirm the workspace build only pulls `tolaria-server` + `tolaria-core` for that `-p` target (it does, since cargo builds only the named package and its path deps).

- [ ] **Step 3: docker-compose for local/server run**

Create `docker-compose.yml`:

```yaml
services:
  tolaria-web:
    build: .
    image: tolaria-server:latest
    ports:
      - "8787:8787"
    volumes:
      # Mount a vault clone read-only into the container.
      - ./demo-vault-v2:/vault:ro
    environment:
      TOLARIA_VAULT_PATH: /vault
    restart: unless-stopped
```

- [ ] **Step 4: Build the image**

Run: `docker build -t tolaria-server:latest .`
Expected: all three stages succeed; final image built.

- [ ] **Step 5: Run and verify**

```bash
docker run --rm -p 8787:8787 -v "$PWD/demo-vault-v2:/vault:ro" tolaria-server:latest &
sleep 3
curl -s -XPOST localhost:8787/api/cmd/list_vault -H 'content-type: application/json' -d '{"path":"/vault"}' | head -c 200
```
Expected: JSON array of entries. Open `http://localhost:8787` in a browser → read-only vault renders.

- [ ] **Step 6: Commit**

```bash
git add Dockerfile .dockerignore docker-compose.yml
git commit -m "feat(docker): multi-stage image building web bundle + tolaria-server"
```

---

### Task 9: ADR, docs, and gate verification

**Files:**
- Create: `docs/adr/0147-web-server-read-only-phase.md` (via `/create-adr`)
- Modify: `docs/ARCHITECTURE.md` (add the web-server component)

- [ ] **Step 1: ADR**

Use `/create-adr` to record: read-only `tolaria-server` (Axum) over `tolaria-core`, generic `/api/cmd/:command` dispatch with 501→degrade, web Vite target aliasing Tauri APIs, Docker deployment, unauthenticated-read-only-behind-proxy posture (auth is a later phase).

- [ ] **Step 2: Update ARCHITECTURE.md**

Add a "Web server (Phase 2)" subsection describing the crate, the RPC contract, and the Docker image. Keep it factual and short.

- [ ] **Step 3: Gate verification**

Run, and confirm each passes (or is offloaded to the Chunk sidecar at push time):
```bash
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
pnpm vitest run src/web/transport.test.ts
```
Expected: PASS. CodeScene/Codacy/coverage run via the pre-push gate.

- [ ] **Step 4: Commit**

```bash
git add docs/
git commit -m "docs: ADR 0147 + ARCHITECTURE for read-only web server"
```

---

## Self-Review

**Spec coverage (Phase 2 = design §4.2, §5.1, §5.3, §6, §9, §11, §13 step 2):**
- Axum server reusing core (§4.2) → Tasks 1, 4, 5.
- Routes incl. `POST /api/cmd/:command` + static SPA (§5.1) → Tasks 3, 5.
- RPC dispatch with whitelisted v1 set + "unsupported on web" (§5.3) → Tasks 3, 4.
- Web transport seam aliasing Tauri modules (§6) → Task 6.
- Read-only scope, rich features shimmed/hidden (§9) → 501→degrade (Task 6) + boot whitelist (Task 7).
- Deployment: single binary + static, env config, Docker (§11) → Tasks 2, 8.
- Phasing: auth/write/git explicitly deferred (§13 steps 3–5) → Global Constraints + ADR note.

**Deferred to later phases (correctly absent):** auth/sessions (Phase 3), write path + optimistic concurrency (Phase 4), git sync controls + WebSocket events (Phase 5). The `event.ts` stub and the read-only command set make this boundary explicit.

**Placeholder scan:** No "TBD"/"handle errors". Two research-dependent spots are explicit, not vague: Task 6 Step 5 (confirm `vite.config.ts` export shape before writing the merge) and Task 7 (enumerate real boot commands, add only render-blocking ones). Both name the exact file to read and the exact decision to make.

**Type consistency:** `handlers::dispatch(command, args) -> Result<Value, RpcError>` is defined in Task 4 and consumed by `rpc::command_route` in Task 5. `rpc::{RpcError, unsupported, ok_json}` defined in Task 3, used in Tasks 4–5. `ServerConfig` fields (`vault_path`, `static_dir`, `listen_addr`) defined in Task 2, used in Task 5. Env var names (`TOLARIA_VAULT_PATH`, `TOLARIA_STATIC_DIR`, `TOLARIA_HOST`, `TOLARIA_PORT`) are consistent across Tasks 2, 5, 8.

**Note on TDD:** server handlers and transport follow red→green (real new behavior). The Docker task is verification-by-run, not unit TDD (appropriate for packaging).
