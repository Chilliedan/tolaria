//! Repo-locked dispatch for index-mutating git commands (commit/push/pull/
//! conflict resolution). Serialized by `AppState::repo_lock` so git's index is
//! never mutated concurrently. Commits are authored by the acting user.

use crate::rpc::{AppState, RpcError};
use serde::Deserialize;
use serde_json::{json, Value};
use tolaria_core::git::{
    discard_file_changes, git_add_remote, git_commit_all_as, git_commit_conflict_resolution,
    git_pull, git_push, git_resolve_conflict, init_repo, CommitIdentity,
};

pub fn is_git_command(command: &str) -> bool {
    matches!(
        command,
        "git_commit"
            | "git_push"
            | "git_pull"
            | "git_resolve_conflict"
            | "git_commit_conflict_resolution"
            | "git_add_remote"
            | "git_discard_file"
            | "init_git_repo"
    )
}

/// Read a required string arg, accepting either a camelCase or snake_case
/// key. Mirrors `handlers::arg_str`; kept as a small local copy so this
/// module stays self-contained.
fn str_arg(args: &Value, camel: &str, snake: &str) -> Result<String, RpcError> {
    args.get(camel)
        .or_else(|| args.get(snake))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| RpcError::bad_request(format!("missing string arg '{camel}'/'{snake}'")))
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
        "git_add_remote" => {
            // The web client nests args under `request` (AddRemoteModal.tsx);
            // fall back to top-level for other callers.
            let req = args.get("request").unwrap_or(&args);
            let url = str_arg(req, "remoteUrl", "remote_url")?;
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

    #[tokio::test]
    async fn git_add_remote_sets_the_remote() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path();
        let state = repo_state(vault); // git init'd
        std::fs::write(vault.join("note.md"), "# Note\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(vault)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "seed"])
            .current_dir(vault)
            .output()
            .unwrap();

        // Use a local bare repo as the remote so the connection succeeds
        // deterministically without depending on network access.
        let bare = tempfile::tempdir().unwrap();
        std::process::Command::new("git")
            .args(["init", "--bare"])
            .current_dir(bare.path())
            .output()
            .unwrap();
        let remote_url = bare.path().to_string_lossy().to_string();

        let out = dispatch_git(
            &state,
            &eve(),
            "git_add_remote",
            json!({ "request": { "remoteUrl": remote_url } }),
        )
        .await
        .unwrap();
        // GitAddRemoteResult serializes to an object; the remote now exists.
        assert!(out.is_object());
        assert_eq!(out["status"], json!("connected"));
        let remotes = std::process::Command::new("git")
            .args(["remote", "-v"])
            .current_dir(vault)
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&remotes.stdout).contains(&remote_url));
    }

    #[tokio::test]
    async fn git_discard_file_missing_arg_is_bad_request() {
        let dir = tempfile::tempdir().unwrap();
        let state = repo_state(dir.path());
        let err = dispatch_git(&state, &eve(), "git_discard_file", json!({}))
            .await
            .unwrap_err();
        assert_eq!(err.status, axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn git_discard_file_discards_relative_path() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path();
        let state = repo_state(vault);
        std::fs::write(vault.join("note.md"), "# Note\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(vault)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "seed"])
            .current_dir(vault)
            .output()
            .unwrap();
        std::fs::write(vault.join("note.md"), "# Note changed\n").unwrap();

        let out = dispatch_git(
            &state,
            &eve(),
            "git_discard_file",
            json!({ "vaultPath": vault.to_string_lossy(), "relativePath": "note.md" }),
        )
        .await
        .unwrap();
        assert_eq!(out, Value::Null);
        let contents = std::fs::read_to_string(vault.join("note.md")).unwrap();
        assert_eq!(contents, "# Note\n");
    }

    #[tokio::test]
    async fn init_git_repo_initializes_the_vault_root() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path();
        // Note: repo_state() already git-inits the dir; use a fresh AppState-like
        // setup on a directory that has NOT been git-initialized yet.
        let state = AppState::new(
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
        );

        let out = dispatch_git(
            &state,
            &eve(),
            "init_git_repo",
            json!({ "vaultPath": vault.to_string_lossy() }),
        )
        .await
        .unwrap();
        assert_eq!(out, Value::Null);
        assert!(vault.join(".git").is_dir());
    }
}
