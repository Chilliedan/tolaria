//! Write-command dispatch for the read-write web server.
//!
//! Unlike `handlers::dispatch` (synchronous, read-only), write commands go
//! through this async dispatcher so they can be serialized per-path via
//! `AppState::locks` and, for saves, enforce optimistic concurrency against
//! a client-supplied content hash (`baseHash`).

use crate::rpc::{AppState, RpcError};
use crate::version::content_version;
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use tolaria_core::vault;

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
        // other write arms added in Task 3
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
}
