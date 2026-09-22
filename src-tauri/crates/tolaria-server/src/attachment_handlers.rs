//! Attachment write commands for the web server.
//!
//! Renaming an attachment moves the file and rewrites every note that
//! references it, so it goes through the same containment check, per-path
//! lock, and autogit commit-all path as note renames.

use crate::handlers::contained_note_path;
use crate::rpc::{AppState, RpcError};
use crate::write_handlers::autogit_commit_all;
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;
use tolaria_core::git::CommitIdentity;
use tolaria_core::vault::{self, AttachmentRenameRequest};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenameAttachmentArgs {
    #[serde(alias = "source_path")]
    source_path: PathBuf,
    #[serde(alias = "requested_name")]
    requested_name: String,
}

/// Rename an attachment inside the vault and rewrite references to it. The
/// vault root always comes from server state, never the client-supplied
/// `vaultPath`, and the source must resolve inside it.
pub(crate) async fn rename_attachment(
    state: &AppState,
    identity: Option<&CommitIdentity>,
    args: Value,
) -> Result<Value, RpcError> {
    let a: RenameAttachmentArgs = serde_json::from_value(args)
        .map_err(|e| RpcError::bad_request(format!("bad arguments: {e}")))?;
    let safe_source = contained_note_path(&state.vault_root, &a.source_path)?;
    let _guard = state.locks.lock(&safe_source).await;
    let vault_root = state.vault_root.to_string_lossy();
    let result = vault::rename_attachment(AttachmentRenameRequest {
        requested_name: &a.requested_name,
        source_path: &safe_source.to_string_lossy(),
        vault_path: &vault_root,
    })
    .map_err(RpcError::bad_request)?;
    // Reference rewrites touch an unknown set of notes (the result only
    // reports a count), so commit everything dirty, as note renames do.
    let message = format!("rename attachment to {}", result.new_name);
    autogit_commit_all(state, identity, &message).await;
    serde_json::to_value(result).map_err(|e| RpcError::internal(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use serde_json::json;
    use std::fs;
    use std::path::Path;
    use std::process::Command;
    use tempfile::tempdir;

    fn state_for(dir: &Path) -> AppState {
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

    fn git(vault: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(vault)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn vault_with_image(dir: &Path) -> std::path::PathBuf {
        let attachments = dir.join("attachments");
        fs::create_dir_all(&attachments).unwrap();
        let image = attachments.join("old.png");
        fs::write(&image, b"png").unwrap();
        fs::write(dir.join("note.md"), "# Note\n\n![](attachments/old.png)\n").unwrap();
        image
    }

    fn identity() -> CommitIdentity {
        CommitIdentity {
            author_name: "Frank F".into(),
            author_email: "frank@example.com".into(),
            committer_name: "Frank F".into(),
            committer_email: "frank@example.com".into(),
        }
    }

    #[tokio::test]
    async fn renames_file_and_rewrites_note_references() {
        let dir = tempdir().unwrap();
        let image = vault_with_image(dir.path());
        let out = rename_attachment(
            &state_for(dir.path()),
            None,
            json!({ "sourcePath": image, "requestedName": "diagram", "vaultPath": "/ignored" }),
        )
        .await
        .unwrap();

        assert_eq!(out["newName"], json!("diagram.png"));
        assert_eq!(out["updatedFiles"], json!(1));
        assert!(!image.exists());
        assert!(dir.path().join("attachments/diagram.png").exists());
        let note = fs::read_to_string(dir.path().join("note.md")).unwrap();
        assert!(note.contains("attachments/diagram.png"), "got: {note}");
    }

    #[tokio::test]
    async fn rejects_source_outside_vault() {
        let dir = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let image = vault_with_image(outside.path());
        let err = rename_attachment(
            &state_for(dir.path()),
            None,
            json!({ "sourcePath": image, "requestedName": "evil" }),
        )
        .await
        .unwrap_err();

        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert!(image.exists(), "file outside the vault must not move");
    }

    #[tokio::test]
    async fn rejects_missing_arguments() {
        let dir = tempdir().unwrap();
        let err = rename_attachment(&state_for(dir.path()), None, json!({}))
            .await
            .unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn autogit_commits_the_rename_as_the_acting_user() {
        let dir = tempdir().unwrap();
        let vault = dir.path();
        let image = vault_with_image(vault);
        git(vault, &["init"]);
        git(vault, &["config", "user.email", "s@t"]);
        git(vault, &["config", "user.name", "S"]);
        git(vault, &["add", "-A"]);
        git(vault, &["commit", "-m", "init"]);

        rename_attachment(
            &state_for(vault),
            Some(&identity()),
            json!({ "source_path": image, "requested_name": "diagram" }),
        )
        .await
        .unwrap();

        let line = git(vault, &["log", "-1", "--format=%an <%ae>|%s"]);
        assert_eq!(
            line,
            "Frank F <frank@example.com>|rename attachment to diagram.png"
        );
        assert_eq!(git(vault, &["status", "--porcelain"]), "");
    }
}
