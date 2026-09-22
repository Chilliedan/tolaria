use crate::rpc::{self, str_arg, RpcError};
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
                vault_path: vault_str.to_string(),
                query: a.query,
                mode: "keyword".to_string(),
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
        "list_views" => {
            let views = tolaria_core::vault::scan_views(vault_root);
            serde_json::to_value(views).map_err(|e| RpcError::internal(e.to_string()))
        }
        other => dispatch_git_read(vault_root, other, &args),
    }
}

/// Read-only git commands. Every arm operates on the server's own
/// `vault_root`, ignoring any client-supplied `vaultPath`, for containment;
/// none touch the git index, so none need the repo lock.
fn dispatch_git_read(vault_root: &Path, command: &str, args: &Value) -> Result<Value, RpcError> {
    match command {
        "git_remote_status" => {
            let status =
                tolaria_core::git::git_remote_status(vault_root).map_err(RpcError::internal)?;
            Ok(serde_json::to_value(status).map_err(|e| RpcError::internal(e.to_string()))?)
        }
        "is_git_repo" => Ok(Value::Bool(vault_root.join(".git").is_dir())),
        "git_workspace_info" => serialize(Ok(tolaria_core::git::git_workspace_info(vault_root))),
        "get_modified_files" => serialize(tolaria_core::git::get_modified_files(vault_root)),
        "get_modified_files_with_stats" => {
            serialize(tolaria_core::git::get_modified_files_with_stats(vault_root))
        }
        "get_last_commit_info" => serialize(tolaria_core::git::get_last_commit_info(vault_root)),
        "get_vault_pulse" => serialize(tolaria_core::git::get_vault_pulse(
            vault_root,
            arg_u64(args, "limit", 30) as usize,
            arg_u64(args, "skip", 0) as usize,
        )),
        "get_file_diff" | "get_file_diff_at_commit" | "get_file_history" | "git_file_url" => {
            dispatch_git_file_read(vault_root, command, args)
        }
        other => Err(rpc::unsupported(other)),
    }
}

/// Git reads scoped to one vault file, whose path arg is containment-checked
/// before any git call.
fn dispatch_git_file_read(
    vault_root: &Path,
    command: &str,
    args: &Value,
) -> Result<Value, RpcError> {
    let file = vault_file_path(vault_root, &file_path_arg(args)?)?;
    let vault = vault_root.to_string_lossy();
    match command {
        "get_file_diff" => serialize(tolaria_core::git::get_file_diff(&vault, &file)),
        "get_file_diff_at_commit" => {
            let commit = str_arg(args, "commitHash", "commit_hash")?;
            serialize(tolaria_core::git::get_file_diff_at_commit(
                &vault, &file, &commit,
            ))
        }
        "get_file_history" => serialize(tolaria_core::git::get_file_history(&vault, &file)),
        _ => serialize(tolaria_core::git::git_file_url(&vault, &file)),
    }
}

/// Read the required file-path arg for git read commands. The real web
/// client sends the vault-relative path under `path`; fall back to
/// `file_path`/`filePath` for other callers (e.g. desktop, direct tests).
fn file_path_arg(args: &Value) -> Result<String, RpcError> {
    str_arg(args, "path", "file_path").or_else(|_| str_arg(args, "filePath", "file_path"))
}

/// Numeric arg with a default when absent or non-numeric.
fn arg_u64(args: &Value, key: &str, default: u64) -> u64 {
    args.get(key).and_then(|v| v.as_u64()).unwrap_or(default)
}

/// Resolve a client-supplied file path (vault-relative, as sent by the web
/// client, or an absolute in-vault `fullPath`) into a full path string
/// understood by `tolaria_core::git`, which expects paths rooted at (or
/// inside) the vault.
///
/// Containment is enforced here rather than relying on git refusing an
/// out-of-repo pathspec, so a future non-git reuse of this helper cannot
/// traverse out of the vault. Absolute paths must canonicalize to a location
/// inside the vault (via [`contained_note_path`]); relative paths must not
/// escape via a `..` component. Escapes return `400 Bad Request`.
fn vault_file_path(vault_root: &Path, file_path: &str) -> Result<String, RpcError> {
    let candidate = Path::new(file_path);
    if candidate.is_absolute() {
        // Validate containment (canonicalize + `starts_with`) but return the
        // ORIGINAL string: core's git relativization strips the vault_root's
        // own — possibly non-canonical (`/var` vs `/private/var`) — prefix, so
        // handing it the canonicalized form would break that match.
        contained_note_path(vault_root, candidate)?;
        return Ok(file_path.to_string());
    }
    if candidate
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(RpcError::bad_request("path is outside the vault"));
    }
    Ok(vault_root.join(candidate).to_string_lossy().to_string())
}

/// Map a `Result<T: Serialize, String>` core result into an RPC JSON result:
/// core error → 500, serialization error → 500.
fn serialize<T: serde::Serialize>(result: Result<T, String>) -> Result<Value, RpcError> {
    let value = result.map_err(RpcError::internal)?;
    serde_json::to_value(value).map_err(|e| RpcError::internal(e.to_string()))
}

#[cfg(test)]
#[path = "handlers_tests.rs"]
mod tests;
