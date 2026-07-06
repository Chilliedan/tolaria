//! Repo-locked dispatch for index-mutating git commands (commit/push/pull/
//! conflict resolution). Serialized by `AppState::repo_lock` so git's index is
//! never mutated concurrently. Commits are authored by the acting user.

use crate::rpc::{AppState, RpcError};
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
            let hash =
                git_commit_all_as(&vault, &a.message, identity).map_err(RpcError::internal)?;
            Ok(json!({ "hash": hash }))
        }
        "git_push" => {
            let r = git_push(&vault).map_err(RpcError::internal)?;
            serde_json::to_value(r).map_err(|e| RpcError::internal(e.to_string()))
        }
        "git_pull" => {
            let r = git_pull(&vault).map_err(RpcError::internal)?;
            serde_json::to_value(r).map_err(|e| RpcError::internal(e.to_string()))
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
    use crate::rpc::AppStateConfig;
    use crate::session::SessionStore;
    use crate::users::UsersDb;
    use std::time::Duration;

    fn repo_state(vault: &std::path::Path) -> AppState {
        for args in [
            ["init"].as_slice(),
            ["config", "user.email", "seed@t"].as_slice(),
            ["config", "user.name", "Seed"].as_slice(),
        ] {
            std::process::Command::new("git")
                .args(args)
                .current_dir(vault)
                .output()
                .unwrap();
        }
        AppState::new(
            vault.to_path_buf(),
            UsersDb::open_in_memory().unwrap(),
            SessionStore::new(Duration::from_secs(60)),
            PathLocks::new(),
            AppStateConfig {
                cookie_secure: false,
                committer_name: "Tolaria Server".to_string(),
                committer_email: "server@tolaria.local".to_string(),
                autogit: true,
            },
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
        assert_eq!(
            String::from_utf8_lossy(&author.stdout).trim(),
            "Eve E <eve@example.com>"
        );
    }

    #[tokio::test]
    async fn git_push_on_repo_without_remote_returns_status() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path();
        let state = repo_state(vault);

        let out = dispatch_git(&state, &eve(), "git_push", json!({}))
            .await
            .unwrap();
        assert_eq!(out["status"], json!("no_remote"));
    }

    #[tokio::test]
    async fn git_pull_on_repo_without_remote_returns_status() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path();
        let state = repo_state(vault);

        let out = dispatch_git(&state, &eve(), "git_pull", json!({}))
            .await
            .unwrap();
        assert_eq!(out["status"], json!("no_remote"));
    }

    #[tokio::test]
    async fn git_resolve_conflict_missing_args_is_bad_request() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path();
        let state = repo_state(vault);

        let err = dispatch_git(&state, &eve(), "git_resolve_conflict", json!({}))
            .await
            .unwrap_err();
        assert_eq!(err.status, axum::http::StatusCode::BAD_REQUEST);
    }
}
