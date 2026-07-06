# Web Server Git Sync Controls Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the web server git sync controls — per-save commits authored by the acting user, plus explicit commit / push / pull / status / conflict-resolution — over the same `tolaria-core::git` code the desktop app uses, serialized by a repo-level lock.

**Architecture:** The frontend already calls `git_commit`, `git_push`, `git_pull`, `git_remote_status`, `git_author_identity`, `git_resolve_conflict`, and `git_commit_conflict_resolution` via the transport. The server implements them by dispatching to existing `tolaria_core::git` functions. All index-mutating operations (commit, push, pull, conflict resolution, and the autogit commit after each write) run behind a single repo-level `tokio::Mutex` so git's index is never touched concurrently. Each commit is authored by the acting user (resolved from their session → `users.git_name`/`git_email`) while the committer is a fixed server identity. This is Phase 5 of the web-client work (spec §8, §13 item 5).

**Tech Stack:** Rust, Axum 0.7, `tolaria-core` (git via shelling to the `git` CLI), rusqlite user store, React + TypeScript transport shim, Vitest, `cargo test` / `cargo llvm-cov`.

## Global Constraints

- **Reuse `tolaria-core::git`** — do not reimplement git logic in the server crate; the server is a thin dispatcher. Extend core only where an author/committer override or path-scoped commit is genuinely missing.
- **Never** add `#[allow(...)]`, `// eslint-disable`, or `as any`. (AGENTS.md)
- **TDD, one Red→Green→Refactor cycle per commit.** For bugs write the failing regression test first. (AGENTS.md)
- **Rust line coverage ≥ 85%** (`cargo llvm-cov --manifest-path src-tauri/Cargo.toml --no-clean --fail-under-lines 85`); **frontend coverage ≥ 70%**.
- **CodeScene:** every touched file must leave with an equal-or-higher score; new scorable files must reach `10.0` or have zero findings. Never edit `.codescene-thresholds` downward.
- **Commit granularity decision (locked):** per-save, path-scoped commit authored by the acting user (`git add <path>` + commit with `--author`/`GIT_AUTHOR_*`). Never `git add -A` for the automatic autogit path. Committer is a fixed server identity.
- **Real-time WebSocket events (§5.4) are OUT of scope for this phase** — deferred to a later plan slice. Clients refresh git status on explicit action.
- **Repo-level lock ordering:** acquire the per-path write lock first (existing `AppState::locks`), complete the file write and release it, then acquire the repo lock for the commit. Never hold a path lock while awaiting the repo lock inside another path's critical section, to avoid lock-order inversion.
- All new user-facing copy (if any) lives in `src/lib/locales/en.json` and is translated via `pnpm l10n:translate`. This phase is expected to add no new copy (git controls already exist in the desktop UI).

---

### Task 1: Core — author/committer-scoped commit of specific paths

Add a `tolaria_core::git` function that stages only the given paths and commits them with an explicit author and committer, leaving the existing `git_commit` (used by desktop) untouched. This is the primitive the server uses for both the per-save autogit commit and the explicit web commit.

**Files:**
- Modify: `src-tauri/crates/tolaria-core/src/git/commit.rs`
- Modify: `src-tauri/crates/tolaria-core/src/git/mod.rs` (export the new fn + struct)

**Interfaces:**
- Consumes: existing `git_command()` (private, in `git/mod.rs`), `ensure_author_config`, `run_commit` helpers in `commit.rs`.
- Produces:
  - `pub struct CommitIdentity { pub author_name: String, pub author_email: String, pub committer_name: String, pub committer_email: String }`
  - `pub fn git_commit_paths_as(vault_path: &str, paths: &[String], message: &str, identity: &CommitIdentity) -> Result<String, String>` — stages exactly `paths` (via `git add -- <paths>`; empty `paths` means "stage nothing new", used by callers that already staged), commits with the identity applied through `GIT_AUTHOR_NAME/EMAIL` + `GIT_COMMITTER_NAME/EMAIL` env vars, retries unsigned on a signing failure (reuse `is_commit_signing_failure`), and maps "nothing to commit" to `Ok("")` (autogit no-op) rather than an error.
  - `pub fn git_commit_all_as(vault_path: &str, message: &str, identity: &CommitIdentity) -> Result<String, String>` — like the existing `git_commit` (`git add -A`) but with the identity override; used by the explicit web "commit" control. Returns the commit stdout; "nothing to commit" is an `Err` here (explicit commit should report it).

- [ ] **Step 1: Write the failing test — path-scoped commit uses the given author, not git config**

Add to the `tests` module in `src-tauri/crates/tolaria-core/src/git/commit.rs`:

```rust
#[test]
fn git_commit_paths_as_uses_supplied_author_and_committer() {
    let _env = GitConfigEnvGuard::isolated();
    let dir = setup_git_repo();
    let vault = dir.path();
    let vp = vault.to_str().unwrap();

    fs::write(vault.join("alice-note.md"), "# Alice\n").unwrap();
    // A second unrelated edit must NOT be swept into Alice's scoped commit.
    fs::write(vault.join("other.md"), "# Other\n").unwrap();

    let identity = CommitIdentity {
        author_name: "Alice".into(),
        author_email: "alice@example.com".into(),
        committer_name: "Tolaria Server".into(),
        committer_email: "server@tolaria.local".into(),
    };
    let out = git_commit_paths_as(vp, &["alice-note.md".into()], "add alice note", &identity);
    assert!(out.is_ok(), "scoped commit should succeed: {out:?}");

    let author = git_command()
        .args(["log", "-1", "--format=%an <%ae> | %cn <%ce>"])
        .current_dir(vault)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&author.stdout).trim(),
        "Alice <alice@example.com> | Tolaria Server <server@tolaria.local>"
    );

    // other.md must still be uncommitted (not staged by the scoped commit).
    let status = git_command()
        .args(["status", "--porcelain", "--", "other.md"])
        .current_dir(vault)
        .output()
        .unwrap();
    assert!(
        !String::from_utf8_lossy(&status.stdout).trim().is_empty(),
        "other.md must remain uncommitted"
    );
}

#[test]
fn git_commit_paths_as_nothing_to_commit_is_ok_empty() {
    let _env = GitConfigEnvGuard::isolated();
    let dir = setup_git_repo();
    let vault = dir.path();
    let vp = vault.to_str().unwrap();
    fs::write(vault.join("x.md"), "# X\n").unwrap();
    let identity = CommitIdentity {
        author_name: "Bob".into(),
        author_email: "bob@example.com".into(),
        committer_name: "Tolaria Server".into(),
        committer_email: "server@tolaria.local".into(),
    };
    git_commit_paths_as(vp, &["x.md".into()], "first", &identity).unwrap();
    // No new changes → autogit no-op → Ok("").
    let again = git_commit_paths_as(vp, &["x.md".into()], "again", &identity).unwrap();
    assert_eq!(again, "");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tolaria-core git_commit_paths_as 2>&1 | tail -20`
Expected: FAIL — `cannot find function 'git_commit_paths_as'` / `cannot find type 'CommitIdentity'`.

- [ ] **Step 3: Implement `CommitIdentity`, `git_commit_paths_as`, `git_commit_all_as`**

In `src-tauri/crates/tolaria-core/src/git/commit.rs`, add near the top (after the `use` lines):

```rust
use std::process::Command;

/// Author + committer identities applied to a server-side commit. The author is
/// the acting user; the committer is a fixed server identity (spec §8).
#[derive(Debug, Clone)]
pub struct CommitIdentity {
    pub author_name: String,
    pub author_email: String,
    pub committer_name: String,
    pub committer_email: String,
}

impl CommitIdentity {
    /// Apply the identity to a git subprocess via the standard git env vars.
    fn apply(&self, command: &mut Command) {
        command
            .env("GIT_AUTHOR_NAME", &self.author_name)
            .env("GIT_AUTHOR_EMAIL", &self.author_email)
            .env("GIT_COMMITTER_NAME", &self.committer_name)
            .env("GIT_COMMITTER_EMAIL", &self.committer_email);
    }
}

/// Stage exactly `paths` and commit them as `identity`. "Nothing to commit"
/// resolves to `Ok(String::new())` so the per-save autogit path is a no-op when
/// the file content did not actually change. Empty `paths` stages nothing new.
pub fn git_commit_paths_as(
    vault_path: &str,
    paths: &[String],
    message: &str,
    identity: &CommitIdentity,
) -> Result<String, String> {
    let vault = Path::new(vault_path);
    if !paths.is_empty() {
        let mut add = git_command();
        add.args(["add", "--"]);
        add.args(paths);
        let add = add
            .current_dir(vault)
            .output()
            .map_err(|e| format!("Failed to run git add: {}", e))?;
        if !add.status.success() {
            return Err(format!(
                "git add failed: {}",
                String::from_utf8_lossy(&add.stderr)
            ));
        }
    }
    match run_commit_as(vault, message, identity, false) {
        Ok(stdout) => Ok(stdout),
        Err(failure) if is_nothing_to_commit(&failure.detail()) => Ok(String::new()),
        Err(failure) if is_commit_signing_failure(&failure.detail()) => {
            run_commit_as(vault, message, identity, true)
                .map(|s| s)
                .or_else(|f| {
                    if is_nothing_to_commit(&f.detail()) {
                        Ok(String::new())
                    } else {
                        Err(format!("git commit failed: {}", f.detail()))
                    }
                })
        }
        Err(failure) => Err(format!("git commit failed: {}", failure.detail())),
    }
}

/// Stage all changes (`git add -A`) and commit as `identity`. Used by the
/// explicit web "commit" control; "nothing to commit" is a real error here.
pub fn git_commit_all_as(
    vault_path: &str,
    message: &str,
    identity: &CommitIdentity,
) -> Result<String, String> {
    let vault = Path::new(vault_path);
    let add = git_command()
        .args(["add", "-A"])
        .current_dir(vault)
        .output()
        .map_err(|e| format!("Failed to run git add: {}", e))?;
    if !add.status.success() {
        return Err(format!(
            "git add failed: {}",
            String::from_utf8_lossy(&add.stderr)
        ));
    }
    match run_commit_as(vault, message, identity, false) {
        Ok(stdout) => Ok(stdout),
        Err(failure) if is_commit_signing_failure(&failure.detail()) => {
            run_commit_as(vault, message, identity, true)
                .map_err(|f| format!("git commit failed: {}", f.detail()))
        }
        Err(failure) => Err(format!("git commit failed: {}", failure.detail())),
    }
}

fn run_commit_as(
    vault: &Path,
    message: &str,
    identity: &CommitIdentity,
    disable_signing: bool,
) -> Result<String, CommitFailure> {
    let mut command = git_command();
    identity.apply(&mut command);
    if disable_signing {
        command.args(["-c", "commit.gpgsign=false"]);
    }
    let commit = command
        .args(["commit", "-m", message])
        .current_dir(vault)
        .output()
        .map_err(|e| CommitFailure {
            stdout: String::new(),
            stderr: format!("Failed to run git commit: {}", e),
        })?;
    if commit.status.success() {
        return Ok(String::from_utf8_lossy(&commit.stdout).to_string());
    }
    Err(CommitFailure {
        stdout: String::from_utf8_lossy(&commit.stdout).to_string(),
        stderr: String::from_utf8_lossy(&commit.stderr).to_string(),
    })
}

fn is_nothing_to_commit(detail: &str) -> bool {
    detail.to_ascii_lowercase().contains("nothing to commit")
}
```

Then in `src-tauri/crates/tolaria-core/src/git/mod.rs`, extend the commit re-export:

```rust
pub use commit::{git_commit, git_commit_all_as, git_commit_paths_as, CommitIdentity};
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tolaria-core commit:: 2>&1 | tail -20`
Expected: PASS — all `commit::tests::*` including the two new tests.

- [ ] **Step 5: Verify clippy + CodeScene are clean on the touched file**

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml -p tolaria-core 2>&1 | grep -cE 'warning|error'`
Expected: `0`. Then run the CodeScene file-level review on `commit.rs` and confirm the score did not drop.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/crates/tolaria-core/src/git/commit.rs src-tauri/crates/tolaria-core/src/git/mod.rs
git commit -m "feat(core): path-scoped and identity-overriding git commits"
```

---

### Task 2: Server — look up a user's git identity by id

The session store only carries `user_id` + `username`. To author commits we need the user's `git_name`/`git_email` from the `users` table.

**Files:**
- Modify: `src-tauri/crates/tolaria-server/src/users.rs`

**Interfaces:**
- Consumes: existing `UsersDb`, `UserRecord { id, username, git_name, git_email }`.
- Produces: `pub fn UsersDb::find_by_id(&self, id: i64) -> Option<UserRecord>`.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `src-tauri/crates/tolaria-server/src/users.rs`:

```rust
#[test]
fn find_by_id_returns_git_identity() {
    let db = UsersDb::open_in_memory().unwrap();
    db.create_user("carol", "pw-carol-123", "Carol Q", "carol@example.com")
        .unwrap();
    let created = db.verify_credentials("carol", "pw-carol-123").unwrap();

    let found = db.find_by_id(created.id).expect("user exists");
    assert_eq!(found.username, "carol");
    assert_eq!(found.git_name, "Carol Q");
    assert_eq!(found.git_email, "carol@example.com");

    assert!(db.find_by_id(9999).is_none());
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tolaria-server find_by_id 2>&1 | tail -15`
Expected: FAIL — `no method named 'find_by_id'`.

- [ ] **Step 3: Implement `find_by_id`**

Add to `impl UsersDb` in `src-tauri/crates/tolaria-server/src/users.rs`:

```rust
/// Look up a user's public record (including git identity) by id. Returns
/// `None` if no such user exists.
pub fn find_by_id(&self, id: i64) -> Option<UserRecord> {
    let conn = self.conn.lock().ok()?;
    conn.query_row(
        "SELECT id, username, git_name, git_email FROM users WHERE id = ?1",
        rusqlite::params![id],
        |r| {
            Ok(UserRecord {
                id: r.get(0)?,
                username: r.get(1)?,
                git_name: r.get(2)?,
                git_email: r.get(3)?,
            })
        },
    )
    .ok()
}
```

> Note: if `self.conn` is not a `Mutex<Connection>`, match the existing locking pattern used by `verify_credentials` in this file instead of `self.conn.lock()`.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tolaria-server find_by_id 2>&1 | tail -15`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/crates/tolaria-server/src/users.rs
git commit -m "feat(server): UsersDb::find_by_id for commit authorship"
```

---

### Task 3: Server — repo-level lock and committer config in AppState

Add the single repo-level lock that serializes all index-mutating git operations, plus the fixed server committer identity, to `AppState`.

**Files:**
- Modify: `src-tauri/crates/tolaria-server/src/rpc.rs`
- Modify: `src-tauri/crates/tolaria-server/src/config.rs` (committer identity + autogit toggle from env)
- Modify call sites of `AppState::new` (compile-fix): `src/lib.rs`, `auth_routes.rs`, `auth_middleware.rs` tests, `write_handlers.rs` tests, `handlers.rs` tests if any.

**Interfaces:**
- Consumes: `tokio::sync::Mutex`, `std::sync::Arc`.
- Produces:
  - `AppState.repo_lock: Arc<tokio::sync::Mutex<()>>`
  - `AppState.committer: Arc<tolaria_core::git::CommitIdentity>`-style config — store `committer_name: Arc<str>`, `committer_email: Arc<str>`, and `autogit: bool`.
  - `AppState::new(...)` gains parameters `committer_name: String`, `committer_email: String`, `autogit: bool`.
  - `pub fn AppState::acting_user(&self, jar: &CookieJar) -> Option<tolaria_core::git::CommitIdentity>` — resolves the session → `user_id` → `find_by_id` → builds a `CommitIdentity` with the server committer. Returns `None` if unauthenticated (should not happen behind `require_auth`, but handled defensively).

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `src-tauri/crates/tolaria-server/src/rpc.rs`:

```rust
use crate::session::SessionStore;
use crate::users::UsersDb;
use axum_extra::extract::cookie::{Cookie, CookieJar};
use std::time::Duration;

fn state_for_identity() -> (AppState, String) {
    let users = UsersDb::open_in_memory().unwrap();
    users
        .create_user("dora", "pw-dora-1234", "Dora D", "dora@example.com")
        .unwrap();
    let rec = users.verify_credentials("dora", "pw-dora-1234").unwrap();
    let sessions = SessionStore::new(Duration::from_secs(60));
    let token = sessions.create(rec.id, "dora");
    let state = AppState::new(
        std::path::PathBuf::from("/tmp"),
        users,
        sessions,
        false,
        PathLocks::new(),
        "Tolaria Server".to_string(),
        "server@tolaria.local".to_string(),
        true,
    );
    (state, token)
}

#[test]
fn acting_user_resolves_author_from_session() {
    let (state, token) = state_for_identity();
    let jar = CookieJar::new().add(Cookie::new(
        crate::auth_routes::SESSION_COOKIE,
        token,
    ));
    let id = state.acting_user(&jar).expect("resolves");
    assert_eq!(id.author_name, "Dora D");
    assert_eq!(id.author_email, "dora@example.com");
    assert_eq!(id.committer_name, "Tolaria Server");
    assert_eq!(id.committer_email, "server@tolaria.local");
}

#[test]
fn acting_user_none_without_session() {
    let (state, _t) = state_for_identity();
    assert!(state.acting_user(&CookieJar::new()).is_none());
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tolaria-server acting_user 2>&1 | tail -20`
Expected: FAIL — `AppState::new` arity mismatch / no method `acting_user`.

- [ ] **Step 3: Extend `AppState`**

In `src-tauri/crates/tolaria-server/src/rpc.rs`, update the struct, constructor, and add the resolver:

```rust
use axum_extra::extract::cookie::CookieJar;
use tolaria_core::git::CommitIdentity;

#[derive(Clone)]
pub struct AppState {
    pub vault_root: Arc<PathBuf>,
    pub users: UsersDb,
    pub sessions: SessionStore,
    pub cookie_secure: bool,
    pub locks: PathLocks,
    pub repo_lock: Arc<tokio::sync::Mutex<()>>,
    pub committer_name: Arc<str>,
    pub committer_email: Arc<str>,
    pub autogit: bool,
}

impl AppState {
    #[allow(clippy::too_many_arguments)] // DO NOT KEEP — see note below
    pub fn new(
        vault_root: PathBuf,
        users: UsersDb,
        sessions: SessionStore,
        cookie_secure: bool,
        locks: PathLocks,
        committer_name: String,
        committer_email: String,
        autogit: bool,
    ) -> Self {
        Self {
            vault_root: Arc::new(vault_root),
            users,
            sessions,
            cookie_secure,
            locks,
            repo_lock: Arc::new(tokio::sync::Mutex::new(())),
            committer_name: committer_name.into(),
            committer_email: committer_email.into(),
            autogit,
        }
    }

    /// Resolve the acting user's commit identity from the session cookie.
    /// Author = the user's git identity; committer = the fixed server identity.
    pub fn acting_user(&self, jar: &CookieJar) -> Option<CommitIdentity> {
        let session = jar
            .get(crate::auth_routes::SESSION_COOKIE)
            .and_then(|c| self.sessions.get(c.value()))?;
        let user = self.users.find_by_id(session.user_id)?;
        Some(CommitIdentity {
            author_name: user.git_name,
            author_email: user.git_email,
            committer_name: self.committer_name.to_string(),
            committer_email: self.committer_email.to_string(),
        })
    }
}
```

> **CodeScene / clippy note:** `#[allow(...)]` is banned by AGENTS.md. Do **not** ship the `too_many_arguments` allow. Instead introduce a small `AppStateConfig { committer_name, committer_email, autogit, cookie_secure }` struct passed to `new`, or a builder, so the signature stays within clippy limits. Decide during Green→Refactor; the test above only depends on `new(...)` and `acting_user`, so refactor the constructor shape freely as long as the call sites and tests compile.

- [ ] **Step 4: Fix every `AppState::new` call site**

Update `config.rs` to read committer identity + autogit from env, and thread them through `lib.rs`. Add to `config.rs`:

```rust
/// Fixed git committer identity for server-authored commits, from
/// `TOLARIA_COMMITTER_NAME` / `TOLARIA_COMMITTER_EMAIL` (with sane defaults).
pub fn committer_identity() -> (String, String) {
    let name = std::env::var("TOLARIA_COMMITTER_NAME")
        .unwrap_or_else(|_| "Tolaria Server".to_string());
    let email = std::env::var("TOLARIA_COMMITTER_EMAIL")
        .unwrap_or_else(|_| "server@tolaria.local".to_string());
    (name, email)
}

/// Whether to auto-commit each write as the acting user. Default on; set
/// `TOLARIA_AUTOGIT=false` to disable.
pub fn autogit_enabled() -> bool {
    !matches!(
        std::env::var("TOLARIA_AUTOGIT").ok().as_deref(),
        Some("false") | Some("0")
    )
}
```

In `lib.rs`, pass them into `AppState::new` (or `AppStateConfig`). In every **test** helper that builds `AppState::new(...)` (`auth_middleware.rs`, `write_handlers.rs`, `handlers.rs` if present, `rpc.rs`), add the three new arguments: `"Tolaria Server".to_string(), "server@tolaria.local".to_string(), true` (or `false` for autogit where a test wants writes without commits).

- [ ] **Step 5: Run the whole server crate to verify it compiles and passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tolaria-server 2>&1 | tail -25`
Expected: PASS — all existing tests plus the two new `acting_user` tests. Then `cargo clippy --manifest-path src-tauri/Cargo.toml -p tolaria-server 2>&1 | grep -cE 'warning|error'` → `0`.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/crates/tolaria-server/src/rpc.rs src-tauri/crates/tolaria-server/src/config.rs src-tauri/crates/tolaria-server/src/lib.rs src-tauri/crates/tolaria-server/src/auth_middleware.rs src-tauri/crates/tolaria-server/src/write_handlers.rs
git commit -m "feat(server): repo lock + acting-user commit identity in AppState"
```

---

### Task 4: Server — read-only git commands (status + author identity)

Wire the non-mutating git reads into `handlers::dispatch` so the git-controls panel can show remote status and the author identity. These need no repo lock (read-only) but `git_author_identity` should reflect the acting user, so it is resolved in `command_route`, not `handlers::dispatch`.

**Files:**
- Modify: `src-tauri/crates/tolaria-server/src/handlers.rs` (`git_remote_status` arm)
- Modify: `src-tauri/crates/tolaria-server/src/rpc.rs` (`git_author_identity` special-case in `command_route`, since it needs the jar)

**Interfaces:**
- Consumes: `tolaria_core::git::{git_remote_status, GitRemoteStatus}`; `AppState::acting_user`.
- Produces: `git_remote_status` → `GitRemoteStatus` JSON (`{ branch, ahead, behind, hasRemote }`); `git_author_identity` → `{ name, email, source: "web-session", warning: null }`.

- [ ] **Step 1: Write the failing test (remote status arm)**

Add to the `tests` module in `handlers.rs` (mirroring the existing vault-registry tests — set up a temp git repo). If `handlers.rs` lacks a git-repo test helper, use `std::process::Command` to `git init` a `tempfile::tempdir`:

```rust
#[test]
fn git_remote_status_reports_no_remote_for_fresh_repo() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path();
    for args in [["init"].as_slice(), ["config", "user.email", "t@t"].as_slice(), ["config", "user.name", "T"].as_slice()] {
        std::process::Command::new("git").args(args).current_dir(vault).output().unwrap();
    }
    let out = dispatch(vault, "git_remote_status", serde_json::json!({ "vaultPath": vault.to_string_lossy() })).unwrap();
    assert_eq!(out["hasRemote"], serde_json::json!(false));
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tolaria-server git_remote_status_reports_no_remote 2>&1 | tail -15`
Expected: FAIL — dispatch returns `unsupported` (`git_remote_status` not a known command).

- [ ] **Step 3: Implement the `git_remote_status` arm**

In `handlers::dispatch`, add an arm (the read git command takes the server's `vault_root`, ignoring any client `vaultPath` for containment — always operate on `vault_root`):

```rust
"git_remote_status" => {
    let status = tolaria_core::git::git_remote_status(vault_root)
        .map_err(RpcError::internal)?;
    Ok(serde_json::to_value(status).map_err(|e| RpcError::internal(e.to_string()))?)
}
```

- [ ] **Step 4: Implement `git_author_identity` in `command_route`**

In `rpc.rs::command_route`, before the write/read dispatch, special-case the author-identity read so it reflects the logged-in user:

```rust
if command == "git_author_identity" {
    return match state.acting_user(&jar) {
        Some(id) => ok_json(json!({
            "name": id.author_name,
            "email": id.author_email,
            "source": "web-session",
            "warning": Value::Null,
        })),
        None => (StatusCode::UNAUTHORIZED, axum::Json(json!({ "error": "not authenticated" }))).into_response(),
    };
}
```

- [ ] **Step 5: Run tests + clippy**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tolaria-server git_remote_status 2>&1 | tail -15` → PASS.
Run: `cargo clippy --manifest-path src-tauri/Cargo.toml -p tolaria-server 2>&1 | grep -cE 'warning|error'` → `0`.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/crates/tolaria-server/src/handlers.rs src-tauri/crates/tolaria-server/src/rpc.rs
git commit -m "feat(server): git_remote_status + git_author_identity reads"
```

---

### Task 5: Server — mutating git commands behind the repo lock

Add a git-command dispatcher for the index-mutating operations: explicit commit, push, pull, resolve-conflict, and commit-conflict-resolution. All run under `state.repo_lock`. `command_route` routes these, resolving the acting user for authorship.

**Files:**
- Create: `src-tauri/crates/tolaria-server/src/git_handlers.rs`
- Modify: `src-tauri/crates/tolaria-server/src/lib.rs` (add `mod git_handlers;`)
- Modify: `src-tauri/crates/tolaria-server/src/rpc.rs` (route git commands, pass acting user)

**Interfaces:**
- Consumes: `tolaria_core::git::{git_commit_all_as, git_push, git_pull, git_resolve_conflict, git_commit_conflict_resolution, CommitIdentity}`; `AppState`.
- Produces:
  - `pub fn is_git_command(command: &str) -> bool` — true for `git_commit`, `git_push`, `git_pull`, `git_resolve_conflict`, `git_commit_conflict_resolution`.
  - `pub async fn dispatch_git(state: &AppState, identity: &CommitIdentity, command: &str, args: Value) -> Result<Value, RpcError>` — acquires `state.repo_lock` and dispatches. `git_commit` uses `git_commit_all_as(vault_root, message, identity)`; `git_push`/`git_pull` shell out (ambient credential); conflict-resolution reuses core.

- [ ] **Step 1: Write the failing test — explicit commit is authored by the acting user**

Create `src-tauri/crates/tolaria-server/src/git_handlers.rs` with a `tests` module:

```rust
//! Repo-locked dispatch for index-mutating git commands (commit/push/pull/
//! conflict resolution). Serialized by `AppState::repo_lock` so git's index is
//! never mutated concurrently. Commits are authored by the acting user.

use crate::rpc::{ok_value, AppState, RpcError};
use serde::Deserialize;
use serde_json::{json, Value};
use tolaria_core::git::{
    git_commit_all_as, git_commit_conflict_resolution, git_pull, git_push, git_resolve_conflict,
    CommitIdentity,
};

pub fn is_git_command(command: &str) -> bool {
    matches!(
        command,
        "git_commit"
            | "git_push"
            | "git_pull"
            | "git_resolve_conflict"
            | "git_commit_conflict_resolution"
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommitArgs {
    message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResolveArgs {
    file: String,
    strategy: String,
}

pub async fn dispatch_git(
    state: &AppState,
    identity: &CommitIdentity,
    command: &str,
    args: Value,
) -> Result<Value, RpcError> {
    let _repo = state.repo_lock.lock().await;
    let vault = state.vault_root.to_string_lossy().to_string();
    match command {
        "git_commit" => {
            let a: CommitArgs = serde_json::from_value(args)
                .map_err(|e| RpcError::bad_request(format!("bad arguments: {e}")))?;
            let hash = git_commit_all_as(&vault, &a.message, identity).map_err(RpcError::internal)?;
            Ok(json!({ "hash": hash }))
        }
        "git_push" => {
            let r = git_push(&vault).map_err(RpcError::internal)?;
            Ok(serde_json::to_value(r).map_err(|e| RpcError::internal(e.to_string()))?)
        }
        "git_pull" => {
            let r = git_pull(&vault).map_err(RpcError::internal)?;
            Ok(serde_json::to_value(r).map_err(|e| RpcError::internal(e.to_string()))?)
        }
        "git_resolve_conflict" => {
            let a: ResolveArgs = serde_json::from_value(args)
                .map_err(|e| RpcError::bad_request(format!("bad arguments: {e}")))?;
            git_resolve_conflict(&vault, &a.file, &a.strategy).map_err(RpcError::internal)?;
            Ok(Value::Null)
        }
        "git_commit_conflict_resolution" => {
            let hash = git_commit_conflict_resolution(&vault).map_err(RpcError::internal)?;
            Ok(json!({ "hash": hash }))
        }
        other => Err(crate::rpc::unsupported(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locks::PathLocks;
    use crate::session::SessionStore;
    use crate::users::UsersDb;
    use std::time::Duration;

    fn repo_state(vault: &std::path::Path) -> AppState {
        for args in [
            ["init"].as_slice(),
            ["config", "user.email", "seed@t"].as_slice(),
            ["config", "user.name", "Seed"].as_slice(),
        ] {
            std::process::Command::new("git").args(args).current_dir(vault).output().unwrap();
        }
        AppState::new(
            vault.to_path_buf(),
            UsersDb::open_in_memory().unwrap(),
            SessionStore::new(Duration::from_secs(60)),
            false,
            PathLocks::new(),
            "Tolaria Server".to_string(),
            "server@tolaria.local".to_string(),
            true,
        )
    }

    fn eve() -> CommitIdentity {
        CommitIdentity {
            author_name: "Eve E".into(),
            author_email: "eve@example.com".into(),
            committer_name: "Tolaria Server".into(),
            committer_email: "server@tolaria.local".into(),
        }
    }

    #[tokio::test]
    async fn git_commit_is_authored_by_acting_user() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path();
        let state = repo_state(vault);
        std::fs::write(vault.join("note.md"), "# Note\n").unwrap();

        let out = dispatch_git(&state, &eve(), "git_commit", json!({ "message": "add note" }))
            .await
            .unwrap();
        assert!(out["hash"].as_str().is_some());

        let author = std::process::Command::new("git")
            .args(["log", "-1", "--format=%an <%ae>"])
            .current_dir(vault)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&author.stdout).trim(), "Eve E <eve@example.com>");
    }
}
```

> `ok_value` is a plain `Result`-friendly helper; if `rpc.rs` has no such export, just return the `Value` directly (as shown) — remove the unused `ok_value` import.

- [ ] **Step 2: Register the module + route it**

Add `mod git_handlers;` to `lib.rs`. In `rpc.rs::command_route`, after the CSRF check and the `git_author_identity` special-case, add:

```rust
if crate::git_handlers::is_git_command(&command) {
    let Some(identity) = state.acting_user(&jar) else {
        return (StatusCode::UNAUTHORIZED, axum::Json(json!({ "error": "not authenticated" }))).into_response();
    };
    return match crate::git_handlers::dispatch_git(&state, &identity, &command, args.clone()).await {
        Ok(value) => ok_json(value),
        Err(err) => err.into_response(),
    };
}
```

> Ensure `args` is cloned/positioned so the existing write/read dispatch below still receives it. Restructure `command_route` so the args value is read once and reused.

- [ ] **Step 3: Run to verify the new test fails, then passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tolaria-server git_handlers 2>&1 | tail -20`
Expected after implementation: PASS.

- [ ] **Step 4: clippy + CodeScene on the new file**

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml -p tolaria-server 2>&1 | grep -cE 'warning|error'` → `0`.
Run CodeScene file-level review on `git_handlers.rs` → must be `10.0` (new scorable file) or zero findings.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/crates/tolaria-server/src/git_handlers.rs src-tauri/crates/tolaria-server/src/lib.rs src-tauri/crates/tolaria-server/src/rpc.rs
git commit -m "feat(server): repo-locked git commit/push/pull/conflict dispatch"
```

---

### Task 6: Server — autogit commit after each successful write

After each successful write (`save_note_content`, `create_note*`, `rename_note*`, `delete_note`, frontmatter updates), commit just the affected path(s) as the acting user — when `state.autogit` is enabled. The write already holds and releases the per-path lock; the commit then takes the repo lock (correct ordering per Global Constraints).

**Files:**
- Modify: `src-tauri/crates/tolaria-server/src/write_handlers.rs`
- Modify: `src-tauri/crates/tolaria-server/src/rpc.rs` (pass acting user into `dispatch_write`)

**Interfaces:**
- Consumes: `tolaria_core::git::git_commit_paths_as`, `AppState::{repo_lock, autogit, vault_root}`, `CommitIdentity`.
- Produces: `dispatch_write` gains an `identity: Option<&CommitIdentity>` parameter; a helper `async fn autogit_commit(state, identity, rel_paths: &[String], message: &str)` that no-ops when `identity` is `None` or `state.autogit` is false, else takes the repo lock and calls `git_commit_paths_as`. Autogit failures are logged (`eprintln!`) but **do not** fail the write (the file is already safely on disk).

- [ ] **Step 1: Write the failing test — a save auto-commits the saved file as the user**

Add to the `tests` module in `write_handlers.rs` (build the state over a git-init'd tempdir; mirror the `git_handlers` `repo_state` helper — extract it to a shared test util if convenient):

```rust
#[tokio::test]
async fn save_auto_commits_the_file_as_acting_user() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path();
    for args in [["init"].as_slice(), ["config", "user.email", "s@t"].as_slice(), ["config", "user.name", "S"].as_slice()] {
        std::process::Command::new("git").args(args).current_dir(vault).output().unwrap();
    }
    let state = crate::rpc::AppState::new(
        vault.to_path_buf(),
        crate::users::UsersDb::open_in_memory().unwrap(),
        crate::session::SessionStore::new(std::time::Duration::from_secs(60)),
        false,
        crate::locks::PathLocks::new(),
        "Tolaria Server".to_string(),
        "server@tolaria.local".to_string(),
        true, // autogit on
    );
    let identity = tolaria_core::git::CommitIdentity {
        author_name: "Frank F".into(),
        author_email: "frank@example.com".into(),
        committer_name: "Tolaria Server".into(),
        committer_email: "server@tolaria.local".into(),
    };
    let p = vault.join("auto.md");
    dispatch_write(
        &state,
        Some(&identity),
        "save_note_content",
        json!({ "path": p.to_string_lossy(), "content": "# Auto\n" }),
    )
    .await
    .unwrap();

    let log = std::process::Command::new("git")
        .args(["log", "-1", "--format=%an <%ae>|%s"])
        .current_dir(vault)
        .output()
        .unwrap();
    let line = String::from_utf8_lossy(&log.stdout);
    assert!(line.starts_with("Frank F <frank@example.com>|"), "got: {line}");
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tolaria-server save_auto_commits 2>&1 | tail -20`
Expected: FAIL — `dispatch_write` arity mismatch (no `identity` param).

- [ ] **Step 3: Thread the identity + add `autogit_commit`**

Change `dispatch_write` signature to:

```rust
pub async fn dispatch_write(
    state: &AppState,
    identity: Option<&tolaria_core::git::CommitIdentity>,
    command: &str,
    args: Value,
) -> Result<Value, RpcError> {
```

After each successful mutation, compute the vault-relative path(s) affected and call the helper. For `save_note_content`, in the success tail after `vault::save_note_content(...)`:

```rust
autogit_commit(state, identity, &[rel_path(state, &safe)], &format!("update {}", file_label(&safe))).await;
```

Add helpers to `write_handlers.rs`:

```rust
/// The vault-relative path used for `git add`, falling back to the absolute
/// path string if it is not under the vault root (should not happen — writes
/// are containment-checked).
fn rel_path(state: &AppState, safe: &std::path::Path) -> String {
    safe.strip_prefix(state.vault_root.as_ref())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| safe.to_string_lossy().to_string())
}

fn file_label(safe: &std::path::Path) -> String {
    safe.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

/// Commit the given vault-relative paths as `identity`, when autogit is enabled.
/// A commit failure is logged but never fails the write — the file is already
/// safely persisted to disk. Runs under the repo lock.
async fn autogit_commit(
    state: &AppState,
    identity: Option<&tolaria_core::git::CommitIdentity>,
    rel_paths: &[String],
    message: &str,
) {
    let Some(identity) = identity else { return };
    if !state.autogit {
        return;
    }
    let _repo = state.repo_lock.lock().await;
    let vault = state.vault_root.to_string_lossy().to_string();
    if let Err(e) = tolaria_core::git::git_commit_paths_as(&vault, rel_paths, message, identity) {
        eprintln!("autogit commit failed for {rel_paths:?}: {e}");
    }
}
```

For `delete_note`, `rename_note*`, `update_frontmatter`, `delete_frontmatter_property`, add the same `autogit_commit(...)` call in each success tail. For rename, stage both old and new relative paths so the deletion + addition land in one commit. For delete, the path no longer exists on disk but `git add -- <rel>` still stages the removal.

- [ ] **Step 4: Update the two `command_route` write call sites**

In `rpc.rs`, the write branch becomes:

```rust
let identity = state.acting_user(&jar);
crate::write_handlers::dispatch_write(&state, identity.as_ref(), &command, args).await
```

Update the existing `write_handlers` tests that call `dispatch_write(&state, "cmd", ...)` to pass `None` for the identity argument (those tests assert file effects, not commits): `dispatch_write(&state, None, "delete_note", ...)`.

- [ ] **Step 5: Run to verify pass + full server suite green**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tolaria-server 2>&1 | tail -25` → PASS (including `save_auto_commits...`).
Run: `cargo clippy --manifest-path src-tauri/Cargo.toml -p tolaria-server 2>&1 | grep -cE 'warning|error'` → `0`.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/crates/tolaria-server/src/write_handlers.rs src-tauri/crates/tolaria-server/src/rpc.rs
git commit -m "feat(server): autogit per-save commit authored by acting user"
```

---

### Task 7: Frontend — route git commands to the server + surface git controls

Add the git commands to the web bridge's `SERVER_COMMANDS` so they hit the HTTP transport, and make sure the git-controls UI is enabled on web.

**Files:**
- Modify: `src/web/mockBridge.ts` (`SERVER_COMMANDS`)
- Modify: `src/web/transport.ts` (map `git_commit` args `{ message }`; nothing else needed — generic pass-through) — only if the current transport drops non-`path` args (verify; it already spreads `...args`).
- Test: `src/web/mockBridge.test.ts` (or the existing bridge test file)

**Interfaces:**
- Consumes: existing `invoke` transport, `SERVER_COMMANDS` set.
- Produces: `git_remote_status`, `git_author_identity`, `git_commit`, `git_push`, `git_pull`, `git_resolve_conflict`, `git_commit_conflict_resolution` added to `SERVER_COMMANDS`.

- [ ] **Step 1: Write the failing test**

In the bridge test file, assert the git commands route to the HTTP transport (mock `fetch`, call `mockInvoke('git_remote_status', ...)`, expect a `POST /api/cmd/git_remote_status`). If the existing test file stubs the transport, assert the command is a member of the exported `SERVER_COMMANDS`:

```ts
import { SERVER_COMMANDS } from './mockBridge'

it('routes git sync commands to the server', () => {
  for (const cmd of [
    'git_remote_status', 'git_author_identity', 'git_commit',
    'git_push', 'git_pull', 'git_resolve_conflict', 'git_commit_conflict_resolution',
  ]) {
    expect(SERVER_COMMANDS.has(cmd)).toBe(true)
  }
})
```

> If `SERVER_COMMANDS` is not currently exported, export it (`export const SERVER_COMMANDS = ...`).

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm vitest run src/web/mockBridge.test.ts 2>&1 | tail -20`
Expected: FAIL — git commands not in `SERVER_COMMANDS`.

- [ ] **Step 3: Add the commands**

In `src/web/mockBridge.ts`, add the seven git command strings to the `SERVER_COMMANDS` set (keep alphabetical grouping with a `// git sync (Phase 5)` comment).

- [ ] **Step 4: Verify git controls surface on web**

The git-controls UI is gated by app settings (`settings.git_enabled` / `gitFeaturesEnabled`, `src/App.tsx`). Confirm which command supplies those settings on web (likely `get_settings` / vault config, currently served by the mock). If the mock returns `git_enabled: null`, set the web mock's settings so `git_enabled: true` (edit `src/web/mockBridge.ts` mock defaults, NOT the desktop mock), so the commit/push/pull controls render. Add a focused test asserting the web settings expose `git_enabled: true`.

- [ ] **Step 5: Run frontend checks**

Run: `pnpm vitest run src/web/ 2>&1 | tail -20` → PASS.
Run: `npx tsc --noEmit 2>&1 | tail -5` → no errors.

- [ ] **Step 6: Commit**

```bash
git add src/web/mockBridge.ts src/web/mockBridge.test.ts
git commit -m "feat(web): route git sync commands to the server + enable git controls"
```

---

### Task 8: Deployment, docs, and end-to-end QA

Supply the push credential to the container, document the git-sync model, and QA the full commit→push→pull loop.

**Files:**
- Modify: `docker-compose.yml` (mount an SSH deploy key or set a credential-helper env; add `TOLARIA_COMMITTER_NAME/EMAIL`, `TOLARIA_AUTOGIT`)
- Modify: `nginx.prod.conf` only if the git controls need larger request bodies (unlikely) — otherwise leave.
- Create: `docs/adr/0151-web-server-git-sync-model.md`
- Modify: `docs/ARCHITECTURE.md` (Git sync subsection under the Web Server section), `docs/adr/README.md` (index row)
- Create: `tests/smoke/web-git-sync.spec.ts` (only if the smoke lane can run against the server build; tag `@smoke` since git commit/push is a core workflow)

**Interfaces:** none (integration + docs).

- [ ] **Step 1: Deployment wiring**

In `docker-compose.yml`, add to the `tolaria-web` service `environment`: `TOLARIA_COMMITTER_NAME`, `TOLARIA_COMMITTER_EMAIL`, `TOLARIA_AUTOGIT`. For push, mount a read-only deploy key (`./secrets/deploy_key:/home/app/.ssh/id_ed25519:ro`) and set `GIT_SSH_COMMAND=ssh -i /home/app/.ssh/id_ed25519 -o StrictHostKeyChecking=accept-new`, **or** use an HTTPS remote with a credential helper reading a mounted token. Document that the remote must already be configured on the mounted vault clone (`git remote add origin ...` at provisioning). Do not commit any real key/token — add `.gitignore` entries for `secrets/`.

- [ ] **Step 2: Write ADR 0151**

Create `docs/adr/0151-web-server-git-sync-model.md` (frontmatter `type: ADR`, `id: "0151"`, `status: active`, `date: 2026-07-04`). Document: per-save path-scoped commit authored by the acting user (committer = fixed server identity); explicit commit/push/pull/status/conflict controls behind a single repo-level `tokio::Mutex`; lock ordering (path lock → repo lock); autogit toggle (`TOLARIA_AUTOGIT`); single server-side push credential at deploy time; `git add -A` used only for the explicit commit control, never for autogit; real-time WS events explicitly deferred. Note the known trade-off: the explicit "commit" control (`git add -A`) can sweep concurrent working-tree edits into one commit — acceptable as a deliberate user action, matching desktop.

- [ ] **Step 3: Update ARCHITECTURE.md + ADR README**

Add a "### Git sync (Phase 5)" subsection under the Web Server section describing the command set, repo lock, authorship, and autogit. Add the `0151` row to `docs/adr/README.md`.

- [ ] **Step 4: Full gate run**

Run:
```bash
cargo test --manifest-path src-tauri/Cargo.toml 2>&1 | tail -15
cargo llvm-cov --manifest-path src-tauri/Cargo.toml --no-clean --fail-under-lines 85 2>&1 | tail -5
pnpm lint && npx tsc --noEmit && pnpm test 2>&1 | tail -15
```
Expected: all green; Rust coverage ≥ 85%.

- [ ] **Step 5: Native/browser QA**

Build the web image, log in, create+edit a note (verify a commit appears authored by the user via `git log` in the vault), then use the git controls to push and pull. Confirm remote status (ahead/behind) updates. Capture the result for the completion notes. Fix whatever gates the git panel if it does not render.

- [ ] **Step 6: Commit + push**

```bash
git add docker-compose.yml docs/adr/0151-web-server-git-sync-model.md docs/ARCHITECTURE.md docs/adr/README.md tests/smoke/web-git-sync.spec.ts .gitignore
git commit -m "feat(docker,docs): web git-sync deployment wiring + ADR 0151"
```

---

## Self-Review

**Spec coverage (§8 git sync model, §5.3 command set, §13 item 5):**
- Per-save commit authored by acting user → Tasks 1, 6. ✅
- Committer may be fixed server identity → Task 3 (`committer_*`), Task 1 (`CommitIdentity`). ✅
- Per-path locks for different files (existing) + repo-level lock for git index → Task 3, enforced in Tasks 5 & 6. ✅
- Commit / push / pull / status controls → Tasks 4 (status) + 5 (commit/push/pull). ✅
- Conflict resolution via existing resolver → Task 5 (`git_resolve_conflict`, `git_commit_conflict_resolution`). ✅
- Single server-side push credential at deploy → Task 8. ✅
- Per-user attribution preserved through authorship → Tasks 1, 3, 6. ✅
- WS real-time events → explicitly deferred (locked decision), noted in ADR (Task 8). ✅

**Placeholder scan:** every code step contains concrete code; no "add error handling"/"TBD". The one `#[allow(clippy::too_many_arguments)]` shown in Task 3 is explicitly flagged as **must-remove** with the refactor direction (config struct/builder) — not shipped.

**Type consistency:** `CommitIdentity { author_name, author_email, committer_name, committer_email }` is defined in Task 1 and used identically in Tasks 3, 5, 6. `git_commit_paths_as` / `git_commit_all_as` signatures match between definition (Task 1) and calls (Tasks 5, 6). `dispatch_write` gains `identity: Option<&CommitIdentity>` in Task 6 and every call site (Task 6 Step 4) is updated. `find_by_id` (Task 2) is consumed by `acting_user` (Task 3).

**Open verification the implementer must do (not gaps, but environment-dependent):**
- Confirm `UsersDb`'s connection field/locking shape for `find_by_id` (Task 2 note).
- Confirm the exact settings command that gates `git_enabled` on web (Task 7 Step 4).
- Confirm the smoke lane can exercise the server build; if not, keep git-sync QA as native/manual (Task 8 Step 5) and skip the `@smoke` spec.
