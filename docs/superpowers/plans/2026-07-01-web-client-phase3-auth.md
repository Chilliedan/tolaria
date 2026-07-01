# Web Client — Phase 3: Built-in Auth + Sessions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Put the read-only `tolaria-server` behind built-in authentication: a SQLite-backed user store (argon2 hashes), in-memory sessions with an opaque-token cookie, a server-served login page, and middleware that gates every protected route so only logged-in users reach the vault.

**Architecture:** Extend `AppState` with a `UsersDb` (SQLite via bundled `rusqlite`) and an in-memory `SessionStore` (token → session, behind an `RwLock`). Public routes are `GET /login` (server-rendered HTML form) and `POST /api/auth/login`; everything else — `POST /api/cmd/:command`, `POST /api/auth/logout`, `GET /api/auth/me`, and the SPA — is wrapped in an auth middleware that returns `401` for `/api/*` and a `302 → /login` redirect for page routes. Accounts are provisioned out-of-band with a `tolaria-server useradd` CLI subcommand (no self-service signup). Each user row carries a git name/email for later write phases (unused in Phase 3).

**Tech Stack:** Rust/Axum 0.7 (existing `tolaria-server`), `rusqlite` (bundled SQLite), `argon2` (password hashing), `axum-extra` cookie jar, `rand` (session tokens). No new frontend framework — the login page is plain server-rendered HTML.

**Spec:** `docs/superpowers/specs/2026-06-29-web-client-design.md` (§5.1 routes, §5.2 authentication, §13 Phase 3)

## Global Constraints

- Rust edition `2021`, `rust-version = "1.77.2"` — match the workspace.
- `tolaria-server` still depends on `tolaria-core` only for vault behavior; auth code is server-local and must NOT leak into `tolaria-core`.
- Passwords are hashed with **argon2**; plaintext passwords are never stored or logged.
- Session cookie is **HttpOnly**, **SameSite=Lax**, **Secure** (configurable via `TOLARIA_COOKIE_SECURE`, default `true`), `Path=/`, name `tolaria_session`.
- **No self-service signup** — users are created only via the `useradd` CLI.
- Public (no-auth) routes are EXACTLY `GET /login` and `POST /api/auth/login`. Everything else requires a valid session: `/api/*` → `401 {"error":...}`; page routes → `302` to `/login`.
- SQLite via `rusqlite` with the **`bundled`** feature (no system libsqlite needed in the slim Docker image).
- Read-only scope is unchanged — Phase 3 adds auth, not writes. `handlers::dispatch` still only serves read commands.
- Coverage gate: `cargo llvm-cov … --fail-under-lines 85` (workspace) and frontend ≥70% (run via pre-push sidecar). CodeScene: touched/new files ≥ starting score, new files 10.0/zero findings. Codacy: no new Critical/High.
- **⛔ NEVER `--no-verify`, NEVER lower `.codescene-thresholds`, NEVER add `#[allow(...)]` / `as any` / lint-disables.**
- Localization: the login page is server-rendered outside the React i18n system; keep its copy minimal and in English (note this in the ADR). No `src/lib/locales` changes required.
- Commit after every task.

## Current seam (verified)

- `src-tauri/crates/tolaria-server/src/rpc.rs`: `AppState { vault_root: Arc<PathBuf> }`, `command_route(State<AppState>, Path<String>, Option<Json<Value>>)`.
- `src/lib.rs`: `pub fn build_router(vault_root: PathBuf, static_dir: PathBuf) -> Router` — creates `AppState::new(vault_root)`, routes `/api/cmd/:command`, `/api/*path` (JSON 404), SPA fallback.
- `src/config.rs`: `ServerConfig { vault_path, static_dir, listen_addr }` via `from_lookup`/`from_env`.
- `src/main.rs`: thin `#[tokio::main]` that reads config and serves `build_router(...)`.

## File structure (target)

```
src-tauri/crates/tolaria-server/
  Cargo.toml            # + rusqlite(bundled), argon2, axum-extra(cookie), rand
  src/
    config.rs           # + users_db_path, cookie_secure
    users.rs            # NEW: SQLite user store (schema, create_user, verify_credentials)
    session.rs          # NEW: in-memory SessionStore (create/get/remove, expiry)
    auth_routes.rs      # NEW: GET /login (HTML), POST /api/auth/login, /logout, GET /api/auth/me
    auth_middleware.rs  # NEW: require_auth middleware (401 for /api, 302 /login otherwise)
    rpc.rs              # AppState gains users + sessions
    lib.rs              # build_router: public vs protected split + middleware; new params
    main.rs             # subcommand dispatch: serve (default) | useradd
src/web/transport.ts    # on 401, redirect to /login (session-expiry UX)
Dockerfile              # + users DB dir; document useradd
docker-compose.yml      # + users DB volume + TOLARIA_COOKIE_SECURE
```

---

### Task 1: Dependencies + config fields

**Files:**
- Modify: `src-tauri/crates/tolaria-server/Cargo.toml`
- Modify: `src-tauri/crates/tolaria-server/src/config.rs`

**Interfaces:**
- Produces: `ServerConfig` gains `pub users_db_path: PathBuf` (env `TOLARIA_USERS_DB`, default `/app/data/users.db`) and `pub cookie_secure: bool` (env `TOLARIA_COOKIE_SECURE`, default `true`; `"false"`/`"0"` → false).

- [ ] **Step 1: Add dependencies**

In `src-tauri/crates/tolaria-server/Cargo.toml` under `[dependencies]`, add:

```toml
rusqlite = { version = "0.31", features = ["bundled"] }
argon2 = "0.5"
axum-extra = { version = "0.9", features = ["cookie"] }
rand = "0.8"
```

- [ ] **Step 2: Write the failing config tests**

In `src-tauri/crates/tolaria-server/src/config.rs`, extend the struct and `from_lookup`, and add tests. Replace the struct + `from_lookup` body to include the two new fields:

```rust
pub struct ServerConfig {
    pub vault_path: PathBuf,
    pub static_dir: PathBuf,
    pub listen_addr: SocketAddr,
    pub users_db_path: PathBuf,
    pub cookie_secure: bool,
}
```

Add inside `from_lookup`, before the final `Ok(Self { ... })`:

```rust
        let users_db_path = get("TOLARIA_USERS_DB")
            .unwrap_or_else(|| "/app/data/users.db".into())
            .into();
        let cookie_secure = !matches!(
            get("TOLARIA_COOKIE_SECURE").as_deref(),
            Some("false") | Some("0")
        );
```

and include `users_db_path, cookie_secure` in the returned struct. Add tests:

```rust
    #[test]
    fn defaults_users_db_and_cookie_secure() {
        let map = HashMap::from([("TOLARIA_VAULT_PATH", "/vault")]);
        let cfg = ServerConfig::from_lookup(lookup(&map)).unwrap();
        assert_eq!(cfg.users_db_path, PathBuf::from("/app/data/users.db"));
        assert!(cfg.cookie_secure);
    }

    #[test]
    fn cookie_secure_can_be_disabled() {
        let map = HashMap::from([("TOLARIA_VAULT_PATH", "/vault"), ("TOLARIA_COOKIE_SECURE", "false")]);
        let cfg = ServerConfig::from_lookup(lookup(&map)).unwrap();
        assert!(!cfg.cookie_secure);
    }
```

- [ ] **Step 3: Run tests**

Run: `cargo test --manifest-path /Users/daniel/Projects/tolaria/src-tauri/Cargo.toml -p tolaria-server config`
Expected: PASS (existing 3 + 2 new). Callers of `ServerConfig` (main.rs) will not compile yet only if they build the struct literally — they use `from_env`, so they still compile.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/crates/tolaria-server/Cargo.toml src-tauri/crates/tolaria-server/src/config.rs
git commit -m "feat(server): auth deps + users_db_path/cookie_secure config"
```

---

### Task 2: SQLite user store with argon2

**Files:**
- Create: `src-tauri/crates/tolaria-server/src/users.rs`
- Modify: `src-tauri/crates/tolaria-server/src/lib.rs` (`pub mod users;`)

**Interfaces:**
- Produces:
  - `users::UserRecord { pub id: i64, pub username: String, pub git_name: String, pub git_email: String }`
  - `users::UsersDb` with `UsersDb::open(path: &Path) -> Result<UsersDb, String>` (creates parent dir + schema if absent), `create_user(&self, username: &str, password: &str, git_name: &str, git_email: &str) -> Result<(), String>` (argon2-hash, insert; error on duplicate username), `verify_credentials(&self, username: &str, password: &str) -> Option<UserRecord>` (returns the user only when the password matches).
- Internals: `Arc<Mutex<rusqlite::Connection>>` inside `UsersDb` so it is `Clone + Send + Sync` for `AppState`.

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/crates/tolaria-server/src/users.rs`:

```rust
use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// A user account (password hash intentionally excluded).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserRecord {
    pub id: i64,
    pub username: String,
    pub git_name: String,
    pub git_email: String,
}

/// SQLite-backed user store. Cheap to clone (shares one connection).
#[derive(Clone)]
pub struct UsersDb {
    conn: Arc<Mutex<Connection>>,
}

impl UsersDb {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("users db dir: {e}"))?;
        }
        let conn = Connection::open(path).map_err(|e| format!("open users db: {e}"))?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS users (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                username TEXT NOT NULL UNIQUE,
                password_hash TEXT NOT NULL,
                git_name TEXT NOT NULL,
                git_email TEXT NOT NULL
            )",
            [],
        )
        .map_err(|e| format!("create users table: {e}"))?;
        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }

    /// Open an in-memory store for tests.
    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory().map_err(|e| e.to_string())?;
        conn.execute(
            "CREATE TABLE users (id INTEGER PRIMARY KEY AUTOINCREMENT, username TEXT NOT NULL UNIQUE, password_hash TEXT NOT NULL, git_name TEXT NOT NULL, git_email TEXT NOT NULL)",
            [],
        )
        .map_err(|e| e.to_string())?;
        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }

    pub fn create_user(&self, username: &str, password: &str, git_name: &str, git_email: &str) -> Result<(), String> {
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| format!("hash: {e}"))?
            .to_string();
        let conn = self.conn.lock().map_err(|_| "users db poisoned".to_string())?;
        conn.execute(
            "INSERT INTO users (username, password_hash, git_name, git_email) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![username, hash, git_name, git_email],
        )
        .map_err(|e| format!("insert user: {e}"))?;
        Ok(())
    }

    pub fn verify_credentials(&self, username: &str, password: &str) -> Option<UserRecord> {
        let conn = self.conn.lock().ok()?;
        let mut stmt = conn
            .prepare("SELECT id, username, password_hash, git_name, git_email FROM users WHERE username = ?1")
            .ok()?;
        let row = stmt
            .query_row(rusqlite::params![username], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })
            .ok()?;
        let (id, username, password_hash, git_name, git_email) = row;
        let parsed = PasswordHash::new(&password_hash).ok()?;
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .ok()?;
        Some(UserRecord { id, username, git_name, git_email })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_then_verify_correct_password() {
        let db = UsersDb::open_in_memory().unwrap();
        db.create_user("alice", "s3cret", "Alice", "alice@example.com").unwrap();
        let user = db.verify_credentials("alice", "s3cret").expect("correct password verifies");
        assert_eq!(user.username, "alice");
        assert_eq!(user.git_email, "alice@example.com");
    }

    #[test]
    fn wrong_password_rejected() {
        let db = UsersDb::open_in_memory().unwrap();
        db.create_user("bob", "right", "Bob", "bob@example.com").unwrap();
        assert!(db.verify_credentials("bob", "wrong").is_none());
    }

    #[test]
    fn unknown_user_rejected() {
        let db = UsersDb::open_in_memory().unwrap();
        assert!(db.verify_credentials("nobody", "x").is_none());
    }

    #[test]
    fn duplicate_username_errors() {
        let db = UsersDb::open_in_memory().unwrap();
        db.create_user("carol", "p", "Carol", "c@example.com").unwrap();
        assert!(db.create_user("carol", "p2", "Carol2", "c2@example.com").is_err());
    }

    #[test]
    fn password_is_hashed_not_plaintext() {
        let db = UsersDb::open_in_memory().unwrap();
        db.create_user("dave", "plaintextpw", "Dave", "d@example.com").unwrap();
        let conn = db.conn.lock().unwrap();
        let stored: String = conn
            .query_row("SELECT password_hash FROM users WHERE username='dave'", [], |r| r.get(0))
            .unwrap();
        assert!(stored.starts_with("$argon2"), "stored hash must be argon2 PHC string");
        assert!(!stored.contains("plaintextpw"));
    }
}
```

Add `pub mod users;` to `src/lib.rs`.

- [ ] **Step 2: Run tests**

Run: `cargo test --manifest-path /Users/daniel/Projects/tolaria/src-tauri/Cargo.toml -p tolaria-server users`
Expected: PASS — 5 tests (create/verify, wrong pw, unknown user, duplicate, hash-not-plaintext).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/crates/tolaria-server/src/users.rs src-tauri/crates/tolaria-server/src/lib.rs
git commit -m "feat(server): SQLite user store with argon2 password hashing"
```

---

### Task 3: In-memory session store

**Files:**
- Create: `src-tauri/crates/tolaria-server/src/session.rs`
- Modify: `src-tauri/crates/tolaria-server/src/lib.rs` (`pub mod session;`)

**Interfaces:**
- Produces:
  - `session::Session { pub user_id: i64, pub username: String }` (returned from lookups).
  - `session::SessionStore` (`Clone`, holds `Arc<RwLock<HashMap<String, StoredSession>>>`): `SessionStore::new(ttl: Duration) -> Self`, `create(&self, user_id: i64, username: &str) -> String` (returns a new opaque token), `get(&self, token: &str) -> Option<Session>` (None if missing or expired; lazily drops expired), `remove(&self, token: &str)`.
- Token: 32 random bytes hex-encoded (`rand::rngs::OsRng` + `fill_bytes`).

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/crates/tolaria-server/src/session.rs`:

```rust
use rand::rngs::OsRng;
use rand::RngCore;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// A resolved, still-valid session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub user_id: i64,
    pub username: String,
}

struct StoredSession {
    user_id: i64,
    username: String,
    expires_at: Instant,
}

/// In-memory session store keyed by opaque token. Cheap to clone.
#[derive(Clone)]
pub struct SessionStore {
    inner: Arc<RwLock<HashMap<String, StoredSession>>>,
    ttl: Duration,
}

impl SessionStore {
    pub fn new(ttl: Duration) -> Self {
        Self { inner: Arc::new(RwLock::new(HashMap::new())), ttl }
    }

    fn new_token() -> String {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    pub fn create(&self, user_id: i64, username: &str) -> String {
        let token = Self::new_token();
        let stored = StoredSession {
            user_id,
            username: username.to_string(),
            expires_at: Instant::now() + self.ttl,
        };
        self.inner.write().expect("session store poisoned").insert(token.clone(), stored);
        token
    }

    pub fn get(&self, token: &str) -> Option<Session> {
        // Fast path: read lock.
        {
            let map = self.inner.read().ok()?;
            match map.get(token) {
                Some(s) if s.expires_at > Instant::now() => {
                    return Some(Session { user_id: s.user_id, username: s.username.clone() });
                }
                Some(_) => {} // expired: fall through to remove
                None => return None,
            }
        }
        // Expired: drop it under a write lock.
        self.inner.write().ok()?.remove(token);
        None
    }

    pub fn remove(&self, token: &str) {
        if let Ok(mut map) = self.inner.write() {
            map.remove(token);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_then_get_returns_session() {
        let store = SessionStore::new(Duration::from_secs(60));
        let token = store.create(7, "alice");
        let session = store.get(&token).expect("valid session");
        assert_eq!(session.user_id, 7);
        assert_eq!(session.username, "alice");
    }

    #[test]
    fn unknown_token_returns_none() {
        let store = SessionStore::new(Duration::from_secs(60));
        assert!(store.get("deadbeef").is_none());
    }

    #[test]
    fn removed_session_is_gone() {
        let store = SessionStore::new(Duration::from_secs(60));
        let token = store.create(1, "bob");
        store.remove(&token);
        assert!(store.get(&token).is_none());
    }

    #[test]
    fn expired_session_returns_none() {
        let store = SessionStore::new(Duration::from_millis(0));
        let token = store.create(1, "bob");
        std::thread::sleep(Duration::from_millis(5));
        assert!(store.get(&token).is_none());
    }

    #[test]
    fn tokens_are_unique_and_long() {
        let store = SessionStore::new(Duration::from_secs(60));
        let a = store.create(1, "x");
        let b = store.create(1, "x");
        assert_ne!(a, b);
        assert_eq!(a.len(), 64); // 32 bytes hex
    }
}
```

Add `pub mod session;` to `src/lib.rs`.

- [ ] **Step 2: Run tests**

Run: `cargo test --manifest-path /Users/daniel/Projects/tolaria/src-tauri/Cargo.toml -p tolaria-server session`
Expected: PASS — 5 tests.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/crates/tolaria-server/src/session.rs src-tauri/crates/tolaria-server/src/lib.rs
git commit -m "feat(server): in-memory session store with opaque tokens + expiry"
```

---

### Task 4: Extend AppState with users + sessions + cookie_secure

**Files:**
- Modify: `src-tauri/crates/tolaria-server/src/rpc.rs` (AppState fields + constructor)

**Interfaces:**
- Produces: `AppState { pub vault_root: Arc<PathBuf>, pub users: users::UsersDb, pub sessions: session::SessionStore, pub cookie_secure: bool }` with `AppState::new(vault_root: PathBuf, users: UsersDb, sessions: SessionStore, cookie_secure: bool) -> Self`.
- Consumes: `crate::users::UsersDb`, `crate::session::SessionStore`.

> This changes `AppState::new`'s signature. `build_router` (Task 7) is the only caller and is updated there; the `command_route` handler is unaffected (still reads `state.vault_root`).

- [ ] **Step 1: Update AppState**

In `src-tauri/crates/tolaria-server/src/rpc.rs`, replace the `AppState` struct + impl:

```rust
use crate::session::SessionStore;
use crate::users::UsersDb;

/// Shared application state threaded through Axum handlers.
#[derive(Clone)]
pub struct AppState {
    pub vault_root: Arc<PathBuf>,
    pub users: UsersDb,
    pub sessions: SessionStore,
    pub cookie_secure: bool,
}

impl AppState {
    pub fn new(vault_root: PathBuf, users: UsersDb, sessions: SessionStore, cookie_secure: bool) -> Self {
        Self { vault_root: Arc::new(vault_root), users, sessions, cookie_secure }
    }
}
```

- [ ] **Step 2: Build (expect the known caller break, fixed in Task 7)**

Run: `cargo build --manifest-path /Users/daniel/Projects/tolaria/src-tauri/Cargo.toml -p tolaria-server`
Expected: FAIL — `build_router` in `lib.rs` still calls the old `AppState::new(vault_root)`. This is expected; Task 7 rewires `build_router`. To keep this task independently green, TEMPORARILY update the single call site in `lib.rs` `build_router` to construct the stores inline so it compiles:

```rust
    let users = crate::users::UsersDb::open(std::path::Path::new(":memory:")).expect("users db");
    let sessions = crate::session::SessionStore::new(std::time::Duration::from_secs(60 * 60 * 24 * 7));
    let state = rpc::AppState::new(vault_root, users, sessions, true);
```

> This inline stub is replaced properly in Task 7 (real config-driven wiring + auth routes/middleware). It exists only so Task 4 compiles and existing tests pass in isolation.

- [ ] **Step 3: Build + run existing tests**

Run: `cargo test --manifest-path /Users/daniel/Projects/tolaria/src-tauri/Cargo.toml -p tolaria-server`
Expected: PASS — existing handler/rpc/http tests still green with the extended state.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/crates/tolaria-server/src/rpc.rs src-tauri/crates/tolaria-server/src/lib.rs
git commit -m "feat(server): extend AppState with users, sessions, cookie_secure"
```

---

### Task 5: Auth routes + login page

**Files:**
- Create: `src-tauri/crates/tolaria-server/src/auth_routes.rs`
- Modify: `src-tauri/crates/tolaria-server/src/lib.rs` (`pub mod auth_routes;`)

**Interfaces:**
- Produces (all axum handlers taking `State<AppState>`):
  - `auth_routes::login_page() -> Html<String>` — `GET /login`, renders the HTML form (shows an error line when `?error=1`; takes `Query`).
  - `auth_routes::login(State, CookieJar, Form<LoginForm>) -> Response` — `POST /api/auth/login`; on success creates a session, adds the `tolaria_session` cookie, `303 → /`; on failure `303 → /login?error=1`.
  - `auth_routes::logout(State, CookieJar) -> Response` — `POST /api/auth/logout`; removes the session + clears the cookie, `303 → /login`.
  - `auth_routes::me(State, CookieJar) -> Response` — `GET /api/auth/me`; `200 {"username":...}` if session valid, else `401`.
  - Cookie name constant: `pub const SESSION_COOKIE: &str = "tolaria_session";`
- Consumes: `AppState.{users, sessions, cookie_secure}`.

- [ ] **Step 1: Write the failing tests (HTTP-level)**

Create `src-tauri/crates/tolaria-server/src/auth_routes.rs`:

```rust
use crate::rpc::AppState;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::Form;
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use serde::Deserialize;
use serde_json::json;

pub const SESSION_COOKIE: &str = "tolaria_session";

#[derive(Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct LoginQuery {
    pub error: Option<String>,
}

pub async fn login_page(Query(q): Query<LoginQuery>) -> Html<String> {
    let error_html = if q.error.is_some() {
        r#"<p class="err">Invalid username or password.</p>"#
    } else {
        ""
    };
    Html(format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><title>Tolaria — Sign in</title>
<meta name="viewport" content="width=device-width, initial-scale=1">
<style>body{{font-family:system-ui,sans-serif;background:#111;color:#eee;display:flex;min-height:100vh;align-items:center;justify-content:center;margin:0}}
form{{background:#1c1c1c;padding:2rem;border-radius:10px;width:300px}}
h1{{font-size:1.1rem;margin:0 0 1rem}} label{{display:block;font-size:.8rem;margin:.6rem 0 .2rem}}
input{{width:100%;padding:.5rem;border-radius:6px;border:1px solid #333;background:#111;color:#eee;box-sizing:border-box}}
button{{margin-top:1rem;width:100%;padding:.55rem;border:0;border-radius:6px;background:#4f46e5;color:#fff;font-weight:600;cursor:pointer}}
.err{{color:#f87171;font-size:.8rem}}</style></head>
<body><form method="post" action="/api/auth/login"><h1>Tolaria</h1>{error_html}
<label for="u">Username</label><input id="u" name="username" autocomplete="username" autofocus>
<label for="p">Password</label><input id="p" name="password" type="password" autocomplete="current-password">
<button type="submit">Sign in</button></form></body></html>"#
    ))
}

fn session_cookie(value: String, secure: bool) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, value))
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .path("/")
        .build()
}

pub async fn login(State(state): State<AppState>, jar: CookieJar, Form(form): Form<LoginForm>) -> Response {
    match state.users.verify_credentials(&form.username, &form.password) {
        Some(user) => {
            let token = state.sessions.create(user.id, &user.username);
            let jar = jar.add(session_cookie(token, state.cookie_secure));
            (jar, Redirect::to("/")).into_response()
        }
        None => Redirect::to("/login?error=1").into_response(),
    }
}

pub async fn logout(State(state): State<AppState>, jar: CookieJar) -> Response {
    if let Some(c) = jar.get(SESSION_COOKIE) {
        state.sessions.remove(c.value());
    }
    let jar = jar.remove(Cookie::from(SESSION_COOKIE));
    (jar, Redirect::to("/login")).into_response()
}

pub async fn me(State(state): State<AppState>, jar: CookieJar) -> Response {
    let session = jar.get(SESSION_COOKIE).and_then(|c| state.sessions.get(c.value()));
    match session {
        Some(s) => (StatusCode::OK, axum::Json(json!({ "username": s.username }))).into_response(),
        None => (StatusCode::UNAUTHORIZED, axum::Json(json!({ "error": "not authenticated" }))).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::SessionStore;
    use crate::users::UsersDb;
    use axum::body::Body;
    use axum::http::Request;
    use std::time::Duration;
    use tower::ServiceExt;

    fn test_state() -> AppState {
        let users = UsersDb::open_in_memory().unwrap();
        users.create_user("alice", "s3cret", "Alice", "a@example.com").unwrap();
        AppState::new(
            std::path::PathBuf::from("/tmp"),
            users,
            SessionStore::new(Duration::from_secs(60)),
            false,
        )
    }

    fn app(state: AppState) -> axum::Router {
        axum::Router::new()
            .route("/api/auth/login", axum::routing::post(login))
            .route("/api/auth/me", axum::routing::get(me))
            .with_state(state)
    }

    #[tokio::test]
    async fn good_login_sets_cookie_and_redirects() {
        let resp = app(test_state())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/login")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from("username=alice&password=s3cret"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
        let set_cookie = resp.headers().get("set-cookie").unwrap().to_str().unwrap();
        assert!(set_cookie.contains("tolaria_session="));
        assert!(set_cookie.contains("HttpOnly"));
    }

    #[tokio::test]
    async fn bad_login_redirects_to_error() {
        let resp = app(test_state())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/login")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from("username=alice&password=WRONG"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
        let loc = resp.headers().get("location").unwrap().to_str().unwrap();
        assert_eq!(loc, "/login?error=1");
        assert!(resp.headers().get("set-cookie").is_none());
    }

    #[tokio::test]
    async fn me_without_cookie_is_401() {
        let resp = app(test_state())
            .oneshot(Request::builder().uri("/api/auth/me").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
```

Add `pub mod auth_routes;` to `src/lib.rs`.

- [ ] **Step 2: Run tests**

Run: `cargo test --manifest-path /Users/daniel/Projects/tolaria/src-tauri/Cargo.toml -p tolaria-server auth_routes`
Expected: PASS — 3 async tests (good login sets cookie, bad login redirects, me 401).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/crates/tolaria-server/src/auth_routes.rs src-tauri/crates/tolaria-server/src/lib.rs
git commit -m "feat(server): auth routes (login/logout/me) + server-rendered login page"
```

---

### Task 6: Auth middleware

**Files:**
- Create: `src-tauri/crates/tolaria-server/src/auth_middleware.rs`
- Modify: `src-tauri/crates/tolaria-server/src/lib.rs` (`pub mod auth_middleware;`)

**Interfaces:**
- Produces: `auth_middleware::require_auth(State<AppState>, CookieJar, Request, Next) -> Response` — if the `tolaria_session` cookie resolves to a valid session, calls `next.run(req)`; otherwise: requests whose path starts with `/api/` get `401 {"error":"not authenticated"}`, all other paths get `303 → /login`.
- Consumes: `AppState.sessions`.

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/crates/tolaria-server/src/auth_middleware.rs`:

```rust
use crate::auth_routes::SESSION_COOKIE;
use crate::rpc::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::cookie::CookieJar;

pub async fn require_auth(
    State(state): State<AppState>,
    jar: CookieJar,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let authed = jar
        .get(SESSION_COOKIE)
        .and_then(|c| state.sessions.get(c.value()))
        .is_some();
    if authed {
        return next.run(request).await;
    }
    if request.uri().path().starts_with("/api/") {
        (StatusCode::UNAUTHORIZED, axum::Json(serde_json::json!({ "error": "not authenticated" }))).into_response()
    } else {
        Redirect::to("/login").into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::SessionStore;
    use crate::users::UsersDb;
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::get;
    use std::time::Duration;
    use tower::ServiceExt;

    fn guarded_app(state: AppState) -> axum::Router {
        axum::Router::new()
            .route("/api/cmd/x", get(|| async { "ok" }))
            .route("/somepage", get(|| async { "page" }))
            .layer(axum::middleware::from_fn_with_state(state.clone(), require_auth))
            .with_state(state)
    }

    fn state_with_session() -> (AppState, String) {
        let state = AppState::new(
            std::path::PathBuf::from("/tmp"),
            UsersDb::open_in_memory().unwrap(),
            SessionStore::new(Duration::from_secs(60)),
            false,
        );
        let token = state.sessions.create(1, "alice");
        (state, token)
    }

    #[tokio::test]
    async fn unauthenticated_api_is_401() {
        let (state, _t) = state_with_session();
        let resp = guarded_app(state)
            .oneshot(Request::builder().uri("/api/cmd/x").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn unauthenticated_page_redirects_to_login() {
        let (state, _t) = state_with_session();
        let resp = guarded_app(state)
            .oneshot(Request::builder().uri("/somepage").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
        assert_eq!(resp.headers().get("location").unwrap().to_str().unwrap(), "/login");
    }

    #[tokio::test]
    async fn authenticated_request_passes() {
        let (state, token) = state_with_session();
        let resp = guarded_app(state)
            .oneshot(
                Request::builder()
                    .uri("/api/cmd/x")
                    .header("cookie", format!("{SESSION_COOKIE}={token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
```

Add `pub mod auth_middleware;` to `src/lib.rs`.

- [ ] **Step 2: Run tests**

Run: `cargo test --manifest-path /Users/daniel/Projects/tolaria/src-tauri/Cargo.toml -p tolaria-server auth_middleware`
Expected: PASS — 3 tests (401 for unauth api, redirect for unauth page, pass when authed).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/crates/tolaria-server/src/auth_middleware.rs src-tauri/crates/tolaria-server/src/lib.rs
git commit -m "feat(server): require_auth middleware (401 for api, redirect for pages)"
```

---

### Task 7: Wire auth into `build_router` (public vs protected split)

**Files:**
- Modify: `src-tauri/crates/tolaria-server/src/lib.rs`
- Modify: `src-tauri/crates/tolaria-server/tests/rpc_http.rs` (existing integration test now needs a session)

**Interfaces:**
- Produces: `pub fn build_router(vault_root: PathBuf, static_dir: PathBuf, users: UsersDb, sessions: SessionStore, cookie_secure: bool) -> Router` — public routes (`GET /login`, `POST /api/auth/login`) merged with protected routes (everything else) guarded by `require_auth`.
- Consumes: `auth_routes`, `auth_middleware`, `UsersDb`, `SessionStore`.

- [ ] **Step 1: Rewrite `build_router`**

Replace the `build_router` body in `src/lib.rs` (remove the Task 4 inline stub) with:

```rust
use crate::session::SessionStore;
use crate::users::UsersDb;

/// Build the full application router.
/// `static_dir` is the compiled SPA directory served as the fallback.
pub fn build_router(
    vault_root: PathBuf,
    static_dir: PathBuf,
    users: UsersDb,
    sessions: SessionStore,
    cookie_secure: bool,
) -> Router {
    let state = rpc::AppState::new(vault_root, users, sessions, cookie_secure);

    // Public routes: reachable without a session.
    let public = Router::new()
        .route("/login", axum::routing::get(auth_routes::login_page))
        .route("/api/auth/login", post(auth_routes::login));

    // Protected routes: require a valid session (via require_auth layer).
    let protected = Router::new()
        .route("/api/cmd/:command", post(rpc::command_route))
        .route("/api/auth/logout", post(auth_routes::logout))
        .route("/api/auth/me", axum::routing::get(auth_routes::me))
        .route("/api/*path", axum::routing::any(static_files::api_not_found))
        .fallback_service(static_files::service(static_dir))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth_middleware::require_auth,
        ));

    public.merge(protected).with_state(state)
}
```

Ensure `src/lib.rs` declares all modules: `pub mod auth_middleware; pub mod auth_routes; pub mod config; pub mod handlers; pub mod rpc; pub mod session; pub mod static_files; pub mod users;`.

- [ ] **Step 2: Update `main.rs` to construct and pass the stores**

In `src/main.rs`, the serve path must open the users DB and create the session store from config. Replace the router construction so it reads `cfg.users_db_path` / `cfg.cookie_secure` and passes a 7-day session TTL:

```rust
    let users = tolaria_server::users::UsersDb::open(&cfg.users_db_path)
        .unwrap_or_else(|e| { tracing::error!("users db: {e}"); std::process::exit(1); });
    let sessions = tolaria_server::session::SessionStore::new(std::time::Duration::from_secs(60 * 60 * 24 * 7));
    let app = tolaria_server::build_router(cfg.vault_path, cfg.static_dir, users, sessions, cfg.cookie_secure);
```

(Keep the existing listener/serve lines.) If `main.rs` currently calls `build_router(cfg.vault_path, cfg.static_dir)`, replace that call.

- [ ] **Step 3: Update the existing HTTP integration test to authenticate**

In `src-tauri/crates/tolaria-server/tests/rpc_http.rs`, the `list_vault` call now goes through auth. Update the test helper to build the router with a user + a pre-created session and send the cookie. Replace the router construction with:

```rust
    let users = tolaria_server::users::UsersDb::open_in_memory().unwrap();
    let sessions = tolaria_server::session::SessionStore::new(std::time::Duration::from_secs(60));
    let token = sessions.create(1, "tester");
    let app = tolaria_server::build_router(dir.path().to_path_buf(), dir.path().to_path_buf(), users, sessions, false);
```

and add the cookie header to the request:

```rust
        .header("cookie", format!("tolaria_session={token}"))
```

> `open_in_memory` is `#[cfg(test)]`-only on `UsersDb`; integration tests are a separate crate compilation, so add a non-test constructor if needed. To avoid exposing test-only API across the crate boundary, instead use `UsersDb::open` with a temp file path in the integration test: `let db_path = dir.path().join("users.db"); let users = UsersDb::open(&db_path).unwrap();`. Use that form in the integration test.

Also add a new integration test asserting auth is enforced end-to-end:

```rust
#[tokio::test]
async fn list_vault_without_session_is_401() {
    let dir = tempfile::tempdir().unwrap();
    let users = tolaria_server::users::UsersDb::open(&dir.path().join("users.db")).unwrap();
    let sessions = tolaria_server::session::SessionStore::new(std::time::Duration::from_secs(60));
    let app = tolaria_server::build_router(dir.path().to_path_buf(), dir.path().to_path_buf(), users, sessions, false);
    let req = axum::http::Request::builder()
        .method("POST").uri("/api/cmd/list_vault")
        .header("content-type", "application/json")
        .body(axum::body::Body::from("{\"path\":\"/\"}")).unwrap();
    let resp = tower::ServiceExt::oneshot(app, req).await.unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::UNAUTHORIZED);
}
```

- [ ] **Step 4: Build + full server suite**

Run: `cargo test --manifest-path /Users/daniel/Projects/tolaria/src-tauri/Cargo.toml -p tolaria-server`
Expected: PASS — unit tests + updated integration test (authed list_vault 200) + new unauth-401 test.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/crates/tolaria-server/src/lib.rs src-tauri/crates/tolaria-server/src/main.rs src-tauri/crates/tolaria-server/tests/rpc_http.rs
git commit -m "feat(server): gate routes behind auth; public login, protected everything else"
```

---

### Task 8: `useradd` CLI subcommand + transport 401 handling

**Files:**
- Modify: `src-tauri/crates/tolaria-server/src/main.rs` (subcommand dispatch)
- Modify: `src/web/transport.ts` (redirect to /login on 401)
- Test: `src-tauri/crates/tolaria-server/src/main.rs` is a binary; put the reusable logic in a testable function and unit-test it.

**Interfaces:**
- Produces: running `tolaria-server useradd <username> <git_name> <git_email>` reads a password from the `TOLARIA_NEW_PASSWORD` env var (avoids TTY complexity in containers), opens the configured users DB, and creates the user. `tolaria-server` with no subcommand (or `serve`) runs the server as today.
- A testable helper `run_useradd(users: &UsersDb, username, password, git_name, git_email) -> Result<(), String>`.

- [ ] **Step 1: Write the failing test for the helper**

Add to `src-tauri/crates/tolaria-server/src/users.rs` (co-located with the store) a thin helper + test, OR add a small `cli` module. Put it in `users.rs`:

```rust
/// Create a user, rejecting empty inputs. Used by the `useradd` CLI.
pub fn run_useradd(
    users: &UsersDb,
    username: &str,
    password: &str,
    git_name: &str,
    git_email: &str,
) -> Result<(), String> {
    if username.trim().is_empty() || password.is_empty() {
        return Err("username and password must not be empty".to_string());
    }
    users.create_user(username, password, git_name, git_email)
}
```

Test (add to the `users.rs` tests module):

```rust
    #[test]
    fn run_useradd_creates_verifiable_user() {
        let db = UsersDb::open_in_memory().unwrap();
        run_useradd(&db, "eve", "pw12345", "Eve", "eve@example.com").unwrap();
        assert!(db.verify_credentials("eve", "pw12345").is_some());
    }

    #[test]
    fn run_useradd_rejects_empty_password() {
        let db = UsersDb::open_in_memory().unwrap();
        assert!(run_useradd(&db, "eve", "", "Eve", "eve@example.com").is_err());
    }
```

- [ ] **Step 2: Run the helper test**

Run: `cargo test --manifest-path /Users/daniel/Projects/tolaria/src-tauri/Cargo.toml -p tolaria-server run_useradd`
Expected: PASS — 2 tests.

- [ ] **Step 3: Wire the subcommand in `main.rs`**

Update `main.rs` so `main` dispatches on the first CLI arg. Keep the async serve path; run `useradd` synchronously before the runtime if chosen:

```rust
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("useradd") {
        // tolaria-server useradd <username> <git_name> <git_email>   (password via TOLARIA_NEW_PASSWORD)
        let cfg = tolaria_server::config::ServerConfig::from_env()
            .unwrap_or_else(|e| { eprintln!("config error: {e}"); std::process::exit(1); });
        let username = args.get(2).cloned().unwrap_or_default();
        let git_name = args.get(3).cloned().unwrap_or_else(|| username.clone());
        let git_email = args.get(4).cloned().unwrap_or_default();
        let password = std::env::var("TOLARIA_NEW_PASSWORD").unwrap_or_default();
        let users = tolaria_server::users::UsersDb::open(&cfg.users_db_path)
            .unwrap_or_else(|e| { eprintln!("users db: {e}"); std::process::exit(1); });
        match tolaria_server::users::run_useradd(&users, &username, &password, &git_name, &git_email) {
            Ok(()) => { println!("created user '{username}'"); }
            Err(e) => { eprintln!("useradd failed: {e}"); std::process::exit(1); }
        }
        return;
    }
    serve();
}

#[tokio::main]
async fn serve() {
    // ... existing tracing init + config + build_router + axum::serve, using the Task 7 wiring ...
}
```

> `useradd` requires `TOLARIA_VAULT_PATH` to be set only because `ServerConfig` requires it; that is acceptable in the container (it's always set). If you prefer, read `TOLARIA_USERS_DB` directly for `useradd` to avoid needing the vault path — either is fine; keep it simple and reuse `ServerConfig`.

- [ ] **Step 4: transport 401 → redirect to /login**

In `src/web/transport.ts`, in `invoke`, before the generic `!res.ok` throw, handle 401 so an expired session sends the user back to login:

```ts
  if (res.status === 401) {
    if (typeof window !== 'undefined') window.location.assign('/login')
    return undefined as T
  }
```

(Place it after the existing `res.status === 501` block.)

- [ ] **Step 5: Build server + run vitest for transport**

Run: `cargo build --manifest-path /Users/daniel/Projects/tolaria/src-tauri/Cargo.toml -p tolaria-server`
Run: `pnpm vitest run src/web/transport.test.ts`
Expected: both PASS. (Add a transport test for the 401 case: mock `fetch` → 401, assert `invoke` resolves `undefined`; stub `window.location.assign` via `vi.stubGlobal` or asserting no throw.)

Add to `src/web/transport.test.ts`:

```ts
  it('returns undefined and redirects on 401', async () => {
    const assign = vi.fn()
    vi.stubGlobal('location', { assign } as unknown as Location)
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('{}', { status: 401 })))
    const result = await invoke('list_vault', { path: '/v' })
    expect(result).toBeUndefined()
    expect(assign).toHaveBeenCalledWith('/login')
  })
```

- [ ] **Step 6: Commit**

```bash
git add src-tauri/crates/tolaria-server/src/main.rs src-tauri/crates/tolaria-server/src/users.rs src/web/transport.ts src/web/transport.test.ts
git commit -m "feat(server): useradd CLI subcommand; web transport redirects to /login on 401"
```

---

### Task 9: Docker/compose wiring, ADR, docs, gate verification

**Files:**
- Modify: `Dockerfile` (users DB dir + cookie-secure default)
- Modify: `docker-compose.yml` (users DB volume + `TOLARIA_COOKIE_SECURE`)
- Create: `docs/adr/0148-web-server-builtin-auth.md`
- Modify: `docs/ARCHITECTURE.md` (auth subsection)

- [ ] **Step 1: Dockerfile — persist users DB**

In `Dockerfile`, add to the runtime `ENV` block `TOLARIA_USERS_DB=/app/data/users.db` and ensure `/app/data` exists:

```dockerfile
ENV TOLARIA_STATIC_DIR=/app/dist \
    TOLARIA_HOST=0.0.0.0 \
    TOLARIA_PORT=8787 \
    TOLARIA_USERS_DB=/app/data/users.db
RUN mkdir -p /app/data
```

(Do NOT re-add `TOLARIA_VAULT_PATH` — it stays required/compose-supplied.)

- [ ] **Step 2: docker-compose — volume + env**

In `docker-compose.yml`, add a named volume for the users DB and mount it, plus the cookie-secure toggle (default true; set false only when testing without TLS):

```yaml
    environment:
      TOLARIA_VAULT_PATH: /vault
      TOLARIA_USERS_DB: /app/data/users.db
      TOLARIA_COOKIE_SECURE: ${TOLARIA_COOKIE_SECURE:-true}
    volumes:
      - ${TOLARIA_VAULT_MOUNT:-./demo-vault-v2}:/vault:ro
      - tolaria_users:/app/data
```

and add at the bottom:

```yaml
volumes:
  tolaria_users:
```

- [ ] **Step 3: Create ADR 0148**

Write `docs/adr/0148-web-server-builtin-auth.md` matching the existing ADR frontmatter format (see `0147-web-server-read-only-phase.md`): `type: ADR`, `id: "0148"`, `status: active`, `date: 2026-07-01`. Record: built-in auth for the web server — SQLite user store (argon2), in-memory sessions with an opaque-token HttpOnly/SameSite=Lax/Secure cookie, server-rendered `/login` page, `require_auth` middleware (401 for `/api/*`, 302→`/login` for pages), `useradd` CLI provisioning (no self-service signup), per-user git identity stored for later phases, and the login page living outside the React i18n system. Note that `TOLARIA_COOKIE_SECURE=false` is only for non-TLS local testing.

- [ ] **Step 4: Update ARCHITECTURE.md**

Add an "Authentication (Phase 3)" subsection under the web-server section describing the user store, session model, cookie attributes, route protection, and `useradd`.

- [ ] **Step 5: Gate verification**

Run and confirm pass:
```bash
cargo clippy --manifest-path /Users/daniel/Projects/tolaria/src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path /Users/daniel/Projects/tolaria/src-tauri/Cargo.toml -p tolaria-server
pnpm vitest run src/web/transport.test.ts
```
(`cargo llvm-cov`, CodeScene, Codacy run via the pre-push sidecar — do not run locally.)

- [ ] **Step 6: Commit**

```bash
git add Dockerfile docker-compose.yml docs/
git commit -m "feat(docker,docs): persist users db, cookie-secure toggle; ADR 0148 + auth docs"
```

---

## Self-Review

**Spec coverage (§5.1, §5.2, §13 Phase 3):**
- `/api/auth/login` (POST, sets cookie) → Task 5. `/api/auth/logout` → Task 5. `/api/auth/me` → Task 5. → §5.1.
- Authenticated `/api/cmd/:command` → Task 6 middleware + Task 7 wiring. → §5.1.
- Built-in user store, argon2-hashed → Task 2. → §5.2.
- Session cookie HttpOnly+Secure+SameSite → Task 5 `session_cookie` + Task 1 `cookie_secure`. → §5.2.
- Per-user git identity stored → Task 2 (`git_name`/`git_email` columns), unused until Phase 4. → §5.2.
- No self-service signup; admin provisioning → Task 8 `useradd`. → §5.2.
- Login UI (chosen: server-rendered page) → Task 5 `login_page`. Session storage (chosen: in-memory) → Task 3. User store (chosen: SQLite) → Task 2.
- `/api/events` WebSocket is Phase 5 — correctly absent.

**Placeholder scan:** No TBD/vague steps. The one deliberately-flagged choice (Task 4 inline stub replaced in Task 7) is explicit about why it exists and where it's removed. Task 7 Step 3 explicitly resolves the test-only `open_in_memory` visibility issue by using `UsersDb::open` with a temp path in the integration test.

**Type consistency:** `AppState::new(vault_root, users, sessions, cookie_secure)` defined in Task 4, consumed in Tasks 5/6/7. `UsersDb::{open, open_in_memory, create_user, verify_credentials, run_useradd}` defined Task 2/8, used Tasks 5/7/8. `SessionStore::{new, create, get, remove}` defined Task 3, used Tasks 5/6/7. `SESSION_COOKIE` defined Task 5, used Tasks 5/6. `build_router(vault_root, static_dir, users, sessions, cookie_secure)` defined Task 7, used by main.rs (Task 7) + integration test (Task 7) + must be the signature Docker's runtime relies on.

**Security note:** the vault-root containment from Phase 2 remains; Phase 3 adds authentication in front of it. Login failures are indistinguishable (single "invalid username or password") — no user enumeration. Passwords never logged (verify returns a record, not the hash).
