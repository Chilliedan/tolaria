# Web Git & Vault Command Surface Completion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Route the remaining git read/status, git remote-management, and vault-folder commands the web UI uses to the real server (they currently fall back to the in-browser mock and show fake data), and explicitly no-op the genuinely desktop-only commands so they stop 501-ing.

**Architecture:** The web build sends every `mockInvoke(cmd)` either to the real HTTP transport (if `cmd ∈ SERVER_COMMANDS`) or to the in-browser mock. Phase 5 routed the git *actions* but not the git *reads* or vault-folder ops, so the Changes/diff/history/pulse panels and Add Remote render mock data. This plan implements the missing commands in `tolaria-server` (reusing the already-`pub` `tolaria-core::git`/`vault` functions), adds them to `SERVER_COMMANDS`, and adds a `DESKTOP_ONLY` no-op set for commands with no web meaning. Read commands go through the synchronous `handlers::dispatch`; repo/working-tree mutations go behind the repo lock; folder mutations go through `write_handlers` with autogit.

**Tech Stack:** Rust, Axum 0.7, `tolaria-core`, React + TypeScript, Vitest, `cargo test` / `cargo clippy`.

## Global Constraints

- **Arg casing (the recurring trap):** the web client routes these commands through the mock path, which sends **snake_case** field names (`file_path`, `folder_path`, `remote_url`, `new_name`), while desktop Tauri sends **camelCase**. Every multi-word arg field the server deserializes MUST accept **both** (serde `#[serde(rename_all = "camelCase")]` + `#[serde(alias = "snake_case_name")]`, or manual extraction that tries both). Single-word fields (`path`, `limit`) need no alias. Tests MUST send the snake_case web form.
- **Client `vaultPath` is advisory** — every server git/vault op operates on the server's `state.vault_root`, never a client-supplied path. Per-file args are used only as repo-relative pathspecs.
- **Reuse `tolaria-core`** — do not reimplement git/vault logic in the server; these functions already exist and are `pub`. The server is a thin dispatcher.
- **Reads vs mutations:** read commands (status/diff/history/pulse/is-git) go in `handlers::dispatch` (sync, no lock). Repo/working-tree mutations (`git_add_remote`, `git_discard_file`, `init_git_repo`) run behind `state.repo_lock`. Folder mutations (`create/delete/rename_vault_folder`) run through `write_handlers` with a path lock + autogit commit.
- **NEVER** `#[allow(...)]`, `// eslint-disable`, `as any`, or lint suppression.
- **TDD**, one Red→Green→Refactor cycle per commit. Rust tests + `cargo clippy` clean; frontend `vitest` + `tsc --noEmit` + `eslint` clean.
- **Rust line coverage ≥ 85%**, frontend ≥ 70% (run on the pre-push sidecar; locally run the targeted suites named per task).

---

## Command inventory (what this plan routes)

**Route to server — git reads (Task 2, `handlers::dispatch`, no lock):**

| Command | Core fn (`tolaria_core::git`) | Args (server uses) | Returns |
|---|---|---|---|
| `is_git_repo` | *(new: check `vault_root/.git`)* | — | `bool` |
| `get_modified_files` | `get_modified_files(vault_root)` | — | `Vec<ModifiedFile>` |
| `get_modified_files_with_stats` | `get_modified_files_with_stats(vault_root)` | — | `Vec<ModifiedFile>` |
| `get_file_diff` | `get_file_diff(vault_root, file_path)` | `file_path` | `String` |
| `get_file_diff_at_commit` | `get_file_diff_at_commit(vault_root, file_path, commit)` | `file_path`, `commit` | `String` |
| `get_file_history` | `get_file_history(vault_root, file_path)` | `file_path` | `Vec<GitCommit>` |
| `get_last_commit_info` | `get_last_commit_info(vault_root)` | — | `Option<LastCommitInfo>` |
| `get_vault_pulse` | `get_vault_pulse(vault_root, limit)` | `limit` | pulse struct |
| `git_file_url` | `git_file_url(vault_root, file_path)` | `file_path` | `Option<String>` |

**Route to server — repo/working-tree mutations (Task 3, `git_handlers::dispatch_git`, repo lock):**

| Command | Core fn | Args | Returns |
|---|---|---|---|
| `git_add_remote` | `git_add_remote(vault_root, remote_url)` | `remote_url` | `GitAddRemoteResult` |
| `git_discard_file` | `discard_file_changes(vault_root, relative_path)` | `relative_path` (alias `file_path`) | `null` |
| `init_git_repo` | `init_repo(vault_root)` | — | `null` |

**Route to server — folder mutations (Task 4, `write_handlers`, path lock + autogit):**

| Command | Core fn (`tolaria_core::vault`) | Args | Returns |
|---|---|---|---|
| `create_vault_folder` | `create_folder(vault_root, folder_path)` | `folder_path` | created path `String` |
| `delete_vault_folder` | `delete_folder(vault_root, folder_path)` | `folder_path` | deleted path `String` |
| `rename_vault_folder` | `rename_folder(vault_root, folder_path, new_name)` | `folder_path`, `new_name` | result |

**Desktop-only — no-op on web (Task 6, `DESKTOP_ONLY` set):** `copy_text_to_clipboard`, `read_text_from_clipboard`, `copy_image_to_vault`, `save_image`, `export_current_webview_pdf`, `print_current_webview`, `can_export_current_webview_pdf`, `update_current_window_min_size`, `perform_current_window_titlebar_double_click`, `trigger_menu_command`, `update_menu_state`, `check_for_app_update`, `download_and_install_app_update`, `start_vault_watcher`, `stop_vault_watcher`, `get_process_memory_snapshot`, `update_app_icon`, `open_vault_file_external`, `should_use_external_media_preview`, `sync_vault_asset_scope_for_window`, `get_agent_docs_path`, `check_claude_cli`, `get_ai_workspace_sessions`, `save_ai_workspace_sessions`, `save_ai_model_provider_api_key`, `delete_ai_model_provider_api_key`, `test_ai_model_provider`, `stream_ai_agent`, `stream_ai_model`, `stream_claude_chat`.

> **Out of scope (leave on the mock as-is):** `auto_rename_untitled`, `detect_renames`, `update_wikilinks_for_renames`, `batch_delete_notes_async` (desktop batch path), `validate_note_content` — these are either desktop-internal or already handled elsewhere; routing them is a separate concern. Note them in the ADR as knowingly deferred.

---

### Task 1: Server — a shared arg extractor that accepts camelCase + snake_case

Add a tiny helper so read handlers can pull a string/number arg regardless of casing, without repeating `.get("x").or(.get("y"))` everywhere.

**Files:**
- Modify: `src-tauri/crates/tolaria-server/src/handlers.rs`

**Interfaces:**
- Produces:
  - `fn arg_str(args: &Value, camel: &str, snake: &str) -> Result<String, RpcError>` — returns the string at `camel` or `snake`, else `RpcError::bad_request`.
  - `fn arg_opt_str(args: &Value, camel: &str, snake: &str) -> Option<String>` — optional variant.
  - `fn arg_u64(args: &Value, key: &str, default: u64) -> u64` — number with default.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `handlers.rs`:

```rust
#[test]
fn arg_str_accepts_camel_and_snake() {
    let camel = serde_json::json!({ "filePath": "a.md" });
    let snake = serde_json::json!({ "file_path": "b.md" });
    assert_eq!(arg_str(&camel, "filePath", "file_path").unwrap(), "a.md");
    assert_eq!(arg_str(&snake, "filePath", "file_path").unwrap(), "b.md");
    assert!(arg_str(&serde_json::json!({}), "filePath", "file_path").is_err());
    assert_eq!(arg_u64(&serde_json::json!({ "limit": 5 }), "limit", 20), 5);
    assert_eq!(arg_u64(&serde_json::json!({}), "limit", 20), 20);
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tolaria-server arg_str_accepts 2>&1 | tail -12`
Expected: FAIL — `cannot find function 'arg_str'`.

- [ ] **Step 3: Implement the helpers**

Add to `handlers.rs` (module scope):

```rust
/// Read a required string arg by its camelCase or snake_case key. The web mock
/// path sends snake_case; desktop Tauri sends camelCase — accept both.
fn arg_str(args: &Value, camel: &str, snake: &str) -> Result<String, RpcError> {
    args.get(camel)
        .or_else(|| args.get(snake))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| RpcError::bad_request(format!("missing string arg '{camel}'/'{snake}'")))
}

/// Optional string arg by camelCase or snake_case key.
fn arg_opt_str(args: &Value, camel: &str, snake: &str) -> Option<String> {
    args.get(camel)
        .or_else(|| args.get(snake))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Numeric arg with a default when absent or non-numeric.
fn arg_u64(args: &Value, key: &str, default: u64) -> u64 {
    args.get(key).and_then(|v| v.as_u64()).unwrap_or(default)
}
```

> If `arg_opt_str` is unused after Task 2, delete it (YAGNI) — do not leave it to trip clippy's dead-code lint.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tolaria-server arg_str_accepts 2>&1 | tail -8` → PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/crates/tolaria-server/src/handlers.rs
git commit -m "feat(server): casing-tolerant arg extractors for web dispatch"
```

---

### Task 2: Server — git read commands in `handlers::dispatch`

Wire the nine read commands. These are pure reads: no lock, always operate on `state.vault_root`.

**Files:**
- Modify: `src-tauri/crates/tolaria-server/src/handlers.rs`

**Interfaces:**
- Consumes: `arg_str`/`arg_u64` (Task 1); `tolaria_core::git::{get_modified_files, get_modified_files_with_stats, get_file_diff, get_file_diff_at_commit, get_file_history, get_last_commit_info, get_vault_pulse, git_file_url}`.
- Produces: new `dispatch` arms for the nine commands (see inventory table), each returning the serialized core result.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `handlers.rs` (reuse the `git init` temp-repo pattern already used by the `git_remote_status` test — commit one file so history/modified queries have data):

```rust
#[test]
fn git_read_commands_dispatch_against_the_vault() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path();
    for a in [["init"].as_slice(), ["config","user.email","t@t"].as_slice(), ["config","user.name","T"].as_slice()] {
        std::process::Command::new("git").args(a).current_dir(vault).output().unwrap();
    }
    std::fs::write(vault.join("note.md"), "# Note\n").unwrap();
    std::process::Command::new("git").args(["add","-A"]).current_dir(vault).output().unwrap();
    std::process::Command::new("git").args(["commit","-m","init"]).current_dir(vault).output().unwrap();

    // is_git_repo → true for the vault
    assert_eq!(dispatch(vault, "is_git_repo", serde_json::json!({})).unwrap(), serde_json::json!(true));
    // get_modified_files → array (empty after commit)
    assert!(dispatch(vault, "get_modified_files", serde_json::json!({})).unwrap().is_array());
    // get_file_history for note.md (snake_case arg, the web form) → non-empty array
    let hist = dispatch(vault, "get_file_history", serde_json::json!({ "file_path": "note.md" })).unwrap();
    assert!(hist.as_array().map(|a| !a.is_empty()).unwrap_or(false));
    // get_vault_pulse honours limit
    assert!(dispatch(vault, "get_vault_pulse", serde_json::json!({ "limit": 5 })).unwrap().is_object()
        || dispatch(vault, "get_vault_pulse", serde_json::json!({ "limit": 5 })).unwrap().is_array());
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tolaria-server git_read_commands_dispatch 2>&1 | tail -12`
Expected: FAIL — `dispatch` returns `unsupported` for `is_git_repo`.

- [ ] **Step 3: Implement the arms**

In `handlers::dispatch`, add (helper `serialize` avoids repeating the map_err):

```rust
"is_git_repo" => Ok(Value::Bool(vault_root.join(".git").is_dir())),
"get_modified_files" => serialize(tolaria_core::git::get_modified_files(vault_root)),
"get_modified_files_with_stats" => serialize(tolaria_core::git::get_modified_files_with_stats(vault_root)),
"get_last_commit_info" => serialize(tolaria_core::git::get_last_commit_info(vault_root)),
"get_vault_pulse" => serialize(tolaria_core::git::get_vault_pulse(vault_root, arg_u64(&args, "limit", 30) as usize)),
"get_file_diff" => {
    let f = arg_str(&args, "filePath", "file_path")?;
    serialize(tolaria_core::git::get_file_diff(&vault_root.to_string_lossy(), &f))
}
"get_file_diff_at_commit" => {
    let f = arg_str(&args, "filePath", "file_path")?;
    let c = arg_str(&args, "commit", "commit")?;
    serialize(tolaria_core::git::get_file_diff_at_commit(&vault_root.to_string_lossy(), &f, &c))
}
"get_file_history" => {
    let f = arg_str(&args, "filePath", "file_path")?;
    serialize(tolaria_core::git::get_file_history(&vault_root.to_string_lossy(), &f))
}
"git_file_url" => {
    let f = arg_str(&args, "filePath", "file_path")?;
    serialize(tolaria_core::git::git_file_url(&vault_root.to_string_lossy(), &f))
}
```

Add the `serialize` helper near `arg_str`:

```rust
/// Map a `Result<T: Serialize, String>` core result into an RPC JSON result:
/// core error → 500, serialization error → 500.
fn serialize<T: serde::Serialize>(result: Result<T, String>) -> Result<Value, RpcError> {
    let value = result.map_err(RpcError::internal)?;
    serde_json::to_value(value).map_err(|e| RpcError::internal(e.to_string()))
}
```

> Confirm the exact signatures of `get_file_diff_at_commit` and `get_vault_pulse` (arg order, whether `commit` is `&str`) and the `get_vault_pulse` return type when writing this — adjust the arm to match. If `get_file_diff_at_commit` names the commit arg differently in the frontend (e.g. `commitHash`), add that alias to the `arg_str` call.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tolaria-server git_read_commands_dispatch 2>&1 | tail -10` → PASS.
Run: `cargo clippy --manifest-path src-tauri/Cargo.toml -p tolaria-server 2>&1 | grep -cE 'warning|error'` → `0`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/crates/tolaria-server/src/handlers.rs
git commit -m "feat(server): git read commands (status/diff/history/pulse/is-git)"
```

---

### Task 3: Server — repo-mutating git commands behind the repo lock

Add `git_add_remote`, `git_discard_file`, `init_git_repo` to the repo-locked dispatcher.

**Files:**
- Modify: `src-tauri/crates/tolaria-server/src/git_handlers.rs`

**Interfaces:**
- Consumes: `tolaria_core::git::{git_add_remote, discard_file_changes, init_repo}`.
- Produces: three new arms in `is_git_command` + `dispatch_git`.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `git_handlers.rs` (reuse `repo_state`/`eve` helpers):

```rust
#[tokio::test]
async fn git_add_remote_sets_the_remote() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path();
    let state = repo_state(vault); // git init'd
    let out = dispatch_git(&state, &eve(), "git_add_remote",
        json!({ "remote_url": "https://example.com/r.git" })).await.unwrap();
    // GitAddRemoteResult serializes to an object; the remote now exists.
    assert!(out.is_object());
    let remotes = std::process::Command::new("git").args(["remote","-v"])
        .current_dir(vault).output().unwrap();
    assert!(String::from_utf8_lossy(&remotes.stdout).contains("example.com/r.git"));
}

#[tokio::test]
async fn git_discard_file_missing_arg_is_bad_request() {
    let dir = tempfile::tempdir().unwrap();
    let state = repo_state(dir.path());
    let err = dispatch_git(&state, &eve(), "git_discard_file", json!({})).await.unwrap_err();
    assert_eq!(err.status, axum::http::StatusCode::BAD_REQUEST);
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tolaria-server git_add_remote_sets 2>&1 | tail -12`
Expected: FAIL — `git_add_remote` routed to `unsupported`.

- [ ] **Step 3: Implement**

Extend `is_git_command`:

```rust
matches!(command,
    "git_commit" | "git_push" | "git_pull" | "git_resolve_conflict"
        | "git_commit_conflict_resolution"
        | "git_add_remote" | "git_discard_file" | "init_git_repo")
```

Add arms to `dispatch_git` (after the existing ones; all run under the already-acquired `_repo` lock). Use a local casing-tolerant extractor (mirror Task 1, or `pub(crate)` the ones from `handlers.rs` and import them):

```rust
"git_add_remote" => {
    let url = str_arg(&args, "remoteUrl", "remote_url")?;
    let r = git_add_remote(&vault, &url).map_err(RpcError::internal)?;
    Ok(serde_json::to_value(r).map_err(|e| RpcError::internal(e.to_string()))?)
}
"git_discard_file" => {
    let rel = str_arg(&args, "relativePath", "relative_path")
        .or_else(|_| str_arg(&args, "filePath", "file_path"))?;
    discard_file_changes(&vault, &rel).map_err(RpcError::internal)?;
    Ok(Value::Null)
}
"init_git_repo" => {
    init_repo(&*state.vault_root).map_err(RpcError::internal)?;
    Ok(Value::Null)
}
```

Where `str_arg` is a small local helper in `git_handlers.rs` identical in behavior to `handlers::arg_str` (accept camel or snake, else `RpcError::bad_request`). Import `git_add_remote`, `discard_file_changes`, `init_repo` from `tolaria_core::git`.

> Verify `init_repo`'s param type (`impl AsRef<Path>`) — `&*state.vault_root` (a `&PathBuf`) coerces. Confirm `discard_file_changes` takes `(&str, &str)` and `git_add_remote` takes `(&str, &str)`; `vault` is `state.vault_root.to_string_lossy().to_string()` as in the existing arms.

- [ ] **Step 4: Route them in `command_route`**

No change needed if these are only reached via the git branch — but confirm `rpc::command_route`'s git branch calls `is_git_command` (updated) before the write/read branches, so the three new commands dispatch here. They require an authenticated acting user (already enforced by the git branch's 401).

- [ ] **Step 5: Run + clippy**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tolaria-server git_ 2>&1 | grep -E 'test result:' | tail -3` → all PASS.
Run: `cargo clippy --manifest-path src-tauri/Cargo.toml -p tolaria-server --all-targets 2>&1 | grep -cE 'warning|error'` → `0`.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/crates/tolaria-server/src/git_handlers.rs
git commit -m "feat(server): git_add_remote, git_discard_file, init_git_repo"
```

---

### Task 4: Server — vault folder commands with autogit

Add `create_vault_folder`, `delete_vault_folder`, `rename_vault_folder` to `write_handlers` (they mutate the vault → path lock + autogit, like note writes).

**Files:**
- Modify: `src-tauri/crates/tolaria-server/src/write_handlers.rs`

**Interfaces:**
- Consumes: `tolaria_core::vault::{create_folder, delete_folder, rename_folder}` (confirm exact names/signatures — `create_folder` may be named `create_vault_folder` in core; grep before implementing).
- Produces: `is_write_command` gains the three; `dispatch_write` gains three arms; each does `autogit_commit_all(state, identity, "<msg>")` after the mutation (folder ops touch an unknown set of paths → commit-all, like rename).

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `write_handlers.rs` (reuse `state_for`):

```rust
#[tokio::test]
async fn create_vault_folder_makes_the_dir_from_web_snake_args() {
    let dir = tempdir().unwrap();
    let state = state_for(dir.path());
    dispatch_write(&state, None, "create_vault_folder",
        json!({ "vault_path": dir.path(), "folder_path": "Projects" }))
        .await
        .expect("web create_vault_folder must succeed");
    assert!(dir.path().join("Projects").is_dir());
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tolaria-server create_vault_folder_makes 2>&1 | tail -12`
Expected: FAIL — routed to `unsupported`.

- [ ] **Step 3: Implement**

Extend `is_write_command` with the three commands. Add arms to `dispatch_write` — each parses a casing-tolerant `folder_path` (and `new_name` for rename), calls the core fn against `state.vault_root`, then `autogit_commit_all`. Example (create):

```rust
"create_vault_folder" => {
    let folder = str_arg(&args, "folderPath", "folder_path")?;
    // core create_folder confines under the vault; it returns the created path.
    let created = tolaria_core::vault::create_folder(&state.vault_root, &folder)
        .map_err(RpcError::internal)?;
    autogit_commit_all(state, identity, &format!("create folder {folder}")).await;
    Ok(json!(created))
}
```

`delete_vault_folder` → `delete_folder(&state.vault_root, &folder)`; `rename_vault_folder` → `rename_folder(&state.vault_root, &folder, &new_name)` with `new_name = str_arg(&args, "newName", "new_name")?`. Add a local `str_arg` helper (or reuse a shared one). `dispatch_write` already receives `identity` (Task 6 of Phase 5) — thread it into `autogit_commit_all`.

> Grep `tolaria-core/src/vault/folders.rs` for the exact fn names and signatures (`create_folder` vs `create_vault_folder`, whether they take `&Path` or `&str`, and the return type) and adjust. If a folder op internally rewrites wikilinks (like rename), commit-all is correct; if not, path-scoping would also work — commit-all is the safe default.

- [ ] **Step 4: Run + clippy**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tolaria-server vault_folder 2>&1 | grep -E 'test result:' | tail -2` → PASS.
Run: `cargo clippy --manifest-path src-tauri/Cargo.toml -p tolaria-server --all-targets 2>&1 | grep -cE 'warning|error'` → `0`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/crates/tolaria-server/src/write_handlers.rs
git commit -m "feat(server): vault folder create/delete/rename with autogit"
```

---

### Task 5: Frontend — route the new commands to the server

Add every command implemented in Tasks 2–4 to `SERVER_COMMANDS`.

**Files:**
- Modify: `src/web/mockBridge.ts`
- Test: `src/web/mockBridge.test.ts`

**Interfaces:**
- Consumes: `SERVER_COMMANDS` (exported in Phase 5).

- [ ] **Step 1: Write the failing test**

Add to `src/web/mockBridge.test.ts`:

```ts
it('routes git read + folder commands to the server', () => {
  for (const cmd of [
    'is_git_repo', 'get_modified_files', 'get_modified_files_with_stats',
    'get_file_diff', 'get_file_diff_at_commit', 'get_file_history',
    'get_last_commit_info', 'get_vault_pulse', 'git_file_url',
    'git_add_remote', 'git_discard_file', 'init_git_repo',
    'create_vault_folder', 'delete_vault_folder', 'rename_vault_folder',
  ]) {
    expect(SERVER_COMMANDS.has(cmd)).toBe(true)
  }
})
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm vitest run src/web/mockBridge.test.ts 2>&1 | tail -12` → FAIL (commands absent).

- [ ] **Step 3: Add the commands to `SERVER_COMMANDS`**

Add the 15 command strings under a `// git reads + folder ops (routing sweep)` comment in the `SERVER_COMMANDS` set in `src/web/mockBridge.ts`.

- [ ] **Step 4: Run + tsc**

Run: `pnpm vitest run src/web/mockBridge.test.ts 2>&1 | tail -8` → PASS.
Run: `npx tsc --noEmit 2>&1 | tail -3` → no errors.

- [ ] **Step 5: Commit**

```bash
git add src/web/mockBridge.ts src/web/mockBridge.test.ts
git commit -m "feat(web): route git read + folder commands to the server"
```

---

### Task 6: Frontend — explicitly no-op genuinely desktop-only commands

Stop desktop-only commands from falling through to the server (and 501-ing) by giving the mock bridge an explicit `DESKTOP_ONLY` no-op set.

**Files:**
- Modify: `src/web/mockBridge.ts`
- Test: `src/web/mockBridge.test.ts`

**Interfaces:**
- Consumes: `mockInvoke` dispatch in `mockBridge.ts`.
- Produces: `export const DESKTOP_ONLY = new Set<string>([...])`; `mockInvoke` returns `undefined` for a `DESKTOP_ONLY` command before any server/mock lookup.

- [ ] **Step 1: Write the failing test**

Add to `src/web/mockBridge.test.ts` (stub `fetch` so a regression that sends a request would be observable):

```ts
it('no-ops desktop-only commands without hitting the server', async () => {
  const fetchMock = vi.fn()
  vi.stubGlobal('fetch', fetchMock)
  const { mockInvoke } = await import('./mockBridge')
  for (const cmd of ['copy_text_to_clipboard', 'update_current_window_min_size', 'stream_ai_model', 'check_for_app_update']) {
    await expect(mockInvoke(cmd, {})).resolves.toBeUndefined()
  }
  expect(fetchMock).not.toHaveBeenCalled()
})
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm vitest run src/web/mockBridge.test.ts 2>&1 | tail -12`
Expected: FAIL — some of these currently reach the mock handler or the server.

- [ ] **Step 3: Add the `DESKTOP_ONLY` set + short-circuit**

In `src/web/mockBridge.ts`, add the `DESKTOP_ONLY` set (the full list from the inventory "Desktop-only" row) and, at the top of `mockInvoke`, before the `SERVER_COMMANDS`/mock branches:

```ts
if (DESKTOP_ONLY.has(cmd)) return undefined as T
```

Keep it a first-class explicit set (not a blanket "unknown → undefined" fallback), so a genuinely missing server command still surfaces rather than being silently swallowed.

- [ ] **Step 4: Run + tsc + lint**

Run: `pnpm vitest run src/web/mockBridge.test.ts 2>&1 | tail -8` → PASS.
Run: `npx tsc --noEmit && pnpm lint 2>&1 | tail -5` → clean.

- [ ] **Step 5: Commit**

```bash
git add src/web/mockBridge.ts src/web/mockBridge.test.ts
git commit -m "feat(web): no-op desktop-only commands on web instead of 501"
```

---

### Task 7: Docs — ADR + ARCHITECTURE

**Files:**
- Create: `docs/adr/0152-web-command-surface-completion.md`
- Modify: `docs/ARCHITECTURE.md`, `docs/adr/README.md`

**Interfaces:** none (docs).

- [ ] **Step 1: Write ADR 0152**

Create `docs/adr/0152-web-command-surface-completion.md` (frontmatter `type: ADR`, `id: "0152"`, `status: active`, `date: 2026-07-07`). Document: the three routing categories (git reads via `handlers::dispatch`; repo mutations via `git_handlers` under the repo lock; folder mutations via `write_handlers` with autogit); the casing-tolerant arg extraction (web mock path sends snake_case) as a standing rule for any future web-routed command; the explicit `DESKTOP_ONLY` no-op set (chosen over a blanket unknown-command fallback to avoid masking genuinely missing commands); and the knowingly-deferred commands (`auto_rename_untitled`, `detect_renames`, `update_wikilinks_for_renames`, `batch_delete_notes_async`, `validate_note_content`).

- [ ] **Step 2: Update ARCHITECTURE.md + ADR README**

Extend the "Web client compatibility layer" / "Git sync" section of `docs/ARCHITECTURE.md` noting the completed read/folder surface and the `DESKTOP_ONLY` set. Add the `0152` row to `docs/adr/README.md`.

- [ ] **Step 3: Full gate + commit**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tolaria-core -p tolaria-server 2>&1 | grep -E 'test result:' | tail -4` (green); `pnpm vitest run src/web 2>&1 | tail -5` (green); `npx tsc --noEmit` (clean).

```bash
git add docs/adr/0152-web-command-surface-completion.md docs/ARCHITECTURE.md docs/adr/README.md
git commit -m "docs(adr,architecture): web command surface completion (ADR 0152)"
```

- [ ] **Step 4: Native QA (manual, on the server)**

After rebuilding the web image: open the Changes panel (should show real modified files), a note's diff and history (real git data), the Pulse view (real recent commits), and Add Remote (should configure the real remote). Confirm no `not supported on web` errors in DevTools for clipboard/window/menu actions.

---

## Self-Review

**Command coverage:** every unrouted command from the audit is assigned — git reads (Task 2), repo mutations (Task 3), folder ops (Task 4), routed in the bridge (Task 5), desktop-only no-oped (Task 6), or explicitly deferred (ADR, Task 7). ✅

**Placeholder scan:** each code step carries real code; the "confirm signature / grep core" notes are verification steps for the implementer (the core fns exist and are `pub`, but exact arg order/naming must be checked against the source), not deferred logic. No "add error handling"/"TBD".

**Type consistency:** `arg_str`/`arg_u64`/`serialize` defined in Tasks 1–2 and reused by name; `str_arg` is the git_handlers/write_handlers local twin (same behavior) — the plan flags that they can be shared via `pub(crate)` if preferred. `SERVER_COMMANDS`/`DESKTOP_ONLY` are the exported sets from `mockBridge.ts`. Command names in the Task 5 test match the arms implemented in Tasks 2–4.

**Known implementer verifications (not gaps — environment-dependent):** exact signatures of `get_vault_pulse` (return shape), `get_file_diff_at_commit` (commit arg name/type), and the vault folder fns (`create_folder` naming, `&Path` vs `&str`, return types); whether the frontend sends `commit`/`commitHash` for `get_file_diff_at_commit` and `relativePath`/`filePath` for `git_discard_file` (add the matching alias). Each is a grep + adjust during implementation.

**Casing rule enforced:** Tasks 2/3/4 tests all send snake_case (the real web form); Task 1 makes both forms work. This is the defect class that caused rename to fail — encoded as a first-class constraint and tested.
