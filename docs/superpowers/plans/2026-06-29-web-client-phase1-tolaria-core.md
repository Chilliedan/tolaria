# Web Client — Phase 1: Extract `tolaria-core` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract Tolaria's transport-agnostic vault/git/search/frontmatter logic out of the Tauri crate into a reusable `tolaria-core` library crate, with the desktop app rebuilt on top of it and all existing gates still passing.

**Architecture:** Convert `src-tauri` into a two-member Cargo workspace: the existing Tauri app (`tolaria_lib`) plus a new dependency-free library crate `tolaria-core`. The pure modules (`vault/`, `git/`, `frontmatter/`, `search.rs`) and two small process/env helpers move into `tolaria-core` in dependency order; the Tauri layer keeps only thin command wrappers that call into the core. This is the foundation every later web-client phase builds on (see the design spec).

**Tech Stack:** Rust (edition 2021, rust-version 1.77.2), Cargo workspaces, existing crates (`serde`, `gray_matter`, `walkdir`, `chrono`, `regex`, `tempfile`).

**Spec:** `docs/superpowers/specs/2026-06-29-web-client-design.md` (§4.1, §13 Phase 1)

## Global Constraints

- Rust edition `2021`, `rust-version = "1.77.2"` — `tolaria-core` MUST use the same.
- `tolaria-core` MUST NOT depend on `tauri`, `tauri-*` plugins, `sentry`, `reqwest`, or any GUI/IPC crate. It is a pure library.
- Coverage gate: `cargo llvm-cov --manifest-path src-tauri/Cargo.toml --no-clean --fail-under-lines 85` must pass (workspace-wide; core's moved tests count).
- CodeScene: every touched file must leave with a score ≥ its starting score; files already at `10.0` stay `10.0`; new scorable files reach `10.0` (or have zero findings if unscorable). **Pure file moves preserve content, so scores are unchanged — do not reformat moved files.**
- Codacy: no new Critical/High findings.
- **⛔ NEVER use `--no-verify`. NEVER edit `.codescene-thresholds` downward. NEVER add `#[allow(...)]` / `as any` / lint-disables.**
- Localization: this phase changes **no UI copy** — record "Localization: no UI copy changes" in any completion comment.
- Move files with `git mv` so history and content are preserved exactly.
- Commit after every task. Use `refactor:` for moves, `feat:`/`test:` where new code/tests are added.

## File Structure (target)

```
src-tauri/
  Cargo.toml                      # becomes workspace root + app package
  src/                            # tolaria_lib (Tauri app) — keeps command wrappers
    lib.rs                        # hidden_command moved out; re-exports from core
    search.rs                     # becomes thin wrapper calling core::search
    cli_agent_runtime.rs          # re-exports shell_env items from core
    commands/...                  # use-paths updated to tolaria_core::*
  crates/
    tolaria-core/
      Cargo.toml                  # new library crate
      src/
        lib.rs                    # pub mod process; shell_env; frontmatter; git; vault; search;
        process.rs                # hidden_command (moved from lib.rs)
        shell_env.rs              # moved from cli_agent_runtime/shell_env.rs
        frontmatter/              # moved from src/frontmatter/
        git/                      # moved from src/git/
        vault/                    # moved from src/vault/
        search.rs                 # moved from src/search.rs, settings read parameterized
```

**Dependency order (build bottom-up):** `process` → `shell_env` → `frontmatter` → `git` → `vault` → `search`. Each task moves one layer and ends green.

---

### Task 1: Create the workspace and empty `tolaria-core` crate

**Files:**
- Modify: `src-tauri/Cargo.toml` (add `[workspace]` table)
- Create: `src-tauri/crates/tolaria-core/Cargo.toml`
- Create: `src-tauri/crates/tolaria-core/src/lib.rs`

**Interfaces:**
- Produces: a buildable workspace with member crate `tolaria-core` (lib name `tolaria_core`). No public items yet.

- [ ] **Step 1: Add the workspace table to `src-tauri/Cargo.toml`**

Insert this block immediately **above** the existing `[package]` line at the top of `src-tauri/Cargo.toml`:

```toml
[workspace]
members = [".", "crates/tolaria-core"]
resolver = "2"

```

- [ ] **Step 2: Create the core crate manifest**

Create `src-tauri/crates/tolaria-core/Cargo.toml`:

```toml
[package]
name = "tolaria-core"
version = "0.1.0"
edition = "2021"
rust-version = "1.77.2"
license = "AGPL-3.0-or-later"

[lib]
name = "tolaria_core"

[dependencies]

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: Create the empty crate root**

Create `src-tauri/crates/tolaria-core/src/lib.rs`:

```rust
//! Transport-agnostic core for Tolaria: vault, git, frontmatter, and search
//! logic shared by the desktop (Tauri) app and the web server. No GUI/IPC deps.
```

- [ ] **Step 4: Verify the workspace builds**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: PASS — both `tolaria` and `tolaria-core` compile (core is empty).

- [ ] **Step 5: Verify existing tests still pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: PASS — no behavior changed.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/crates/tolaria-core
git commit -m "refactor: add tolaria-core workspace member crate"
```

---

### Task 2: Move the `process` helper (`hidden_command`) into core

**Files:**
- Create: `src-tauri/crates/tolaria-core/src/process.rs`
- Modify: `src-tauri/crates/tolaria-core/src/lib.rs`
- Modify: `src-tauri/src/lib.rs` (remove the fn, re-export from core)

**Interfaces:**
- Produces: `tolaria_core::process::hidden_command(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command`.
- Consumes: nothing.

- [ ] **Step 1: Read the current definition**

Read `src-tauri/src/lib.rs` around line 56 (`pub(crate) fn hidden_command`) and copy the **entire** function body, including any `#[cfg(windows)]` blocks and the imports it needs (`std::ffi::OsStr`, `std::process::Command`, and on Windows `std::os::windows::process::CommandExt`).

- [ ] **Step 2: Create `process.rs` in core with the moved function**

Create `src-tauri/crates/tolaria-core/src/process.rs` with the function copied verbatim, but change `pub(crate)` to `pub`:

```rust
use std::ffi::OsStr;
use std::process::Command;

/// Build a `Command` that does not flash a console window on Windows.
/// (Body copied verbatim from the previous `tolaria_lib::hidden_command`,
/// including its `#[cfg(windows)]` window-flag handling.)
pub fn hidden_command(program: impl AsRef<OsStr>) -> Command {
    // ... exact body moved from src-tauri/src/lib.rs ...
}
```

- [ ] **Step 3: Declare the module in core**

Add to `src-tauri/crates/tolaria-core/src/lib.rs`:

```rust
pub mod process;
```

- [ ] **Step 4: Add core as a dependency of the Tauri crate**

In `src-tauri/Cargo.toml`, under `[dependencies]`, add:

```toml
tolaria-core = { path = "crates/tolaria-core" }
```

- [ ] **Step 5: Remove the old definition and re-export from core**

In `src-tauri/src/lib.rs`, delete the `pub(crate) fn hidden_command(...) { ... }` definition and replace it with a re-export so all existing `crate::hidden_command` call sites keep compiling:

```rust
pub(crate) use tolaria_core::process::hidden_command;
```

- [ ] **Step 6: Build**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: PASS — every `crate::hidden_command` / `hidden_command(...)` call site resolves via the re-export.

- [ ] **Step 7: Test**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/lib.rs src-tauri/crates/tolaria-core
git commit -m "refactor: move hidden_command into tolaria-core::process"
```

---

### Task 3: Move `shell_env` into core

**Files:**
- Move: `src-tauri/src/cli_agent_runtime/shell_env.rs` → `src-tauri/crates/tolaria-core/src/shell_env.rs`
- Modify: `src-tauri/crates/tolaria-core/src/lib.rs`
- Modify: `src-tauri/src/cli_agent_runtime.rs` (re-export from core)

**Interfaces:**
- Produces: `tolaria_core::shell_env::{EnvName, env_value_from_process_or_user_shell, apply_user_shell_env_vars_if_missing}` (and the rest of that file's public surface).
- Consumes: `tolaria_core::process::hidden_command`.

- [ ] **Step 1: Move the file**

```bash
git mv src-tauri/src/cli_agent_runtime/shell_env.rs src-tauri/crates/tolaria-core/src/shell_env.rs
```

- [ ] **Step 2: Fix the intra-crate reference inside the moved file**

In `src-tauri/crates/tolaria-core/src/shell_env.rs`, change the dependency on the process helper from the old crate path to the core path. Replace any `crate::hidden_command` with `crate::process::hidden_command` (this file is now inside `tolaria-core`).

- [ ] **Step 3: Promote visibility for cross-crate use**

In the moved `shell_env.rs`, change the `pub(crate)` visibility on `EnvName`, `env_value_from_process_or_user_shell`, and `apply_user_shell_env_vars_if_missing` (and any other item used outside the module) to `pub` so the Tauri crate and `git/` can use them across the crate boundary.

- [ ] **Step 4: Declare the module in core**

Add to `src-tauri/crates/tolaria-core/src/lib.rs`:

```rust
pub mod shell_env;
```

- [ ] **Step 5: Re-export from the old location**

In `src-tauri/src/cli_agent_runtime.rs`, remove the now-deleted `mod shell_env;` declaration and the line that imports those items from the local module. Replace with a re-export from core so existing `crate::cli_agent_runtime::{EnvName, ...}` call sites keep working:

```rust
pub(crate) use tolaria_core::shell_env::{
    apply_user_shell_env_vars_if_missing, env_value_from_process_or_user_shell, EnvName,
};
```

- [ ] **Step 6: Build**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: PASS.

- [ ] **Step 7: Test**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: PASS — `shell_env` tests now run inside the core crate.

- [ ] **Step 8: Commit**

```bash
git add -A src-tauri/src/cli_agent_runtime.rs src-tauri/crates/tolaria-core src-tauri/src/cli_agent_runtime/
git commit -m "refactor: move shell_env into tolaria-core"
```

---

### Task 4: Move `frontmatter/` into core

**Files:**
- Move: `src-tauri/src/frontmatter/` → `src-tauri/crates/tolaria-core/src/frontmatter/`
- Modify: `src-tauri/crates/tolaria-core/src/Cargo.toml` (add `gray_matter`, `serde`, `serde_yaml` if used)
- Modify: `src-tauri/crates/tolaria-core/src/lib.rs`
- Modify: `src-tauri/src/lib.rs` (remove `mod frontmatter;`, re-export from core)

**Interfaces:**
- Produces: `tolaria_core::frontmatter::*` (same public surface as today's `crate::frontmatter`).
- Consumes: nothing from other moved layers.

- [ ] **Step 1: Inspect dependencies the module needs**

Run: `grep -rhoE "(serde|serde_yaml|serde_json|gray_matter|chrono|regex)\b" src-tauri/src/frontmatter/ | sort -u`
Note which external crates appear; add exactly those to core's `[dependencies]` in the next step.

- [ ] **Step 2: Add required deps to core**

In `src-tauri/crates/tolaria-core/Cargo.toml`, add under `[dependencies]` the crates found in Step 1, pinned to the same versions as `src-tauri/Cargo.toml`. For example (include only those actually used):

```toml
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
serde_yaml = "0.9"
gray_matter = "0.2"
```

- [ ] **Step 3: Move the module**

```bash
git mv src-tauri/src/frontmatter src-tauri/crates/tolaria-core/src/frontmatter
```

- [ ] **Step 4: Declare the module in core and remove from app**

Add `pub mod frontmatter;` to `src-tauri/crates/tolaria-core/src/lib.rs`.
In `src-tauri/src/lib.rs`, remove `mod frontmatter;` (or `pub mod frontmatter;`) and add a re-export so existing `crate::frontmatter::*` references resolve:

```rust
pub(crate) use tolaria_core::frontmatter;
```

- [ ] **Step 5: Fix any intra-module `crate::` paths**

Run: `grep -rn "crate::" src-tauri/crates/tolaria-core/src/frontmatter/`
For any reference to a sibling that now lives in core (e.g. `crate::frontmatter::...` self-references are fine), leave as-is; there should be no references to Tauri-side modules. If `grep` shows a path pointing outside core, stop and reassess.

- [ ] **Step 6: Build**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: PASS.

- [ ] **Step 7: Test**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: PASS — frontmatter tests (incl. `ops_update_tests`) run in core.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor: move frontmatter into tolaria-core"
```

---

### Task 5: Move `git/` into core

**Files:**
- Move: `src-tauri/src/git/` → `src-tauri/crates/tolaria-core/src/git/`
- Modify: `src-tauri/crates/tolaria-core/Cargo.toml` (add git module deps)
- Modify: `src-tauri/crates/tolaria-core/src/lib.rs`
- Modify: `src-tauri/src/lib.rs` (remove `mod git;`, re-export)

**Interfaces:**
- Produces: `tolaria_core::git::*` (same public surface as today's `crate::git`).
- Consumes: `tolaria_core::process::hidden_command`, `tolaria_core::shell_env::{EnvName, env_value_from_process_or_user_shell}`.

- [ ] **Step 1: Inspect external + intra-crate deps**

Run:
```bash
grep -rhoE "(serde|serde_json|serde_yaml|chrono|regex|tempfile|uuid)\b" src-tauri/src/git/ | sort -u
grep -rn "crate::" src-tauri/src/git/ | grep -vE "crate::git" | sort -u
```
The second command should show only `crate::hidden_command`, `crate::cli_agent_runtime::{...}` (env helpers), and possibly `crate::frontmatter` / `crate::vault`-free references. Note them.

- [ ] **Step 2: Add required external deps to core**

Add to core `Cargo.toml [dependencies]` any crates from Step 1 not already present, pinned to match `src-tauri/Cargo.toml` (e.g. `chrono = { version = "0.4", features = ["serde"] }`, `regex = "1"`, `uuid = { version = "1", features = ["v4"] }`).

- [ ] **Step 3: Move the module**

```bash
git mv src-tauri/src/git src-tauri/crates/tolaria-core/src/git
```

- [ ] **Step 4: Rewrite intra-crate paths inside the moved module**

Inside `src-tauri/crates/tolaria-core/src/git/`, update imports to core-local paths:
- `crate::hidden_command` → `crate::process::hidden_command`
- `crate::cli_agent_runtime::{env_value_from_process_or_user_shell, EnvName}` → `crate::shell_env::{env_value_from_process_or_user_shell, EnvName}`
- any `crate::frontmatter::...` stays `crate::frontmatter::...` (frontmatter is now in core too)

Run `grep -rn "crate::cli_agent_runtime\|crate::hidden_command" src-tauri/crates/tolaria-core/src/git/` afterward; expect **no matches**.

- [ ] **Step 5: Declare in core, re-export from app**

Add `pub mod git;` to core `lib.rs`.
In `src-tauri/src/lib.rs`, remove `mod git;` / `pub mod git;` and add:

```rust
pub(crate) use tolaria_core::git;
```

- [ ] **Step 6: Build**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: PASS.

- [ ] **Step 7: Test**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: PASS — git module tests run in core.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor: move git module into tolaria-core"
```

---

### Task 6: Move `vault/` into core

**Files:**
- Move: `src-tauri/src/vault/` → `src-tauri/crates/tolaria-core/src/vault/`
- Modify: `src-tauri/crates/tolaria-core/Cargo.toml` (add vault module deps)
- Modify: `src-tauri/crates/tolaria-core/src/lib.rs`
- Modify: `src-tauri/src/lib.rs` (remove `mod vault;`, re-export)

**Interfaces:**
- Produces: `tolaria_core::vault::*` (same public surface as today's `crate::vault`).
- Consumes: `tolaria_core::git::*`, `tolaria_core::frontmatter::*`.

- [ ] **Step 1: Inspect external + intra-crate deps**

Run:
```bash
grep -rhoE "(serde|serde_json|serde_yaml|gray_matter|walkdir|chrono|regex|tempfile|dirs|base64|csv)\b" src-tauri/src/vault/ | sort -u
grep -rn "crate::" src-tauri/src/vault/ | grep -vE "crate::vault" | sort -u
```
Expected intra-crate refs: `crate::git`, `crate::frontmatter` (both now in core). If anything else outside core appears (e.g. `crate::settings`), stop and reassess — it must be parameterized like Task 7 does for search.

- [ ] **Step 2: Add required external deps to core**

Add to core `Cargo.toml [dependencies]` any crates from Step 1 not already present, pinned to match `src-tauri/Cargo.toml` (e.g. `walkdir = "2"`, `dirs = "5"`, `base64 = "0.22"`, `csv = "1.4"`).

- [ ] **Step 3: Move the module**

```bash
git mv src-tauri/src/vault src-tauri/crates/tolaria-core/src/vault
```

- [ ] **Step 4: Verify intra-crate paths resolve**

Run: `grep -rn "crate::git\|crate::frontmatter" src-tauri/crates/tolaria-core/src/vault/ | head`
These now resolve within core (no change needed). Run `grep -rn "crate::settings\|crate::telemetry\|crate::hidden_command\|crate::cli_agent_runtime" src-tauri/crates/tolaria-core/src/vault/` and expect **no matches**.

- [ ] **Step 5: Declare in core, re-export from app**

Add `pub mod vault;` to core `lib.rs`.
In `src-tauri/src/lib.rs`, remove `mod vault;` / `pub mod vault;` and add:

```rust
pub(crate) use tolaria_core::vault;
```

- [ ] **Step 6: Build**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: PASS — `commands/vault/*` wrappers still reference `crate::vault::*` via the re-export.

- [ ] **Step 7: Test**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: PASS — all vault tests (`mod_tests`, `parsing_tests`, `view_tests`, etc.) run in core.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor: move vault module into tolaria-core"
```

---

### Task 7: Move `search.rs` into core and parameterize its settings read

This is the one module with a Tauri-side coupling: `src-tauri/src/search.rs:129` calls `crate::settings::hide_gitignored_files_enabled()`. Core must not read app settings, so the flag becomes a parameter the caller supplies.

**Files:**
- Move: `src-tauri/src/search.rs` → `src-tauri/crates/tolaria-core/src/search.rs`
- Modify: `src-tauri/crates/tolaria-core/src/lib.rs`
- Modify: `src-tauri/src/lib.rs` (remove `mod search;`, add a thin wrapper module)
- Modify: the search Tauri command wrapper (in `src-tauri/src/commands/` — find with grep in Step 1)
- Test: `src-tauri/crates/tolaria-core/src/search.rs` (add a test for the flag behavior)

**Interfaces:**
- Produces: a core search entry point whose signature takes `hide_gitignored_files: bool` instead of reading settings. Exact name: keep the existing public function name (e.g. `tolaria_core::search::search_vault`) but **add a `hide_gitignored_files: bool` parameter** to the function that currently calls `crate::settings::hide_gitignored_files_enabled()`.
- Consumes: `tolaria_core::vault::*` if search references vault types (verify in Step 1).

- [ ] **Step 1: Locate the call site and the Tauri command**

Run:
```bash
sed -n '120,140p' src-tauri/src/search.rs
grep -rn "search_vault\|fn search" src-tauri/src/commands/ src-tauri/src/lib.rs
grep -rn "crate::" src-tauri/src/search.rs | grep -vE "crate::vault" | sort -u
```
Record: the exact function that reads `hide_gitignored_files_enabled()`, the `#[tauri::command]` wrapper name and its file, and any non-vault `crate::` deps.

- [ ] **Step 2: Write the failing test for the parameterized flag (in core)**

Move the file first so the test lives in core:

```bash
git mv src-tauri/src/search.rs src-tauri/crates/tolaria-core/src/search.rs
```

Append a test to `src-tauri/crates/tolaria-core/src/search.rs` that builds a temp vault with one gitignored file and one normal file, then asserts the flag controls inclusion. Use the real public function name found in Step 1 (shown here as `search_vault`):

```rust
#[cfg(test)]
mod hide_gitignored_param_tests {
    use super::*;
    use tempfile::tempdir;
    use std::fs;

    #[test]
    fn respects_hide_gitignored_flag() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join(".gitignore"), "secret.md\n").unwrap();
        fs::write(root.join("secret.md"), "needle in ignored\n").unwrap();
        fs::write(root.join("visible.md"), "needle in visible\n").unwrap();

        let shown = search_vault(root.to_str().unwrap(), "needle", "all", false, false);
        let hidden = search_vault(root.to_str().unwrap(), "needle", "all", false, true);

        assert!(shown.len() >= 2, "no hiding -> ignored file included");
        assert!(
            hidden.iter().all(|r| !r.path.ends_with("secret.md")),
            "hide flag must exclude gitignored files"
        );
    }
}
```

> Adjust the argument list/return type to match the **actual** `search_vault` signature recorded in Step 1. The point is: a new trailing `hide_gitignored_files: bool` parameter, and the gitignored file is excluded only when it is `true`.

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tolaria-core respects_hide_gitignored_flag`
Expected: FAIL to compile — `search_vault` does not yet take the `hide_gitignored_files` parameter.

- [ ] **Step 4: Parameterize the core function**

In `src-tauri/crates/tolaria-core/src/search.rs`:
- Add `hide_gitignored_files: bool` as a parameter to the function that previously read the setting.
- Replace the line `hide_gitignored_files: crate::settings::hide_gitignored_files_enabled(),` with `hide_gitignored_files,` (use the passed parameter).
- Remove any now-unused `use crate::settings...`.

- [ ] **Step 5: Declare the module in core, remove from app**

Add `pub mod search;` to `src-tauri/crates/tolaria-core/src/lib.rs`.
In `src-tauri/src/lib.rs`, remove `mod search;` / `pub mod search;`.

- [ ] **Step 6: Update the Tauri command wrapper to read the setting and pass it**

In the command file found in Step 1, change the `#[tauri::command]` wrapper so it reads the setting on the Tauri side and forwards it to core. Example (adapt names/args to the real signature):

```rust
#[tauri::command]
pub fn search_vault(vault_path: String, query: String, mode: String, exclude_frontmatter: bool) -> Vec<SearchResult> {
    let hide_gitignored = crate::settings::hide_gitignored_files_enabled();
    tolaria_core::search::search_vault(&vault_path, &query, &mode, exclude_frontmatter, hide_gitignored)
}
```

Add `use tolaria_core::search::...` / `SearchResult` imports as needed (re-export `SearchResult` from core if the command signature references it: `pub(crate) use tolaria_core::search::SearchResult;` in `lib.rs`).

- [ ] **Step 7: Run the new test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tolaria-core respects_hide_gitignored_flag`
Expected: PASS.

- [ ] **Step 8: Build and run the full suite**

Run: `cargo build --manifest-path src-tauri/Cargo.toml && cargo test --manifest-path src-tauri/Cargo.toml`
Expected: PASS — desktop search command compiles against the parameterized core function; all existing search tests pass.

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "refactor: move search into tolaria-core with parameterized gitignore flag

test: add core test asserting hide_gitignored flag controls results"
```

---

### Task 8: Tighten core's public surface and verify all gates

**Files:**
- Modify: `src-tauri/crates/tolaria-core/src/lib.rs` (module doc + ordering)
- Modify: `src-tauri/src/lib.rs` (consolidate re-exports)

**Interfaces:**
- Produces: a clean, documented `tolaria_core` crate root that every later web-client phase consumes.

- [ ] **Step 1: Confirm no pure logic remains in the app crate**

Run:
```bash
ls src-tauri/src/vault src-tauri/src/git src-tauri/src/frontmatter src-tauri/src/search.rs 2>&1
```
Expected: all report "No such file or directory" (everything moved to core).

- [ ] **Step 2: Confirm core has no forbidden dependencies**

Run: `grep -rn "tauri\|sentry\|reqwest" src-tauri/crates/tolaria-core/src/ src-tauri/crates/tolaria-core/Cargo.toml`
Expected: **no matches**. If any appear, the corresponding logic was moved incorrectly — fix before continuing.

- [ ] **Step 3: Order and document core module declarations**

Ensure `src-tauri/crates/tolaria-core/src/lib.rs` reads (dependency order, all `pub`):

```rust
//! Transport-agnostic core for Tolaria: vault, git, frontmatter, and search
//! logic shared by the desktop (Tauri) app and the web server. No GUI/IPC deps.

pub mod process;
pub mod shell_env;
pub mod frontmatter;
pub mod git;
pub mod vault;
pub mod search;
```

- [ ] **Step 4: Run clippy across the workspace**

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
Expected: PASS with zero warnings. Fix any clippy findings in core **without** `#[allow(...)]`.

- [ ] **Step 5: Run coverage gate**

Run: `cargo llvm-cov --manifest-path src-tauri/Cargo.toml --no-clean --fail-under-lines 85`
Expected: PASS — line coverage ≥85% workspace-wide (moved tests keep coverage intact).

- [ ] **Step 6: Run CodeScene and Codacy checks on touched files**

- CodeScene: review each touched file (`src-tauri/src/lib.rs`, `src-tauri/src/cli_agent_runtime.rs`, the search command file, and the new `tolaria-core` files). Moved files must equal their prior score; new files (`lib.rs`, `process.rs`, `Cargo.toml`) must be `10.0` or have zero findings. Use `mcp__codescene__code_health_score` / `cs` CLI per AGENTS.md.
- Codacy: `.codacy/cli.sh analyze src-tauri/crates/tolaria-core --format sarif` (or Codacy MCP). Fix any new Critical/High.

- [ ] **Step 7: Frontend untouched — sanity check**

Run: `git status --short -- src demo-vault demo-vault-v2`
Expected: no frontend or demo-vault changes (this phase is Rust-only). Demo-vault must be clean.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor: finalize tolaria-core public surface and verify gates"
```

- [ ] **Step 9: Create the ADR for the extraction**

Use `/create-adr` to add an ADR recording the `tolaria-core` extraction (the shared, transport-agnostic core consumed by both the Tauri app and the future web server — design spec §4.1, §12 item 1). Commit it in the same step.

```bash
git add docs/adr/
git commit -m "docs: ADR for tolaria-core extraction"
```

---

## Self-Review

**Spec coverage (Phase 1 scope = design §4.1, §13 step 1):**
- "Extract transport-agnostic vault/git/search/frontmatter into a Tauri-free crate" → Tasks 3–7 (frontmatter, git, vault, search) + Tasks 1–2 (workspace + process helper).
- "Both desktop app and server depend on it" → desktop now depends on `tolaria-core` (Task 2 adds the dep; Tasks 3–7 rewire); server is a later phase.
- "Desktop keeps passing all gates" → Task 8 (clippy, coverage, CodeScene, Codacy, demo-vault hygiene).
- "Single source of truth, no reimplementation" → all logic moved via `git mv`, not rewritten.
- ADR for the architecture decision → Task 8 Step 9.

**Out of Phase 1 (correctly deferred to later plans):** the Axum server, auth, transport shim, optimistic concurrency, git sync UI — none appear here, as intended.

**Placeholder scan:** No "TBD"/"handle edge cases"/"similar to". The one signature that depends on real code (`search_vault`) is explicitly flagged to be matched to the actual signature discovered in Task 7 Step 1, with the exact transformation (add trailing `hide_gitignored_files: bool`, replace the settings read) specified.

**Type consistency:** Re-export names are consistent — `tolaria_core::process::hidden_command`, `tolaria_core::shell_env::{EnvName, env_value_from_process_or_user_shell, apply_user_shell_env_vars_if_missing}`, `tolaria_core::{frontmatter, git, vault, search}`. The app crate re-exports each under its original `crate::` name so existing call sites are unchanged.

**Note on TDD framing:** Tasks 1–6 are pure module *moves* — the "test" leg is the existing suite staying green (the safety net for a refactor), not new red tests. Task 7 introduces genuine new behavior (parameterized flag) and follows full red→green TDD.
