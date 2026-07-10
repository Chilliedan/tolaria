//! Write-command dispatch for the read-write web server.
//!
//! Unlike `handlers::dispatch` (synchronous, read-only), write commands go
//! through this async dispatcher so they can be serialized per-path via
//! `AppState::locks` and, for saves, enforce optimistic concurrency against
//! a client-supplied content hash (`baseHash`).

use crate::handlers::contained_note_path;
use crate::rpc::{str_arg, AppState, RpcError};
use crate::version::content_version;
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use tolaria_core::frontmatter::{self, FrontmatterValue};
use tolaria_core::vault::{
    self, MoveNoteToFolderRequest, RenameNoteFilenameRequest, RenameNoteRequest, ViewDefinition,
};

/// True if `command` is a write command that must go through
/// [`dispatch_write`] instead of the read-only `handlers::dispatch`.
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
            | "create_vault_folder"
            | "delete_vault_folder"
            | "rename_vault_folder"
            | "move_note_to_folder"
            | "save_view_cmd"
            | "delete_view_cmd"
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

/// Dispatch a write command name + JSON args to its async handler.
///
/// `identity` is the acting user's commit identity, resolved from the
/// session cookie by the caller. When present and `state.autogit` is
/// enabled, the write is followed by an autogit commit authored by that
/// user; when `None` (e.g. internal callers, unauthenticated defensive
/// paths, or tests asserting file effects only), no commit is made.
pub async fn dispatch_write(
    state: &AppState,
    identity: Option<&tolaria_core::git::CommitIdentity>,
    command: &str,
    args: Value,
) -> Result<Value, RpcError> {
    match command {
        "save_note_content" => save_note_content(state, identity, args).await,
        "create_note_content" | "create_note" => create_note_content(state, identity, args).await,
        "delete_note" => delete_note(state, identity, args).await,
        "rename_note" => rename_note(state, identity, args).await,
        "rename_note_filename" => rename_note_filename(state, identity, args).await,
        "update_frontmatter" => update_frontmatter(state, identity, args).await,
        "delete_frontmatter_property" => delete_frontmatter_property(state, identity, args).await,
        "create_vault_folder" => create_vault_folder(state, identity, args).await,
        "delete_vault_folder" => delete_vault_folder(state, identity, args).await,
        "rename_vault_folder" => rename_vault_folder(state, identity, args).await,
        "move_note_to_folder" => move_note_to_folder(state, identity, args).await,
        "save_view_cmd" => save_view_command(state, identity, args).await,
        "delete_view_cmd" => delete_view_command(state, identity, args).await,
        other => Err(crate::rpc::unsupported(other)),
    }
}

/// The vault-relative path used for `git add`, falling back to the absolute
/// path string if it is not under the vault root (should not happen — writes
/// are containment-checked).
fn rel_path(state: &AppState, safe: &std::path::Path) -> String {
    safe.strip_prefix(state.vault_root.as_ref())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| safe.to_string_lossy().to_string())
}

fn file_label(safe: &std::path::Path) -> String {
    safe.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Commit exactly `rel_paths` as `identity`, when autogit is enabled. Used
/// for single-file writes (save/create/delete/frontmatter) where the set of
/// changed files is known precisely.
///
/// No-ops when `identity` is `None` or `state.autogit` is `false`. A commit
/// failure is logged but never fails the write — the file is already safely
/// persisted to disk. Runs under the repo lock, which is intentionally
/// acquired while the caller's per-path `_guard` is still held (the guard is
/// held until the caller returns) — a known throughput trade-off documented
/// in ADR 0151. Lock acquisition order is always per-path-lock → repo-lock,
/// never the reverse, so this cannot deadlock.
async fn autogit_commit_paths(
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

/// Commit ALL working-tree changes as `identity`, when autogit is enabled.
/// Used for rename handlers: renaming rewrites wikilinks across an unknown
/// set of other notes (the result only reports a count via `updated_files`,
/// not their paths), so a path-scoped commit would miss them.
///
/// Same no-op / failure-logging / lock-ordering contract as
/// [`autogit_commit_paths`]: the repo lock is intentionally acquired while
/// the caller's per-path `_guard` is still held (a known throughput
/// trade-off documented in ADR 0151); acquisition order is always
/// per-path-lock → repo-lock, never the reverse, so this cannot deadlock.
async fn autogit_commit_all(
    state: &AppState,
    identity: Option<&tolaria_core::git::CommitIdentity>,
    message: &str,
) {
    let Some(identity) = identity else { return };
    if !state.autogit {
        return;
    }
    let _repo = state.repo_lock.lock().await;
    let vault = state.vault_root.to_string_lossy().to_string();
    if let Err(e) = tolaria_core::git::git_commit_all_as(&vault, message, identity) {
        eprintln!("autogit commit-all failed: {e}");
    }
}

/// Build the `409` response body for a stale `baseHash`: the caller's edit is
/// rejected and the current on-disk content is returned so the client can
/// reconcile.
fn conflict_error(current_content: &str) -> RpcError {
    RpcError {
        status: StatusCode::CONFLICT,
        message: serde_json::to_string(&json!({
            "error": "conflict",
            "currentContent": current_content,
        }))
        .unwrap_or_else(|_| "{\"error\":\"conflict\"}".to_string()),
    }
}

/// Returns `Some(current_content)` if `safe` exists and its content hash no
/// longer matches `base`, i.e. the client's edit is based on stale content.
fn stale_conflict(safe: &std::path::Path, base: &str) -> Result<Option<String>, RpcError> {
    if !safe.exists() {
        return Ok(None);
    }
    let current = vault::get_note_content(safe).map_err(RpcError::internal)?;
    if content_version(&current) != base {
        Ok(Some(current))
    } else {
        Ok(None)
    }
}

/// Optimistic save: under the per-path lock, reject with `409` if the file
/// exists and `baseHash` no longer matches the on-disk content; otherwise
/// write the new content and return its version hash.
async fn save_note_content(
    state: &AppState,
    identity: Option<&tolaria_core::git::CommitIdentity>,
    args: Value,
) -> Result<Value, RpcError> {
    let a: SaveArgs = parse(args)?;
    // When the file already exists, resolve it the same way the other
    // existing-file arms do (fully canonicalized) so a concurrent save +
    // frontmatter/delete on the same file take the same lock and serialize.
    // Fall back to the parent-confined path for a first save (file absent).
    let safe = match contained_note_path(&state.vault_root, &a.path) {
        Ok(existing) => existing,
        Err(_) => contained_note_path_for_write(&state.vault_root, &a.path)?,
    };
    let _guard = state.locks.lock(&safe).await;

    if let Some(base) = &a.base_hash {
        if let Some(current) = stale_conflict(&safe, base)? {
            return Err(conflict_error(&current));
        }
    }

    let path_str = safe.to_string_lossy().to_string();
    vault::save_note_content(&path_str, &a.content).map_err(RpcError::internal)?;
    autogit_commit_paths(
        state,
        identity,
        &[rel_path(state, &safe)],
        &format!("update {}", file_label(&safe)),
    )
    .await;
    Ok(json!({ "version": content_version(&a.content) }))
}

#[derive(Deserialize)]
struct CreateArgs {
    path: PathBuf,
    #[serde(default)]
    content: String,
}

/// Create a new note at `path` (parent-confined, since the file itself does
/// not exist yet) and write its initial content.
async fn create_note_content(
    state: &AppState,
    identity: Option<&tolaria_core::git::CommitIdentity>,
    args: Value,
) -> Result<Value, RpcError> {
    let a: CreateArgs = parse(args)?;
    let safe = contained_note_path_for_write(&state.vault_root, &a.path)?;
    let _guard = state.locks.lock(&safe).await;
    vault::create_note_content(&safe.to_string_lossy(), &a.content).map_err(RpcError::internal)?;
    autogit_commit_paths(
        state,
        identity,
        &[rel_path(state, &safe)],
        &format!("create {}", file_label(&safe)),
    )
    .await;
    Ok(json!({ "path": safe.to_string_lossy(), "version": content_version(&a.content) }))
}

#[derive(Deserialize)]
struct DeleteArgs {
    path: PathBuf,
}

/// Permanently delete an existing note.
async fn delete_note(
    state: &AppState,
    identity: Option<&tolaria_core::git::CommitIdentity>,
    args: Value,
) -> Result<Value, RpcError> {
    let a: DeleteArgs = parse(args)?;
    let safe = contained_note_path(&state.vault_root, &a.path)?;
    let _guard = state.locks.lock(&safe).await;
    let removed = vault::delete_note(&safe.to_string_lossy()).map_err(RpcError::internal)?;
    autogit_commit_paths(
        state,
        identity,
        &[rel_path(state, &safe)],
        &format!("delete {}", file_label(&safe)),
    )
    .await;
    Ok(json!(removed))
}

// Accept both the camelCase (desktop Tauri) and snake_case (web mock path)
// field names. The web client routes rename through mockInvoke, which sends
// snake_case; without these aliases every web rename fails deserialization.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenameArgs {
    #[serde(alias = "old_path")]
    old_path: PathBuf,
    #[serde(alias = "new_title")]
    new_title: String,
    // The optional title hint arrives as `oldTitle` (Tauri) or `old_title`
    // (web) — neither matches the default `oldTitleHint`, so alias both.
    #[serde(alias = "old_title_hint", alias = "oldTitle", alias = "old_title")]
    old_title_hint: Option<String>,
}

/// Rename a note's title (and, if needed, its filename slug), rewriting
/// wikilinks in other notes that referenced it. The vault root always comes
/// from server state, never the client-supplied `vaultPath`.
async fn rename_note(
    state: &AppState,
    identity: Option<&tolaria_core::git::CommitIdentity>,
    args: Value,
) -> Result<Value, RpcError> {
    let a: RenameArgs = parse(args)?;
    let safe_old = contained_note_path(&state.vault_root, &a.old_path)?;
    let _guard = state.locks.lock(&safe_old).await;
    let vault_root = state.vault_root.to_string_lossy();
    let old_path = safe_old.to_string_lossy();
    let result = vault::rename_note(RenameNoteRequest {
        vault_path: &vault_root,
        old_path: &old_path,
        new_title: &a.new_title,
        old_title_hint: a.old_title_hint.as_deref(),
    })
    .map_err(RpcError::internal)?;
    // Renaming rewrites wikilinks across an unknown set of other notes (the
    // result only reports a count, not paths), so this must commit
    // everything currently dirty rather than a fixed path list.
    autogit_commit_all(
        state,
        identity,
        &format!("rename note to {}", a.new_title),
    )
    .await;
    serde_json::to_value(result).map_err(|e| RpcError::internal(e.to_string()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenameFilenameArgs {
    #[serde(alias = "old_path")]
    old_path: PathBuf,
    #[serde(alias = "new_filename_stem")]
    new_filename_stem: String,
}

/// Rename only a note's filename (stem), leaving its title untouched,
/// rewriting wikilinks in other notes that referenced its old filename.
async fn rename_note_filename(
    state: &AppState,
    identity: Option<&tolaria_core::git::CommitIdentity>,
    args: Value,
) -> Result<Value, RpcError> {
    let a: RenameFilenameArgs = parse(args)?;
    let safe_old = contained_note_path(&state.vault_root, &a.old_path)?;
    let _guard = state.locks.lock(&safe_old).await;
    let vault_root = state.vault_root.to_string_lossy();
    let old_path = safe_old.to_string_lossy();
    let result = vault::rename_note_filename(RenameNoteFilenameRequest {
        vault_path: &vault_root,
        old_path: &old_path,
        new_filename_stem: &a.new_filename_stem,
    })
    .map_err(RpcError::internal)?;
    // Same reasoning as `rename_note`: wikilink rewrites touch an unknown
    // set of other notes, so commit everything dirty rather than a fixed
    // path list.
    autogit_commit_all(
        state,
        identity,
        &format!("rename note file to {}", a.new_filename_stem),
    )
    .await;
    serde_json::to_value(result).map_err(|e| RpcError::internal(e.to_string()))
}

#[derive(Deserialize)]
struct UpdateFrontmatterArgs {
    path: PathBuf,
    key: String,
    value: FrontmatterValue,
}

/// Set (or overwrite) a single frontmatter property on an existing note.
async fn update_frontmatter(
    state: &AppState,
    identity: Option<&tolaria_core::git::CommitIdentity>,
    args: Value,
) -> Result<Value, RpcError> {
    let a: UpdateFrontmatterArgs = parse(args)?;
    let safe = contained_note_path(&state.vault_root, &a.path)?;
    let _guard = state.locks.lock(&safe).await;
    let content = frontmatter::update_frontmatter(&safe.to_string_lossy(), &a.key, a.value)
        .map_err(RpcError::internal)?;
    autogit_commit_paths(
        state,
        identity,
        &[rel_path(state, &safe)],
        &format!("update frontmatter on {}", file_label(&safe)),
    )
    .await;
    Ok(json!(content))
}

#[derive(Deserialize)]
struct DeleteFrontmatterArgs {
    path: PathBuf,
    key: String,
}

/// Remove a single frontmatter property from an existing note.
async fn delete_frontmatter_property(
    state: &AppState,
    identity: Option<&tolaria_core::git::CommitIdentity>,
    args: Value,
) -> Result<Value, RpcError> {
    let a: DeleteFrontmatterArgs = parse(args)?;
    let safe = contained_note_path(&state.vault_root, &a.path)?;
    let _guard = state.locks.lock(&safe).await;
    let content = frontmatter::delete_frontmatter_property(&safe.to_string_lossy(), &a.key)
        .map_err(RpcError::internal)?;
    autogit_commit_paths(
        state,
        identity,
        &[rel_path(state, &safe)],
        &format!("delete frontmatter property on {}", file_label(&safe)),
    )
    .await;
    Ok(json!(content))
}

/// Create a folder under the vault root. The web client sends `folderName`
/// and an optional `parentPath` (App.tsx); when a parent is given the new
/// folder is nested under it. Folder ops touch an unknown set of paths (a
/// nested `mkdir -p`), so this commits everything dirty rather than a fixed
/// path list, matching the rename handlers' reasoning.
async fn create_vault_folder(
    state: &AppState,
    identity: Option<&tolaria_core::git::CommitIdentity>,
    args: Value,
) -> Result<Value, RpcError> {
    let name = str_arg(&args, "folderName", "folder_name")?;
    let parent = str_arg(&args, "parentPath", "parent_path")
        .ok()
        .filter(|s| !s.is_empty());
    let folder_path = match parent {
        Some(p) => format!("{p}/{name}"),
        None => name,
    };
    let created =
        vault::create_folder(&state.vault_root, &folder_path).map_err(RpcError::internal)?;
    autogit_commit_all(state, identity, &format!("create folder {created}")).await;
    Ok(json!(created))
}

/// Delete a folder (and its contents) under the vault root. Commits
/// everything dirty since a directory removal can span many tracked files.
async fn delete_vault_folder(
    state: &AppState,
    identity: Option<&tolaria_core::git::CommitIdentity>,
    args: Value,
) -> Result<Value, RpcError> {
    let folder = str_arg(&args, "folderPath", "folder_path")?;
    let deleted = vault::delete_folder(&state.vault_root, &folder).map_err(RpcError::internal)?;
    autogit_commit_all(state, identity, &format!("delete folder {folder}")).await;
    Ok(json!(deleted))
}

/// Rename a folder under the vault root. Commits everything dirty, same
/// reasoning as `rename_note`/`rename_note_filename`.
async fn rename_vault_folder(
    state: &AppState,
    identity: Option<&tolaria_core::git::CommitIdentity>,
    args: Value,
) -> Result<Value, RpcError> {
    let folder = str_arg(&args, "folderPath", "folder_path")?;
    let new_name = str_arg(&args, "newName", "new_name")?;
    let result = vault::rename_folder(&state.vault_root, &folder, &new_name)
        .map_err(RpcError::internal)?;
    autogit_commit_all(state, identity, &format!("rename folder {folder}")).await;
    serde_json::to_value(result).map_err(|e| RpcError::internal(e.to_string()))
}

/// Move a note into an existing folder within the vault. The client sends the
/// destination as a vault-relative path (`folder_path`); core wants an absolute
/// existing directory, so resolve it under the vault and confine it.
async fn move_note_to_folder(
    state: &AppState,
    identity: Option<&tolaria_core::git::CommitIdentity>,
    args: Value,
) -> Result<Value, RpcError> {
    let old = str_arg(&args, "oldPath", "old_path")?;
    let folder = str_arg(&args, "folderPath", "folder_path")?;
    let safe_old = contained_note_path(&state.vault_root, std::path::Path::new(&old))?;
    let _guard = state.locks.lock(&safe_old).await;
    let dest = contained_existing_folder(&state.vault_root, &folder)?;
    let vault_root = state.vault_root.to_string_lossy();
    let result = vault::move_note_to_folder(MoveNoteToFolderRequest {
        vault_path: &vault_root,
        old_path: &safe_old.to_string_lossy(),
        destination_folder_path: &dest.to_string_lossy(),
    })
    .map_err(RpcError::internal)?;
    autogit_commit_all(state, identity, "move note to folder").await;
    serde_json::to_value(result).map_err(|e| RpcError::internal(e.to_string()))
}

#[derive(Deserialize)]
struct SaveViewArgs {
    filename: String,
    definition: ViewDefinition,
}

/// Create/overwrite a saved view (`<vault>/views/<filename>.yml`).
async fn save_view_command(
    state: &AppState,
    identity: Option<&tolaria_core::git::CommitIdentity>,
    args: Value,
) -> Result<Value, RpcError> {
    let a: SaveViewArgs = parse(args)?;
    validate_view_filename(&a.filename)?;
    vault::save_view(&state.vault_root, &a.filename, &a.definition).map_err(RpcError::internal)?;
    autogit_commit_all(state, identity, &format!("save view {}", a.filename)).await;
    Ok(Value::Null)
}

/// Delete a saved view file.
async fn delete_view_command(
    state: &AppState,
    identity: Option<&tolaria_core::git::CommitIdentity>,
    args: Value,
) -> Result<Value, RpcError> {
    let filename = str_arg(&args, "filename", "filename")?;
    validate_view_filename(&filename)?;
    vault::delete_view(&state.vault_root, &filename).map_err(RpcError::internal)?;
    autogit_commit_all(state, identity, &format!("delete view {}", filename)).await;
    Ok(Value::Null)
}

/// A view filename must be a bare name (no directory part, no `..`) so it can't
/// escape `<vault>/views/`. `save_view`/`delete_view` join it directly.
fn validate_view_filename(filename: &str) -> Result<(), RpcError> {
    let bare = std::path::Path::new(filename)
        .file_name()
        .map(|n| n == std::ffi::OsStr::new(filename))
        .unwrap_or(false);
    if !bare {
        return Err(RpcError::bad_request("invalid view filename"));
    }
    Ok(())
}

/// Resolve a vault-relative (or absolute-in-vault) folder path to a confined,
/// existing directory, rejecting anything outside the vault.
fn contained_existing_folder(
    vault_root: &std::path::Path,
    folder: &str,
) -> Result<PathBuf, RpcError> {
    let candidate = if std::path::Path::new(folder).is_absolute() {
        PathBuf::from(folder)
    } else {
        vault_root.join(folder)
    };
    let canon = std::fs::canonicalize(&candidate)
        .map_err(|_| RpcError::bad_request("destination folder does not exist"))?;
    let root = std::fs::canonicalize(vault_root).map_err(|e| RpcError::internal(e.to_string()))?;
    if !canon.starts_with(&root) {
        return Err(RpcError::bad_request("destination folder is outside the vault"));
    }
    if !canon.is_dir() {
        return Err(RpcError::bad_request("destination is not a folder"));
    }
    Ok(canon)
}

/// For writes the file may not exist yet (create/first save), so canonicalize
/// the PARENT and confine, rather than requiring the file itself to exist.
///
/// The read path keeps using `handlers::contained_note_path`, which requires
/// the file to already exist.
pub(crate) fn contained_note_path_for_write(
    vault_root: &std::path::Path,
    requested: &std::path::Path,
) -> Result<PathBuf, RpcError> {
    let root = std::fs::canonicalize(vault_root)
        .map_err(|e| RpcError::internal(format!("vault root error: {e}")))?;
    let parent = requested.parent().unwrap_or(requested);
    let canon_parent = std::fs::canonicalize(parent)
        .map_err(|_| RpcError::bad_request("path parent is invalid"))?;
    if !canon_parent.starts_with(&root) {
        return Err(RpcError::bad_request("path is outside the vault"));
    }
    let file_name = requested
        .file_name()
        .ok_or_else(|| RpcError::bad_request("invalid file name"))?;
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
            crate::users::UsersDb::open_in_memory().unwrap(),
            crate::session::SessionStore::new(std::time::Duration::from_secs(60)),
            crate::locks::PathLocks::new(),
            crate::rpc::AppStateConfig {
                cookie_secure: false,
                autogit: true,
            },
        )
    }

    #[tokio::test]
    async fn save_without_base_hash_writes_and_returns_version() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("n.md");
        fs::write(&p, "old").unwrap();
        let out = save_note_content(
            &state_for(dir.path()),
            None,
            json!({ "path": p, "content": "new" }),
        )
        .await
        .unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "new");
        assert_eq!(out["version"], json!(content_version("new")));
    }

    #[tokio::test]
    async fn save_with_matching_base_hash_succeeds() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("n.md");
        fs::write(&p, "old").unwrap();
        let base = content_version("old");
        let out = save_note_content(
            &state_for(dir.path()),
            None,
            json!({ "path": p, "content": "new", "baseHash": base }),
        )
        .await
        .unwrap();
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
            None,
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
        let err = save_note_content(
            &state_for(dir.path()),
            None,
            json!({ "path": p, "content": "x" }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn different_paths_do_not_block_each_other() {
        let dir = tempdir().unwrap();
        let a = dir.path().join("a.md");
        let b = dir.path().join("b.md");
        fs::write(&a, "a-old").unwrap();
        fs::write(&b, "b-old").unwrap();
        let state = state_for(dir.path());

        // Hold a's lock manually to simulate an in-flight write, and confirm
        // a concurrent write to b is not blocked by it.
        let guard = state.locks.lock(&fs::canonicalize(&a).unwrap()).await;
        let out = save_note_content(&state, None, json!({ "path": b, "content": "b-new" }))
            .await
            .unwrap();
        assert_eq!(fs::read_to_string(&b).unwrap(), "b-new");
        assert_eq!(out["version"], json!(content_version("b-new")));
        drop(guard);
    }

    #[tokio::test]
    async fn create_note_content_writes_new_file() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("new-note.md");
        let state = state_for(dir.path());
        let out = dispatch_write(
            &state,
            None,
            "create_note_content",
            json!({ "path": p, "content": "hello" }),
        )
        .await
        .unwrap();
        assert!(p.exists());
        assert_eq!(fs::read_to_string(&p).unwrap(), "hello");
        assert_eq!(out["version"], json!(content_version("hello")));
    }

    #[tokio::test]
    async fn create_note_outside_vault_rejected() {
        let dir = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let p = outside.path().join("evil.md");
        let state = state_for(dir.path());
        let err = dispatch_write(
            &state,
            None,
            "create_note_content",
            json!({ "path": p, "content": "x" }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert!(!p.exists());
    }

    #[tokio::test]
    async fn delete_note_removes_file() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("gone.md");
        fs::write(&p, "bye").unwrap();
        let canon = fs::canonicalize(&p).unwrap();
        let state = state_for(dir.path());
        let out = dispatch_write(&state, None, "delete_note", json!({ "path": p }))
            .await
            .unwrap();
        assert!(!canon.exists());
        assert_eq!(out, json!(canon.to_string_lossy()));
    }

    #[tokio::test]
    async fn delete_note_missing_file_errors() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("missing.md");
        let state = state_for(dir.path());
        let err = dispatch_write(&state, None, "delete_note", json!({ "path": p }))
            .await
            .unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn rename_note_moves_file_and_updates_title() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("old-title.md");
        fs::write(&p, "# Old Title\n\nbody").unwrap();
        let state = state_for(dir.path());
        let out = dispatch_write(
            &state,
            None,
            "rename_note",
            json!({ "vaultPath": dir.path(), "oldPath": p, "newTitle": "New Title" }),
        )
        .await
        .unwrap();
        let new_path = PathBuf::from(out["new_path"].as_str().unwrap());
        assert!(!p.exists());
        assert!(new_path.exists());
        assert!(fs::read_to_string(&new_path).unwrap().contains("New Title"));
    }

    #[tokio::test]
    async fn rename_note_outside_vault_rejected() {
        let dir = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let p = outside.path().join("evil.md");
        fs::write(&p, "# Evil\n").unwrap();
        let state = state_for(dir.path());
        let err = dispatch_write(
            &state,
            None,
            "rename_note",
            json!({ "vaultPath": dir.path(), "oldPath": p, "newTitle": "New Title" }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn rename_note_filename_changes_stem() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("old-stem.md");
        fs::write(&p, "# Old Stem\n\nbody").unwrap();
        let state = state_for(dir.path());
        let out = dispatch_write(
            &state,
            None,
            "rename_note_filename",
            json!({ "vaultPath": dir.path(), "oldPath": p, "newFilenameStem": "new-stem" }),
        )
        .await
        .unwrap();
        let new_path = PathBuf::from(out["new_path"].as_str().unwrap());
        assert!(!p.exists());
        assert!(new_path.exists());
        assert_eq!(new_path.file_name().unwrap(), "new-stem.md");
    }

    // The web client routes rename through the mock path, which sends
    // snake_case field names (old_path / new_filename_stem / new_title /
    // old_title). The server must accept those, not only the camelCase Tauri
    // form — otherwise every web rename fails with "missing field `oldPath`".
    #[tokio::test]
    async fn rename_note_filename_accepts_snake_case_web_args() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("untitled-note-1.md");
        fs::write(&p, "# Hello\n\nbody").unwrap();
        let state = state_for(dir.path());
        let out = dispatch_write(
            &state,
            None,
            "rename_note_filename",
            json!({
                "vault_path": dir.path(),
                "old_path": p,
                "new_filename_stem": "hello"
            }),
        )
        .await
        .expect("web snake_case rename_note_filename must succeed");
        let new_path = PathBuf::from(out["new_path"].as_str().unwrap());
        assert!(!p.exists());
        assert!(new_path.exists());
        assert_eq!(new_path.file_name().unwrap(), "hello.md");
    }

    #[tokio::test]
    async fn rename_note_accepts_snake_case_web_args() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("untitled-note-2.md");
        fs::write(&p, "# Untitled\n\nbody").unwrap();
        let state = state_for(dir.path());
        let out = dispatch_write(
            &state,
            None,
            "rename_note",
            json!({
                "vault_path": dir.path(),
                "old_path": p,
                "new_title": "My Note",
                "old_title": "Untitled"
            }),
        )
        .await
        .expect("web snake_case rename_note must succeed");
        let new_path = PathBuf::from(out["new_path"].as_str().unwrap());
        assert!(!p.exists());
        assert!(new_path.exists());
        assert_eq!(new_path.file_name().unwrap(), "my-note.md");
    }

    #[tokio::test]
    async fn update_frontmatter_sets_key() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("fm.md");
        fs::write(&p, "# Title\n\nbody").unwrap();
        let state = state_for(dir.path());
        dispatch_write(
            &state,
            None,
            "update_frontmatter",
            json!({ "path": p, "key": "status", "value": "done" }),
        )
        .await
        .unwrap();
        let content = fs::read_to_string(&p).unwrap();
        assert!(content.contains("status: done"));
    }

    #[tokio::test]
    async fn update_frontmatter_outside_vault_rejected() {
        let dir = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let p = outside.path().join("evil.md");
        fs::write(&p, "# Evil\n").unwrap();
        let state = state_for(dir.path());
        let err = dispatch_write(
            &state,
            None,
            "update_frontmatter",
            json!({ "path": p, "key": "status", "value": "done" }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn delete_frontmatter_property_removes_key() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("fm2.md");
        fs::write(&p, "---\nstatus: done\n---\n# Title\n\nbody").unwrap();
        let state = state_for(dir.path());
        dispatch_write(
            &state,
            None,
            "delete_frontmatter_property",
            json!({ "path": p, "key": "status" }),
        )
        .await
        .unwrap();
        let content = fs::read_to_string(&p).unwrap();
        assert!(!content.contains("status: done"));
    }

    #[tokio::test]
    async fn save_auto_commits_the_file_as_acting_user() {
        let dir = tempdir().unwrap();
        let vault = dir.path();
        for args in [
            ["init"].as_slice(),
            ["config", "user.email", "s@t"].as_slice(),
            ["config", "user.name", "S"].as_slice(),
        ] {
            std::process::Command::new("git")
                .args(args)
                .current_dir(vault)
                .output()
                .unwrap();
        }
        let state = state_for(vault);
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
        assert!(
            line.starts_with("Frank F <frank@example.com>|"),
            "got: {line}"
        );
    }

    #[tokio::test]
    async fn delete_note_autogit_commits_the_removal() {
        let dir = tempdir().unwrap();
        let vault = dir.path();
        for args in [
            ["init"].as_slice(),
            ["config", "user.email", "s@t"].as_slice(),
            ["config", "user.name", "S"].as_slice(),
        ] {
            std::process::Command::new("git")
                .args(args)
                .current_dir(vault)
                .output()
                .unwrap();
        }
        let state = state_for(vault);
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
            "create_note_content",
            json!({ "path": p.to_string_lossy(), "content": "# Auto\n" }),
        )
        .await
        .unwrap();

        dispatch_write(
            &state,
            Some(&identity),
            "delete_note",
            json!({ "path": p.to_string_lossy() }),
        )
        .await
        .unwrap();

        let log = std::process::Command::new("git")
            .args(["log", "-1", "--format=%an <%ae>|%s"])
            .current_dir(vault)
            .output()
            .unwrap();
        let line = String::from_utf8_lossy(&log.stdout);
        assert!(
            line.starts_with("Frank F <frank@example.com>|"),
            "got: {line}"
        );

        let deleted = std::process::Command::new("git")
            .args(["log", "-1", "--diff-filter=D", "--name-only"])
            .current_dir(vault)
            .output()
            .unwrap();
        let deleted_out = String::from_utf8_lossy(&deleted.stdout);
        assert!(
            deleted_out.contains("auto.md"),
            "expected auto.md to show as deleted in HEAD, got: {deleted_out}"
        );
    }

    #[tokio::test]
    async fn create_vault_folder_makes_the_dir_from_web_args() {
        let dir = tempdir().unwrap();
        let state = state_for(dir.path());
        dispatch_write(
            &state,
            None,
            "create_vault_folder",
            json!({ "vault_path": dir.path(), "folderName": "Projects" }),
        )
        .await
        .expect("web create_vault_folder must succeed");
        assert!(dir.path().join("Projects").is_dir());
    }

    #[tokio::test]
    async fn create_vault_folder_with_parent_path_nests_under_parent() {
        let dir = tempdir().unwrap();
        let state = state_for(dir.path());
        dispatch_write(
            &state,
            None,
            "create_vault_folder",
            json!({ "folderName": "sub", "parentPath": "Projects" }),
        )
        .await
        .expect("web create_vault_folder with parentPath must succeed");
        assert!(dir.path().join("Projects/sub").is_dir());
    }

    #[tokio::test]
    async fn delete_vault_folder_removes_the_dir() {
        let dir = tempdir().unwrap();
        let state = state_for(dir.path());
        dispatch_write(
            &state,
            None,
            "create_vault_folder",
            json!({ "folderName": "Projects" }),
        )
        .await
        .unwrap();

        dispatch_write(
            &state,
            None,
            "delete_vault_folder",
            json!({ "folderPath": "Projects" }),
        )
        .await
        .expect("web delete_vault_folder must succeed");

        assert!(!dir.path().join("Projects").exists());
    }

    #[tokio::test]
    async fn rename_vault_folder_renames_the_dir() {
        let dir = tempdir().unwrap();
        let state = state_for(dir.path());
        dispatch_write(
            &state,
            None,
            "create_vault_folder",
            json!({ "folderName": "Projects" }),
        )
        .await
        .unwrap();

        dispatch_write(
            &state,
            None,
            "rename_vault_folder",
            json!({ "folderPath": "Projects", "newName": "Archive" }),
        )
        .await
        .expect("web rename_vault_folder must succeed");

        assert!(dir.path().join("Archive").is_dir());
        assert!(!dir.path().join("Projects").exists());
    }

    #[tokio::test]
    async fn move_note_to_folder_moves_the_file_from_web_args() {
        let dir = tempdir().unwrap();
        let state = state_for(dir.path());
        fs::create_dir(dir.path().join("Archive")).unwrap();
        let note = dir.path().join("note.md");
        fs::write(&note, "# Note\n\nbody").unwrap();

        dispatch_write(
            &state,
            None,
            "move_note_to_folder",
            // Real web arg shape: snake_case, vault-relative destination folder.
            json!({ "vault_path": dir.path(), "old_path": note, "folder_path": "Archive" }),
        )
        .await
        .expect("web move_note_to_folder must succeed");

        assert!(!note.exists(), "original note should be gone");
        assert!(dir.path().join("Archive").join("note.md").exists(), "note should be in Archive");
    }

    #[tokio::test]
    async fn move_note_to_folder_rejects_destination_outside_vault() {
        let dir = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let state = state_for(dir.path());
        let note = dir.path().join("note.md");
        fs::write(&note, "# Note\n").unwrap();

        let err = dispatch_write(
            &state,
            None,
            "move_note_to_folder",
            json!({ "old_path": note, "folder_path": outside.path().to_string_lossy() }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert!(note.exists(), "note must not be moved out of the vault");
    }

    #[tokio::test]
    async fn save_view_cmd_writes_the_view_file() {
        let dir = tempdir().unwrap();
        let state = state_for(dir.path());
        dispatch_write(
            &state,
            None,
            "save_view_cmd",
            json!({
                "vaultPath": dir.path(),
                "filename": "active.yml",
                "definition": { "name": "Active Projects", "filters": { "all": [] } }
            }),
        )
        .await
        .expect("save_view_cmd should succeed");
        let path = dir.path().join("views").join("active.yml");
        assert!(path.exists());
        assert!(fs::read_to_string(&path).unwrap().contains("Active Projects"));
    }

    #[tokio::test]
    async fn save_view_cmd_rejects_path_traversal_filename() {
        let dir = tempdir().unwrap();
        let state = state_for(dir.path());
        let err = dispatch_write(
            &state,
            None,
            "save_view_cmd",
            json!({
                "filename": "../evil.yml",
                "definition": { "name": "X", "filters": { "all": [] } }
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert!(!dir.path().parent().unwrap().join("evil.yml").exists());
    }

    #[tokio::test]
    async fn delete_view_cmd_removes_the_view_file() {
        let dir = tempdir().unwrap();
        let state = state_for(dir.path());
        fs::create_dir(dir.path().join("views")).unwrap();
        let path = dir.path().join("views").join("gone.yml");
        fs::write(&path, "name: G\nfilters:\n  all: []\n").unwrap();
        dispatch_write(&state, None, "delete_view_cmd", json!({ "filename": "gone.yml" }))
            .await
            .expect("delete_view_cmd should succeed");
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn rename_note_does_not_autogit_when_identity_is_none() {
        let dir = tempdir().unwrap();
        let vault = dir.path();
        for args in [
            ["init"].as_slice(),
            ["config", "user.email", "s@t"].as_slice(),
            ["config", "user.name", "S"].as_slice(),
        ] {
            std::process::Command::new("git")
                .args(args)
                .current_dir(vault)
                .output()
                .unwrap();
        }
        let p = vault.join("old-title.md");
        fs::write(&p, "# Old Title\n\nbody").unwrap();
        let state = state_for(vault);
        dispatch_write(
            &state,
            None,
            "rename_note",
            json!({ "vaultPath": vault, "oldPath": p, "newTitle": "New Title" }),
        )
        .await
        .unwrap();

        let status = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(vault)
            .output()
            .unwrap();
        assert!(
            !String::from_utf8_lossy(&status.stdout).trim().is_empty(),
            "no commit should have been made without an identity"
        );
    }
}
