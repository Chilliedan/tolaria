use crate::rpc::{self, RpcError};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tolaria_core::search::{search_vault_with_options, SearchOptions};
use tolaria_core::vault::{
    filter_gitignored_entries, filter_gitignored_folders, get_note_content, parse_md_file,
    scan_vault_cached, scan_vault_folders,
};

#[derive(Deserialize)]
struct PathArgs {
    path: PathBuf,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchArgs {
    query: String,
    limit: Option<usize>,
    exclude_frontmatter: Option<bool>,
}

fn parse_args<T: serde::de::DeserializeOwned>(args: Value) -> Result<T, RpcError> {
    serde_json::from_value(args).map_err(|e| RpcError::bad_request(format!("bad arguments: {e}")))
}

/// Ensure `requested` resolves to a path inside `vault_root`.
/// Returns the canonicalized path on success, or a `RpcError` if it does not exist
/// or is outside the vault.
pub(crate) fn contained_note_path(vault_root: &Path, requested: &Path) -> Result<PathBuf, RpcError> {
    let root = std::fs::canonicalize(vault_root)
        .map_err(|e| RpcError::internal(format!("vault root error: {e}")))?;
    let full = std::fs::canonicalize(requested)
        .map_err(|_| RpcError::bad_request("path does not exist or is invalid"))?;
    if full.starts_with(&root) {
        Ok(full)
    } else {
        Err(RpcError::bad_request("path is outside the vault"))
    }
}

/// Dispatch a command name + JSON args to a read-only handler.
///
/// `vault_root` is the configured vault directory. Vault-root commands
/// ignore any client-supplied path and use `vault_root` directly. Note-path
/// commands accept a client path but enforce that it lies within `vault_root`.
pub fn dispatch(vault_root: &Path, command: &str, args: Value) -> Result<Value, RpcError> {
    match command {
        "list_vault" | "reload_vault" => {
            let entries = scan_vault_cached(vault_root).map_err(RpcError::internal)?;
            let visible = filter_gitignored_entries(vault_root, entries, true);
            Ok(serde_json::to_value(visible).map_err(|e| RpcError::internal(e.to_string()))?)
        }
        "get_note_content" => {
            let a: PathArgs = parse_args(args)?;
            let safe = contained_note_path(vault_root, &a.path)?;
            let content = get_note_content(&safe).map_err(RpcError::internal)?;
            Ok(json!(content))
        }
        "search_vault" => {
            let a: SearchArgs = parse_args(args)?;
            let vault_str = vault_root
                .to_str()
                .ok_or_else(|| RpcError::internal("vault path is not valid UTF-8"))?;
            let resp = search_vault_with_options(SearchOptions {
                vault_path: vault_str,
                query: &a.query,
                mode: "keyword",
                limit: a.limit.unwrap_or(20),
                hide_gitignored_files: true,
                exclude_frontmatter: a.exclude_frontmatter.unwrap_or(false),
            })
            .map_err(RpcError::internal)?;
            Ok(serde_json::to_value(resp).map_err(|e| RpcError::internal(e.to_string()))?)
        }
        "list_vault_folders" => {
            let folders = scan_vault_folders(vault_root).map_err(RpcError::internal)?;
            let visible = filter_gitignored_folders(vault_root, folders, true);
            Ok(serde_json::to_value(visible).map_err(|e| RpcError::internal(e.to_string()))?)
        }
        "get_all_content" => {
            let entries = scan_vault_cached(vault_root).map_err(RpcError::internal)?;
            let visible = filter_gitignored_entries(vault_root, entries, true);
            let mut out = serde_json::Map::new();
            for entry in visible {
                if let Ok(content) = get_note_content(Path::new(&entry.path)) {
                    out.insert(entry.path.clone(), json!(content));
                }
            }
            Ok(Value::Object(out))
        }
        "reload_vault_entry" => {
            let a: PathArgs = parse_args(args)?;
            let safe = contained_note_path(vault_root, &a.path)?;
            let entry = parse_md_file(&safe, None).map_err(RpcError::internal)?;
            Ok(serde_json::to_value(entry).map_err(|e| RpcError::internal(e.to_string()))?)
        }
        // Vault registry: the server serves exactly one vault (`vault_root`), so
        // the app is told its single vault is that path — this is what makes the
        // client build every note path under the real server vault instead of a
        // mock default.
        "load_vault_list" => {
            let path = vault_root.to_string_lossy().to_string();
            let label = vault_root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.clone());
            Ok(json!({
                "vaults": [{ "label": label, "path": path }],
                "active_vault": path,
                "hidden_defaults": [],
            }))
        }
        "get_last_vault_path" => Ok(json!(vault_root.to_string_lossy())),
        // Availability check the app runs per vault before loading it. True only
        // for the server's own vault, so the client marks `/vault` available
        // instead of falling into onboarding.
        "check_vault_exists" => {
            let a: PathArgs = parse_args(args)?;
            let matches_vault = std::fs::canonicalize(&a.path)
                .ok()
                .zip(std::fs::canonicalize(vault_root).ok())
                .map(|(requested, root)| requested == root)
                .unwrap_or(false);
            Ok(json!(matches_vault))
        }
        // Single-vault server: the client cannot reconfigure which vault is
        // served, so persistence of the vault list / last path is a no-op.
        "set_last_vault_path" | "save_vault_list" => Ok(Value::Null),
        // Read-only git status: always operates on the server's own vault_root,
        // ignoring any client-supplied vaultPath, for containment.
        "git_remote_status" => {
            let status =
                tolaria_core::git::git_remote_status(vault_root).map_err(RpcError::internal)?;
            Ok(serde_json::to_value(status).map_err(|e| RpcError::internal(e.to_string()))?)
        }
        other => Err(rpc::unsupported(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn load_vault_list_returns_server_vault_as_active() {
        let dir = tempdir().unwrap();
        let out = dispatch(dir.path(), "load_vault_list", json!({})).unwrap();
        let active = out["active_vault"].as_str().unwrap();
        assert_eq!(active, dir.path().to_string_lossy());
        assert_eq!(out["vaults"][0]["path"].as_str().unwrap(), active);
        assert!(out["hidden_defaults"].is_array());
    }

    #[test]
    fn get_last_vault_path_returns_server_vault() {
        let dir = tempdir().unwrap();
        let out = dispatch(dir.path(), "get_last_vault_path", json!({})).unwrap();
        assert_eq!(out.as_str().unwrap(), dir.path().to_string_lossy());
    }

    #[test]
    fn check_vault_exists_true_for_server_vault_false_otherwise() {
        let dir = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let yes = dispatch(dir.path(), "check_vault_exists", json!({ "path": dir.path() })).unwrap();
        let no = dispatch(dir.path(), "check_vault_exists", json!({ "path": outside.path() })).unwrap();
        assert_eq!(yes, json!(true));
        assert_eq!(no, json!(false));
    }

    #[test]
    fn list_vault_returns_entries() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("note.md"), "# Hello\n\nbody\n").unwrap();
        let out = dispatch(dir.path(), "list_vault", json!({})).unwrap();
        assert!(out.is_array(), "list_vault returns a JSON array");
        assert!(!out.as_array().unwrap().is_empty(), "the note is listed");
    }

    #[test]
    fn get_note_content_returns_text() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("n.md");
        fs::write(&p, "# Title\n\nhello\n").unwrap();
        let out = dispatch(dir.path(), "get_note_content", json!({ "path": p })).unwrap();
        assert_eq!(out.as_str().unwrap(), "# Title\n\nhello\n");
    }

    #[test]
    fn get_note_content_rejects_path_outside_vault() {
        let vault = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let outside_file = outside.path().join("secret.md");
        fs::write(&outside_file, "secret\n").unwrap();
        let err =
            dispatch(vault.path(), "get_note_content", json!({ "path": outside_file })).unwrap_err();
        assert_eq!(err.status, axum::http::StatusCode::BAD_REQUEST);
        assert!(
            err.message.contains("outside the vault"),
            "error message mentions outside the vault: {}",
            err.message
        );
    }

    #[test]
    fn search_vault_finds_keyword() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.md"), "# A\n\nfindme please\n").unwrap();
        let out =
            dispatch(dir.path(), "search_vault", json!({ "query": "findme" })).unwrap();
        assert!(out.get("results").is_some(), "search returns a results field");
    }

    #[test]
    fn unsupported_command_errors_501() {
        let dir = tempdir().unwrap();
        let err = dispatch(dir.path(), "save_note_content", json!({})).unwrap_err();
        assert_eq!(err.status, axum::http::StatusCode::NOT_IMPLEMENTED);
    }

    #[test]
    fn list_vault_folders_returns_array() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("subfolder")).unwrap();
        let out = dispatch(dir.path(), "list_vault_folders", json!({})).unwrap();
        assert!(out.is_array(), "list_vault_folders returns a JSON array");
    }

    #[test]
    fn get_all_content_returns_object_with_note() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("alpha.md"), "# Alpha\n\ncontent here\n").unwrap();
        let out = dispatch(dir.path(), "get_all_content", json!({})).unwrap();
        assert!(out.is_object(), "get_all_content returns a JSON object");
        let obj = out.as_object().unwrap();
        let note_key = obj.keys().find(|k| k.ends_with("alpha.md"));
        assert!(note_key.is_some(), "alpha.md appears as a key in the result");
        assert_eq!(
            obj[note_key.unwrap()].as_str().unwrap(),
            "# Alpha\n\ncontent here\n"
        );
    }

    #[test]
    fn reload_vault_entry_returns_parsed_entry() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("note.md");
        fs::write(&p, "# My Note\n\nbody text\n").unwrap();
        let out = dispatch(dir.path(), "reload_vault_entry", json!({ "path": p })).unwrap();
        assert!(out.is_object(), "reload_vault_entry returns a JSON object");
        let title = out.get("title").and_then(|v| v.as_str());
        assert_eq!(title, Some("My Note"), "title field matches h1 heading");
    }

    #[test]
    fn git_remote_status_reports_no_remote_for_fresh_repo() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path();
        for args in [
            ["init"].as_slice(),
            ["config", "user.email", "t@t"].as_slice(),
            ["config", "user.name", "T"].as_slice(),
        ] {
            std::process::Command::new("git")
                .args(args)
                .current_dir(vault)
                .output()
                .unwrap();
        }
        let out = dispatch(
            vault,
            "git_remote_status",
            serde_json::json!({ "vaultPath": vault.to_string_lossy() }),
        )
        .unwrap();
        assert_eq!(out["hasRemote"], serde_json::json!(false));
    }
}
