//! Write-command dispatch for the read-write web server.
//!
//! Unlike `handlers::dispatch` (synchronous, read-only), write commands go
//! through this async dispatcher so they can be serialized per-path via
//! `AppState::locks` and, for saves, enforce optimistic concurrency against
//! a client-supplied content hash (`baseHash`).

use crate::handlers::contained_note_path;
use crate::rpc::{AppState, RpcError};
use crate::version::content_version;
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use tolaria_core::frontmatter::{self, FrontmatterValue};
use tolaria_core::vault::{self, RenameNoteFilenameRequest, RenameNoteRequest};

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
pub async fn dispatch_write(
    state: &AppState,
    command: &str,
    args: Value,
) -> Result<Value, RpcError> {
    match command {
        "save_note_content" => save_note_content(state, args).await,
        "create_note_content" | "create_note" => create_note_content(state, args).await,
        "delete_note" => delete_note(state, args).await,
        "rename_note" => rename_note(state, args).await,
        "rename_note_filename" => rename_note_filename(state, args).await,
        "update_frontmatter" => update_frontmatter(state, args).await,
        "delete_frontmatter_property" => delete_frontmatter_property(state, args).await,
        other => Err(crate::rpc::unsupported(other)),
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
async fn save_note_content(state: &AppState, args: Value) -> Result<Value, RpcError> {
    let a: SaveArgs = parse(args)?;
    let safe = contained_note_path_for_write(&state.vault_root, &a.path)?;
    let _guard = state.locks.lock(&safe).await;

    if let Some(base) = &a.base_hash {
        if let Some(current) = stale_conflict(&safe, base)? {
            return Err(conflict_error(&current));
        }
    }

    let path_str = safe.to_string_lossy().to_string();
    vault::save_note_content(&path_str, &a.content).map_err(RpcError::internal)?;
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
async fn create_note_content(state: &AppState, args: Value) -> Result<Value, RpcError> {
    let a: CreateArgs = parse(args)?;
    let safe = contained_note_path_for_write(&state.vault_root, &a.path)?;
    let _guard = state.locks.lock(&safe).await;
    vault::create_note_content(&safe.to_string_lossy(), &a.content).map_err(RpcError::internal)?;
    Ok(json!({ "path": safe.to_string_lossy(), "version": content_version(&a.content) }))
}

#[derive(Deserialize)]
struct DeleteArgs {
    path: PathBuf,
}

/// Permanently delete an existing note.
async fn delete_note(state: &AppState, args: Value) -> Result<Value, RpcError> {
    let a: DeleteArgs = parse(args)?;
    let safe = contained_note_path(&state.vault_root, &a.path)?;
    let _guard = state.locks.lock(&safe).await;
    let removed = vault::delete_note(&safe.to_string_lossy()).map_err(RpcError::internal)?;
    Ok(json!(removed))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenameArgs {
    old_path: PathBuf,
    new_title: String,
    old_title_hint: Option<String>,
}

/// Rename a note's title (and, if needed, its filename slug), rewriting
/// wikilinks in other notes that referenced it. The vault root always comes
/// from server state, never the client-supplied `vaultPath`.
async fn rename_note(state: &AppState, args: Value) -> Result<Value, RpcError> {
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
    serde_json::to_value(result).map_err(|e| RpcError::internal(e.to_string()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenameFilenameArgs {
    old_path: PathBuf,
    new_filename_stem: String,
}

/// Rename only a note's filename (stem), leaving its title untouched,
/// rewriting wikilinks in other notes that referenced its old filename.
async fn rename_note_filename(state: &AppState, args: Value) -> Result<Value, RpcError> {
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
    serde_json::to_value(result).map_err(|e| RpcError::internal(e.to_string()))
}

#[derive(Deserialize)]
struct UpdateFrontmatterArgs {
    path: PathBuf,
    key: String,
    value: FrontmatterValue,
}

/// Set (or overwrite) a single frontmatter property on an existing note.
async fn update_frontmatter(state: &AppState, args: Value) -> Result<Value, RpcError> {
    let a: UpdateFrontmatterArgs = parse(args)?;
    let safe = contained_note_path(&state.vault_root, &a.path)?;
    let _guard = state.locks.lock(&safe).await;
    let content = frontmatter::update_frontmatter(&safe.to_string_lossy(), &a.key, a.value)
        .map_err(RpcError::internal)?;
    Ok(json!(content))
}

#[derive(Deserialize)]
struct DeleteFrontmatterArgs {
    path: PathBuf,
    key: String,
}

/// Remove a single frontmatter property from an existing note.
async fn delete_frontmatter_property(state: &AppState, args: Value) -> Result<Value, RpcError> {
    let a: DeleteFrontmatterArgs = parse(args)?;
    let safe = contained_note_path(&state.vault_root, &a.path)?;
    let _guard = state.locks.lock(&safe).await;
    let content = frontmatter::delete_frontmatter_property(&safe.to_string_lossy(), &a.key)
        .map_err(RpcError::internal)?;
    Ok(json!(content))
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
            false,
            crate::locks::PathLocks::new(),
        )
    }

    #[tokio::test]
    async fn save_without_base_hash_writes_and_returns_version() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("n.md");
        fs::write(&p, "old").unwrap();
        let out = save_note_content(
            &state_for(dir.path()),
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
        let err = save_note_content(&state_for(dir.path()), json!({ "path": p, "content": "x" }))
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
        let out = save_note_content(&state, json!({ "path": b, "content": "b-new" }))
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
        let out = dispatch_write(&state, "delete_note", json!({ "path": p }))
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
        let err = dispatch_write(&state, "delete_note", json!({ "path": p }))
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

    #[tokio::test]
    async fn update_frontmatter_sets_key() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("fm.md");
        fs::write(&p, "# Title\n\nbody").unwrap();
        let state = state_for(dir.path());
        dispatch_write(
            &state,
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
            "delete_frontmatter_property",
            json!({ "path": p, "key": "status" }),
        )
        .await
        .unwrap();
        let content = fs::read_to_string(&p).unwrap();
        assert!(!content.contains("status: done"));
    }
}
