---
type: ADR
id: "0160"
title: "tolaria-core shared transport-agnostic crate"
status: active
date: 2026-06-30
---

## Context

Tolaria's vault, git, frontmatter, and search logic lived inside the Tauri
application crate (`tolaria_lib`) under `src-tauri/src/{vault,git,frontmatter,search.rs}`.
These modules were already free of any `tauri` dependency, but they were only
reachable through `#[tauri::command]` wrappers and the desktop binary.

The web client (design spec `docs/superpowers/specs/2026-06-29-web-client-design.md`)
introduces a second consumer of exactly this logic: an Axum server that serves a
single shared vault to browser clients and acts as a git peer. We do not want a
second, divergent reimplementation of vault scanning, frontmatter handling,
keyword search, or git operations. The web server and the desktop app must share
one source of truth for vault behavior.

## Decision

**The transport-agnostic vault/git/frontmatter/search logic is extracted into a
standalone library crate, `tolaria-core`, that both the Tauri desktop app and the
future web server depend on.**

- `src-tauri` becomes a two-member Cargo workspace: the existing app package plus
  `crates/tolaria-core` (lib name `tolaria_core`).
- `tolaria-core` MUST NOT depend on `tauri`, `tauri-*` plugins, `sentry`,
  `reqwest`, or any GUI/IPC crate. It is a pure library.
- Modules moved into the crate, in dependency order:
  `process` (the `hidden_command` spawn helper, previously in `lib.rs`),
  `shell_env` (previously `cli_agent_runtime/shell_env.rs`),
  `frontmatter`, `git`, `vault`, and `search`.
- The Tauri crate re-exports each moved module under its original `crate::` path
  (e.g. `pub use tolaria_core::vault;`) so existing command wrappers and call
  sites are unchanged.
- App-only couplings are removed from the core: the search path no longer reads
  app settings; `hide_gitignored_files` is passed in as a parameter, and the
  Tauri command wrapper reads the setting and forwards it.
- A `build.rs` in `tolaria-core` mirrors Tauri's `desktop` / `mobile` cfg
  convention (set from `CARGO_CFG_TARGET_OS`) so platform-gated code moved out of
  the Tauri crate keeps compiling its branches identically.

## Consequences

- The desktop app builds, passes its full Rust test suite, and is clippy-clean on
  the refactored core; behavior is unchanged (pure moves plus one parameterized
  settings read).
- The web server (later phases) links `tolaria-core` directly, with no Tauri in
  its dependency graph.
- A small number of items were widened from `pub(crate)` to `pub` to cross the new
  crate boundary. Items consumed only by the command layer (e.g.
  `vault::filename_rules` validators) stay `pub`; items now co-located with their
  only consumers inside the crate are candidates to re-narrow to `pub(crate)`.
- The heavy release gates (coverage via `cargo llvm-cov`, CodeScene, Codacy) are
  enforced at push time through the pre-push hook / sidecar lanes, as before; the
  workspace layout keeps `--manifest-path src-tauri/Cargo.toml` working so those
  commands cover both members.
