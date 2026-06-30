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
    vault_path: String,
    query: String,
    limit: Option<usize>,
    exclude_frontmatter: Option<bool>,
}

fn parse_args<T: serde::de::DeserializeOwned>(args: Value) -> Result<T, RpcError> {
    serde_json::from_value(args).map_err(|e| RpcError::bad_request(format!("bad arguments: {e}")))
}

/// Dispatch a command name + JSON args to a read-only handler.
pub fn dispatch(command: &str, args: Value) -> Result<Value, RpcError> {
    match command {
        "list_vault" | "reload_vault" => {
            let a: PathArgs = parse_args(args)?;
            let entries = scan_vault_cached(&a.path).map_err(RpcError::internal)?;
            let visible = filter_gitignored_entries(&a.path, entries, true);
            Ok(serde_json::to_value(visible).map_err(|e| RpcError::internal(e.to_string()))?)
        }
        "get_note_content" => {
            let a: PathArgs = parse_args(args)?;
            let content = get_note_content(&a.path).map_err(RpcError::internal)?;
            Ok(json!(content))
        }
        "search_vault" => {
            let a: SearchArgs = parse_args(args)?;
            let resp = search_vault_with_options(SearchOptions {
                vault_path: &a.vault_path,
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
            let a: PathArgs = parse_args(args)?;
            let folders = scan_vault_folders(&a.path).map_err(RpcError::internal)?;
            let visible = filter_gitignored_folders(&a.path, folders, true);
            Ok(serde_json::to_value(visible).map_err(|e| RpcError::internal(e.to_string()))?)
        }
        "get_all_content" => {
            let a: PathArgs = parse_args(args)?;
            let entries = scan_vault_cached(&a.path).map_err(RpcError::internal)?;
            let visible = filter_gitignored_entries(&a.path, entries, true);
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
            let entry = parse_md_file(&a.path, None).map_err(RpcError::internal)?;
            Ok(serde_json::to_value(entry).map_err(|e| RpcError::internal(e.to_string()))?)
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
    fn list_vault_returns_entries() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("note.md"), "# Hello\n\nbody\n").unwrap();
        let out = dispatch("list_vault", json!({ "path": dir.path() })).unwrap();
        assert!(out.is_array(), "list_vault returns a JSON array");
        assert!(!out.as_array().unwrap().is_empty(), "the note is listed");
    }

    #[test]
    fn get_note_content_returns_text() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("n.md");
        fs::write(&p, "# Title\n\nhello\n").unwrap();
        let out = dispatch("get_note_content", json!({ "path": p })).unwrap();
        assert_eq!(out.as_str().unwrap(), "# Title\n\nhello\n");
    }

    #[test]
    fn search_vault_finds_keyword() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.md"), "# A\n\nfindme please\n").unwrap();
        let out = dispatch(
            "search_vault",
            json!({ "vaultPath": dir.path(), "query": "findme" }),
        )
        .unwrap();
        assert!(out.get("results").is_some(), "search returns a results field");
    }

    #[test]
    fn unsupported_command_errors_501() {
        let err = dispatch("save_note_content", json!({})).unwrap_err();
        assert_eq!(err.status, axum::http::StatusCode::NOT_IMPLEMENTED);
    }

    #[test]
    fn list_vault_folders_returns_array() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("subfolder")).unwrap();
        let out = dispatch("list_vault_folders", json!({ "path": dir.path() })).unwrap();
        assert!(out.is_array(), "list_vault_folders returns a JSON array");
    }

    #[test]
    fn get_all_content_returns_object_with_note() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("alpha.md"), "# Alpha\n\ncontent here\n").unwrap();
        let out = dispatch("get_all_content", json!({ "path": dir.path() })).unwrap();
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
        let out = dispatch("reload_vault_entry", json!({ "path": p })).unwrap();
        assert!(out.is_object(), "reload_vault_entry returns a JSON object");
        let title = out.get("title").and_then(|v| v.as_str());
        assert_eq!(title, Some("My Note"), "title field matches h1 heading");
    }
}
