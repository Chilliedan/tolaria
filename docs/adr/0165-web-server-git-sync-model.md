---
type: ADR
id: "0165"
title: "Web server git sync model (per-save authored commits, repo lock)"
status: active
date: 2026-07-04
---

## Context

ADR-0149 gave the web server authenticated writes (save/create/rename/delete/
frontmatter) but explicitly deferred git: writes landed on disk inside the
mounted vault without being committed. The web client design spec's Phase 5
closes that gap — the browser is now a full git-sync client, matching the
desktop app's behavior, with three requirements specific to a shared,
multi-user server process that a single-user desktop app never had to solve:

1. **Per-user attribution.** Multiple accounts write to the same server
   process and the same vault clone. A commit must record *which user* made
   the change (the git author), not just "the server."
2. **A single push credential.** Desktop users each have their own git
   credentials (SSH key, credential helper) configured locally. A server
   has one filesystem and one git remote config — there is no way to hold a
   separate push credential per logged-in user, so push/pull must work
   under one shared, deploy-time credential.
3. **Concurrent git-index mutation.** Multiple requests (autogit commits from
   concurrent saves, an explicit `git_commit`, a `git_push`) can arrive at
   the same time. `git`'s on-disk index/HEAD is not safe for concurrent
   mutation from multiple threads/processes without external serialization.

Real-time collaboration signals (e.g. broadcasting "user X just pushed" over
the WebSocket, per the design spec's §5.4) are out of scope for this ADR —
see Consequences.

## Decision

### Per-save, path-scoped, dual-identity commits

Every write handler in `write_handlers.rs` (`save_note_content`,
`create_note`/`create_note_content`, `delete_note`,
`update_frontmatter`/`delete_frontmatter_property`) that mutates the
filesystem, when `identity` is present and `AppState::autogit` is enabled,
follows the write with an **autogit commit** via
`tolaria_core::git::git_commit_paths_as`, scoped to exactly the path(s) the
handler touched:

- **Author = the acting user.** `AppState::acting_user` resolves the
  session cookie to a `UsersDb` row and builds a `CommitIdentity` whose
  `author_name`/`author_email` are that user's `git_name`/`git_email`
  (stored since ADR-0148).
- **Committer = a fixed server identity**, from `TOLARIA_COMMITTER_NAME`
  and `TOLARIA_COMMITTER_EMAIL` (defaults: `Tolaria Server` /
  `server@tolaria.local`, resolved once at startup by
  `config::committer_identity`). This mirrors standard git practice for a
  bot/service account that applies changes on a human's behalf (author ≠
  committer), and gives every commit a stable, auditable "applied by"
  identity distinct from the user who requested it.
- **No autogit commit** when `identity` is `None` (unauthenticated — should
  not happen behind `require_auth`, but handled defensively) or when
  `TOLARIA_AUTOGIT=false`. A failed autogit commit logs to stderr and does
  not fail the write itself — the file write already succeeded and is more
  important to the user than the commit.

### Renames commit-all, not path-scoped

`rename_note` and `rename_note_filename` use `autogit_commit_all` (`git add
-A`) instead of the path-scoped `autogit_commit_paths`. A rename triggers
wikilink rewrites across every note that referenced the old title/filename,
and `RenameResult` only exposes a *count* of files touched, not their paths
— there is no path list to scope a commit to. Falling back to "stage
everything changed in the working tree" is the only option that reliably
captures the rename plus all rewritten backlinks in one commit.

### Explicit controls: commit / push / pull / status / conflict resolution

`git_handlers::dispatch_git` adds explicit, user-invoked controls alongside
autogit, all requiring an authenticated session (resolved to the same
per-user author identity as autogit):

- `git_commit` — `git add -A` plus a user-supplied message, authored by the
  acting user. Unlike autogit's path-scoped commits, this is a deliberate,
  explicit user action (they clicked "commit"), so sweeping in any other
  concurrent working-tree edit is an accepted trade-off, not a bug — it
  matches the desktop app's existing "Commit" control, which has the same
  `git add -A` semantics.
- `git_push` / `git_pull` — thin wrappers over `tolaria_core::git::git_push`
  / `git_pull`, using whatever push credential is configured for the
  container process (see below).
- `git_resolve_conflict` / `git_commit_conflict_resolution` — reuse the
  existing `tolaria-core` conflict resolver used by desktop; no
  web-specific conflict logic.
- `git_remote_status` is **not** part of `dispatch_git` — it's read-only
  (ahead/behind/has-conflicts against the current HEAD) and is served by
  the plain synchronous `handlers::dispatch`, alongside the other read
  commands, since it doesn't touch the index and doesn't need the repo
  lock.
- `git_author_identity` is special-cased directly in `rpc::command_route`
  (not routed through `dispatch_git`) because it only needs to reflect the
  logged-in user's own identity from the session cookie — it has no vault
  side effects at all.

### Repo-level lock serializes all index mutation

`AppState::repo_lock` is a single `tokio::sync::Mutex<()>` shared across the
whole server process. Every code path that mutates git state — the autogit
helpers in `write_handlers.rs` and every arm of `git_handlers::dispatch_git`
— acquires it before touching the repository and holds it for the duration
of the git operation. This serializes commit/push/pull/conflict-resolution
process-wide, so two concurrent requests can never interleave git-index
writes.

**Lock ordering is always per-path-lock → repo-lock, never reversed.**
Write handlers acquire their `PathLocks` guard for the file(s) being
written first, then (still holding it) acquire `repo_lock` to make the
autogit commit. Explicit `git_commit`/`git_push`/`git_pull`/conflict
handlers only ever acquire `repo_lock` (they don't hold a path lock at
all). Because no code path acquires `repo_lock` first and then blocks on a
path lock, the two lock kinds can never deadlock against each other.

### Autogit toggle

`TOLARIA_AUTOGIT` (default **on**; set to `false` or `0` to disable) lets an
operator run the server as pure write-without-commit (ADR-0149's original
Phase 4 behavior) if they'd rather commit manually or via an external
process. `config::autogit_enabled()` reads it once at startup into
`AppState::autogit`.

### Single server-side push credential, supplied at deploy time

The server has no concept of a per-user push credential — it has one
filesystem-level identity for talking to the remote. `git_push`/`git_pull`
use whatever credential the container's git config resolves: an SSH deploy
key mounted read-only into the container plus `GIT_SSH_COMMAND` pointing at
it, or an HTTPS remote with a credential helper reading a mounted token.
`docker-compose.yml` documents this as a commented example (commented out
so a key-less deployment still starts cleanly); the vault clone mounted
into the container must already have its remote configured (`git remote
add origin ...`) at provisioning time — the server never runs `git remote
add` itself. This is a deliberate simplification: attribution to individual
users is fully preserved through commit *authorship* (see above); the push
credential only controls whether the *server process* is allowed to talk
to the remote at all, same as a CI deploy key.

### Real-time events deferred

The design spec's §5.4 (WebSocket events broadcasting live git-sync state —
e.g. "another session just committed/pushed" — to connected browser
clients) is explicitly **deferred to a later phase**. Phase 5 ships the
control surface (commit/push/pull/status/conflict resolution) and
correctness (locking, authorship) but not live multi-client notification;
a client only sees updated remote status when it calls
`git_remote_status` again.

### QA

Git commit/push/pull against a real server and a real remote can't be
meaningfully exercised through the mock-based Playwright/dev harness used
for the rest of the web client's smoke coverage — the mock bridge has no
real git repository or remote to operate against, and a fake one wouldn't
exercise the code path under test (real git process invocation, real
locking, real remote auth). Git-sync QA for this phase is **native/manual**
against a real deployment: build the Docker image, log in as a real user,
edit a note, verify the resulting commit is authored correctly via `git
log` in the mounted vault, then exercise push/pull/status through the UI
against a real remote.

## Consequences

- Every write the web client makes is now committed automatically (subject
  to `TOLARIA_AUTOGIT`), attributed to the user who made it, without that
  user needing their own git credentials on the server — feature parity
  with desktop's per-user local commits, adapted to a shared server
  process.
- Operators who want push/pull must provision exactly one deploy credential
  (SSH key or HTTPS token) and configure the vault clone's remote once;
  there is no per-user credential management to build or operate.
- The explicit `git_commit` control's `git add -A` semantics mean a user
  who clicks "commit" while another save is mid-flight can end up
  co-committing that other change. This is accepted as matching desktop's
  existing behavior and as being triggered only by a deliberate user
  action, not a background process.
- **Known trade-off (recorded at review):** the per-path lock is currently
  held across the `repo_lock` acquisition in the autogit path (path lock
  acquired first, then repo lock, per the ordering rule above) — this means
  concurrent saves to *different* files still serialize with each other
  during the commit step, because they all contend for the same
  `repo_lock` while each is still holding its own path lock. This is a
  throughput cost, not a correctness issue (no data race, no deadlock,
  every save still lands on disk and commits correctly), and is flagged
  here as a possible future optimization — e.g. releasing the path lock
  before acquiring the repo lock once the write itself is durable.
- Real-time WebSocket sync events remain unimplemented; a connected client
  only learns about another session's commit/push through an explicit
  `git_remote_status` call, not a push notification. Revisit this ADR (or
  supersede it) if/when that lands.
- Git-sync correctness is exercised by Rust unit/integration tests in
  `git_handlers.rs`/`write_handlers.rs` (e.g.
  `git_commit_is_authored_by_acting_user`) plus native/manual QA; there is
  no `@smoke` Playwright spec for this phase, per the reasoning above.
