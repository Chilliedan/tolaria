---
type: ADR
id: "0166"
title: "Web command surface completion (git reads, folder ops, desktop-only no-ops)"
status: active
date: 2026-07-07
---

## Context

ADR-0150 introduced `mockBridge.ts`'s `SERVER_COMMANDS` allowlist so the
unmodified desktop React app's `mockInvoke` fallback could route
server-implemented commands to the real HTTP transport while everything else
fell through to the in-memory desktop mock. ADR-0151 then routed the git
*control* commands (`git_commit`, `git_push`, `git_pull`, conflict
resolution, `git_remote_status`, `git_author_identity`). That left a wide
band of commands still on the mock: every git *read* the UI needs to render
real repository state, the git *mutations* that configure or reset a repo,
and vault *folder* mutations — plus a long tail of genuinely desktop-only
commands that had no home on either allowlist and were falling through to
the server's 501 path.

Two concrete symptoms drove this task. First, the Changes panel, a note's
diff/history view, and the Pulse view all rendered **fake mock data** in the
browser instead of the real vault's git state, because `is_git_repo`,
`get_modified_files`, `get_file_diff`, `get_file_history`,
`get_last_commit_info`, and `get_vault_pulse` were not on `SERVER_COMMANDS`
and silently resolved through `mockHandlers`. Second, "Add Remote" and
folder create/rename/delete appeared to succeed in the UI but had no effect
on the real vault, because `git_add_remote` and the `create_vault_folder` /
`delete_vault_folder` / `rename_vault_folder` trio were in the same
situation. Both symptoms are instances of the same defect class: a command
missing from `SERVER_COMMANDS` doesn't fail loudly, it degrades to
plausible-looking fake data, which is far harder to notice in QA than an
outright error.

Closing this gap also surfaced a second, more subtle problem: the web
client's `mockInvoke` call sites send **different argument shapes** than the
native Tauri IPC call sites for the same logical command (see Decision
below). A first attempt at wiring some of these commands used the desktop
arg keys and silently failed against real web traffic — the handler
parsed successfully (the JSON key existed) but produced `null`/no-op
results because it read the wrong nested location. This is the class of
bug that motivated making the arg-shape rule explicit here rather than
leaving it as tribal knowledge in individual test cases.

## Decision

### Three routing categories, three dispatch paths

**Git reads — `handlers::dispatch`, no lock.** `is_git_repo`,
`get_modified_files` (+`_with_stats`), `get_file_diff` (+`_at_commit`),
`get_file_history`, `get_last_commit_info`, `get_vault_pulse`, and
`git_file_url` are pure reads against `tolaria_core::git`. They join the
existing read-only dispatch table alongside `list_vault`/`get_note_content`/
`search_vault` — no `AppState::repo_lock` needed because they never mutate
the git index.

**Repo mutations — `git_handlers::dispatch_git`, under the repo lock.**
`git_add_remote`, `git_discard_file`, and `init_git_repo` join the existing
`git_commit`/`git_push`/`git_pull`/conflict-resolution arms in
`dispatch_git`, which acquires `AppState::repo_lock` (per ADR-0151's
lock-ordering rule: path-lock-then-repo-lock, never reversed) before
touching the repository. These are mutations, so they get the same
serialization guarantee as commit/push/pull.

**Folder mutations — `write_handlers`, path lock + autogit-commit-all.**
`create_vault_folder`, `delete_vault_folder`, and `rename_vault_folder` are
routed through `write_handlers`, take the per-path lock (`PathLocks`), and,
like `rename_note`, follow the mutation with an `autogit_commit_all` (`git
add -A`) rather than a path-scoped commit — a folder rename/delete can touch
an unbounded set of files underneath it, so there is no fixed path list to
scope a commit to, same reasoning ADR-0151 already established for note
renames. `create_vault_folder` is backed by a new core function,
`tolaria_core::vault::create_folder(vault_path, folder_path)`, so the
directory-creation logic itself lives in the transport-agnostic crate rather
than being duplicated in the server handler.

### The arg-shape rule (load-bearing)

The web client's `mockInvoke` call sites and the native Tauri IPC call
sites for the *same logical command* do not send the same JSON shape. Any
command reached through `mockBridge.ts`'s `SERVER_COMMANDS` allowlist must
tolerate both:

- **Casing.** The mock/web path sends **snake_case** keys
  (`commit_hash`, `remote_url`, `folder_name`); native Tauri IPC sends
  **camelCase** (`commitHash`, `remoteUrl`, `folderName`). Every arg read on
  a web-routed command goes through a casing-tolerant extractor —
  `handlers::arg_str(args, camel, snake)` for git reads, and a local
  `str_arg` twin with identical behavior in both `git_handlers.rs` and
  `write_handlers.rs` — that checks the camelCase key first, then falls back
  to snake_case.
- **Key naming, not just casing.** Some commands use a *different word*
  entirely between the two paths, not just different casing. The file-path
  argument for git read commands arrives as `path` on the real web call
  sites (not `filePath`/`file_path`) — `handlers::file_path_arg` checks
  `path` first and falls back to `file_path`/`filePath` for other callers.
  `git_discard_file` similarly accepts `relativePath`/`relative_path` first,
  then falls back to `filePath`/`file_path`.
- **Envelope nesting.** `git_add_remote`'s web call site wraps its argument
  in a `request` envelope — `{ request: { remoteUrl: "..." } }` — rather
  than sending `remoteUrl` at the top level. The handler reads
  `args.get("request").unwrap_or(&args)` before extracting `remoteUrl`, so
  both the enveloped web shape and a flat shape work.
- **Different field names for conceptually the same value.**
  `create_vault_folder` sends `folderName` + optional `parentPath` (not
  `folderPath`) — the two are combined into the eventual folder path only
  when a parent is present.

**Standing rule:** every server test added for a web-routed command sends
the *real* web argument shape (snake_case, real key names, real envelope
nesting — verified against the actual `mockInvoke`/fetch call site in the
React source, not assumed from the Tauri command's Rust signature). A test
that only exercises the camelCase/Tauri shape does not prove the web path
works, because the two shapes are handled by different branches of the same
extractor. Any future command added to `SERVER_COMMANDS` must go through
this same casing/key-tolerant extraction and be tested with the real web
shape, not just the desktop shape.

### `DESKTOP_ONLY`: an explicit allowlist, not a blanket fallback

`src/web/mockBridge.ts` defines a `DESKTOP_ONLY` set — clipboard
(`copy_text_to_clipboard`, `read_text_from_clipboard`), image/PDF
(`copy_image_to_vault`, `save_image`, `export_current_webview_pdf`,
`print_current_webview`, `can_export_current_webview_pdf`), window/menu
chrome (`update_current_window_min_size`,
`perform_current_window_titlebar_double_click`, `trigger_menu_command`,
`update_menu_state`), the updater (`check_for_app_update`,
`download_and_install_app_update`), the vault file watcher
(`start_vault_watcher`, `stop_vault_watcher`), and AI
streaming/session/credential commands (`stream_ai_agent`,
`stream_ai_model`, `stream_claude_chat`, `get_ai_workspace_sessions`,
`save_ai_workspace_sessions`, `save_ai_model_provider_api_key`,
`delete_ai_model_provider_api_key`, `test_ai_model_provider`,
`check_claude_cli`, `get_agent_docs_path`, plus a handful of
process/window/icon/media-preview helpers). These commands short-circuit to
`Promise.resolve(undefined)` in `mockInvoke` instead of falling through to
the HTTP transport, because there is nothing on the server for them to do —
a browser tab has no OS clipboard-equivalent worth wiring, no local window
chrome, no local AI CLI subprocess to stream from.

This was deliberately built as an **explicit, named allowlist** rather than
"any command not in `SERVER_COMMANDS` and not in `mockHandlers` resolves to
`undefined`." A blanket fallback would silently swallow a command that is
genuinely missing from the server by mistake (a bug) in exactly the same
way it swallows a command that is genuinely desktop-only (by design) — the
two cases are indistinguishable to the caller, which is precisely the
"fake data instead of a loud failure" problem this whole task set out to
fix. With an explicit list, an unrecognized command falls through to
`invoke()` and the server's real 501, which resolves to `undefined` but at
least travels over the network and shows up in a DevTools/log audit as a
miss — an actual gap surfaces instead of being silently indistinguishable
from an intentional no-op.

### Knowingly deferred (still on the mock)

The following commands remain on the in-memory mock, by choice, not
oversight:

- `auto_rename_untitled`, `detect_renames`, `update_wikilinks_for_renames`
  — desktop-only heuristics around reconciling filesystem-level renames
  made outside the app (e.g. in a file manager or another git client) with
  in-app wikilinks. The web client has no equivalent out-of-band rename
  vector to reconcile.
- `batch_delete_notes_async` — the desktop batch-delete path. Web delete
  already goes through per-path `delete_note` (on `SERVER_COMMANDS`) one at
  a time; a batch variant is a performance optimization, not a capability
  gap, and is deferred until it's shown to matter for the web client's
  usage patterns.
- `validate_note_content` — deferred pending a decision on where content
  validation should live for the web path; not required for the git-reads/
  folder-ops/no-op scope of this task.

## Consequences

- The Changes panel, a note's diff/history view, the Pulse view, and Add
  Remote now reflect the real vault's git state and real remote
  configuration in the browser, closing the "looks like it works but shows
  fake data" gap that motivated this task.
- Folder create/rename/delete from the web client now mutate the real vault
  on disk and are captured by an autogit commit, consistent with note-level
  writes under ADR-0149/ADR-0151.
- The arg-shape rule is now a documented, enforced pattern (casing-tolerant
  extractors, web-shape-first tests) rather than something each command
  handler reinvented ad hoc; any future web-routed command should follow
  the same extractor pattern and test discipline.
- A genuinely missing server command still surfaces as a network 501 rather
  than being indistinguishable from an intentional `DESKTOP_ONLY` no-op,
  because the no-op set is an explicit allowlist rather than a blanket
  fallback.
- **Follow-up noted at review (not blocking, tracked here for a future
  pass):**
  - The git read commands' file-path handling (`vault_file_path` /
    `file_path_arg` in `handlers.rs`) does not lexically canonicalize the
    client-supplied path the way `contained_note_path` does for
    read/write-vault commands; containment for these paths currently
    relies on `git`'s own pathspec resolution rejecting paths outside the
    repository, rather than an explicit canonicalize-and-`starts_with`
    check performed before the path reaches `tolaria_core::git`. This has
    not been shown to be exploitable (git itself refuses to operate on
    pathspecs outside its working tree), but it is an inconsistency with
    the containment strategy used elsewhere and is worth tightening if
    these handlers are extended further.
  - `str_arg` (the casing-tolerant string extractor) is duplicated verbatim
    across `git_handlers.rs` and `write_handlers.rs` (and mirrors
    `handlers::arg_str`). A DRY cleanup — hoisting one shared
    `pub(crate)` implementation — was considered and deferred as a pure
    refactor with no behavior change; it does not block this task and can
    land independently.

Neither follow-up changes external behavior, so both are recorded here
rather than blocking this ADR.
