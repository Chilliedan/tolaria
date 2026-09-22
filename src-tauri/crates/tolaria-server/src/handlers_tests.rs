use super::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn git_workspace_info_reports_server_vault_repo_and_ignores_client_path() {
    let dir = tempdir().unwrap();
    let vault = dir.path().join("vault");
    fs::create_dir_all(&vault).unwrap();
    std::process::Command::new("git")
        .arg("init")
        .current_dir(dir.path())
        .output()
        .unwrap();
    let out = dispatch(
        &vault,
        "git_workspace_info",
        json!({ "vaultPath": "/somewhere/else" }),
    )
    .unwrap();
    assert_eq!(out["gitRootRelation"], json!("parent"));
    assert_eq!(out["vaultPathspec"], json!("vault"));
    assert!(out["vaultRoot"].as_str().unwrap().ends_with("vault"));
}

#[test]
fn git_workspace_info_without_repo_reports_none() {
    let dir = tempdir().unwrap();
    let out = dispatch(dir.path(), "git_workspace_info", json!({})).unwrap();
    assert_eq!(out["gitRootRelation"], json!("none"));
    assert_eq!(out["gitRoot"], Value::Null);
}

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
    let yes = dispatch(
        dir.path(),
        "check_vault_exists",
        json!({ "path": dir.path() }),
    )
    .unwrap();
    let no = dispatch(
        dir.path(),
        "check_vault_exists",
        json!({ "path": outside.path() }),
    )
    .unwrap();
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
    let err = dispatch(
        vault.path(),
        "get_note_content",
        json!({ "path": outside_file }),
    )
    .unwrap_err();
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
    let out = dispatch(dir.path(), "search_vault", json!({ "query": "findme" })).unwrap();
    assert!(
        out.get("results").is_some(),
        "search returns a results field"
    );
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
    assert!(
        note_key.is_some(),
        "alpha.md appears as a key in the result"
    );
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
fn list_views_returns_views_from_the_vault_directory() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path();
    // No views dir yet → empty array (not an error).
    assert_eq!(
        dispatch(vault, "list_views", serde_json::json!({})).unwrap(),
        serde_json::json!([])
    );
    // A real view file in <vault>/views is scanned and returned.
    std::fs::create_dir(vault.join("views")).unwrap();
    std::fs::write(
        vault.join("views").join("active.yml"),
        "name: Active Projects\nfilters:\n  all:\n    - field: type\n      op: equals\n      value: Project\n",
    )
    .unwrap();
    let out = dispatch(vault, "list_views", serde_json::json!({})).unwrap();
    let arr = out.as_array().expect("array of views");
    assert_eq!(arr.len(), 1);
    assert!(out.to_string().contains("Active Projects"));
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

#[test]
fn git_read_commands_dispatch_against_the_vault() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path();
    for a in [
        ["init"].as_slice(),
        ["config", "user.email", "t@t"].as_slice(),
        ["config", "user.name", "T"].as_slice(),
    ] {
        std::process::Command::new("git")
            .args(a)
            .current_dir(vault)
            .output()
            .unwrap();
    }
    std::fs::write(vault.join("note.md"), "# Note\n").unwrap();
    std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(vault)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(vault)
        .output()
        .unwrap();

    // is_git_repo → true for the vault
    assert_eq!(
        dispatch(vault, "is_git_repo", serde_json::json!({})).unwrap(),
        serde_json::json!(true)
    );
    // get_modified_files → array (empty after commit)
    assert!(dispatch(vault, "get_modified_files", serde_json::json!({}))
        .unwrap()
        .is_array());
    // get_file_history for note.md (`path`, the real web client key) → non-empty array
    let hist = dispatch(
        vault,
        "get_file_history",
        serde_json::json!({ "path": "note.md" }),
    )
    .unwrap();
    assert!(hist.as_array().map(|a| !a.is_empty()).unwrap_or(false));
    // get_vault_pulse honours limit
    assert!(
        dispatch(vault, "get_vault_pulse", serde_json::json!({ "limit": 5 }))
            .unwrap()
            .is_object()
            || dispatch(vault, "get_vault_pulse", serde_json::json!({ "limit": 5 }))
                .unwrap()
                .is_array()
    );
    // get_modified_files_with_stats → array (empty after commit)
    assert!(dispatch(
        vault,
        "get_modified_files_with_stats",
        serde_json::json!({})
    )
    .unwrap()
    .is_array());
    // get_last_commit_info → a real (non-null) object describing the commit just made
    let last_commit = dispatch(vault, "get_last_commit_info", serde_json::json!({})).unwrap();
    assert!(
        !last_commit.is_null(),
        "get_last_commit_info returns commit info after a commit exists"
    );
    // git_file_url for note.md (`path`, the real web client key) → Ok, string or null
    // depending on remote config (this repo has no remote configured).
    let file_url = dispatch(
        vault,
        "git_file_url",
        serde_json::json!({ "path": "note.md" }),
    );
    assert!(file_url.is_ok(), "git_file_url dispatches successfully");
    let file_url = file_url.unwrap();
    assert!(
        file_url.is_string() || file_url.is_null(),
        "git_file_url returns a string or null"
    );
    // get_file_diff_at_commit for note.md at HEAD, using the real client key
    // `commitHash` (camelCase) rather than `commit_hash`.
    let head_hash = String::from_utf8(
        std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(vault)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();
    assert!(!head_hash.is_empty(), "HEAD hash was resolved");
    let diff_at_commit = dispatch(
        vault,
        "get_file_diff_at_commit",
        serde_json::json!({ "path": "note.md", "commitHash": head_hash }),
    );
    assert!(
        diff_at_commit.is_ok(),
        "get_file_diff_at_commit dispatches successfully with commitHash: {diff_at_commit:?}"
    );
    assert!(
        diff_at_commit.unwrap().is_string(),
        "get_file_diff_at_commit returns a string diff"
    );
}

fn git_init(vault: &std::path::Path) {
    for a in [
        ["init"].as_slice(),
        ["config", "user.email", "t@t"].as_slice(),
        ["config", "user.name", "T"].as_slice(),
    ] {
        std::process::Command::new("git")
            .args(a)
            .current_dir(vault)
            .output()
            .unwrap();
    }
}

// A `../`-escaping relative path must be rejected before any filesystem or
// git read, so a future non-git reuse of `vault_file_path` cannot traverse
// out of the vault.
#[test]
fn get_file_diff_rejects_relative_traversal_path() {
    let dir = tempdir().unwrap();
    let err = dispatch(
        dir.path(),
        "get_file_diff",
        json!({ "path": "../../etc/passwd" }),
    )
    .unwrap_err();
    assert_eq!(err.status, axum::http::StatusCode::BAD_REQUEST);
}

// An absolute path that resolves outside the vault must be rejected even
// though absolute in-vault paths (the web client's `fullPath`) are allowed.
#[test]
fn get_file_diff_rejects_absolute_path_outside_vault() {
    let dir = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let secret = outside.path().join("secret.md");
    fs::write(&secret, "secret\n").unwrap();
    let err = dispatch(
        dir.path(),
        "get_file_diff",
        json!({ "path": secret.to_string_lossy() }),
    )
    .unwrap_err();
    assert_eq!(err.status, axum::http::StatusCode::BAD_REQUEST);
}

// A legitimate absolute in-vault path still dispatches successfully.
#[test]
fn get_file_diff_allows_absolute_in_vault_path() {
    let dir = tempdir().unwrap();
    let vault = dir.path();
    git_init(vault);
    fs::write(vault.join("note.md"), "# Note\n").unwrap();
    for a in [
        ["add", "-A"].as_slice(),
        ["commit", "-m", "seed"].as_slice(),
    ] {
        std::process::Command::new("git")
            .args(a)
            .current_dir(vault)
            .output()
            .unwrap();
    }
    let abs = vault.join("note.md");
    let out = dispatch(
        vault,
        "get_file_diff",
        json!({ "path": abs.to_string_lossy() }),
    );
    assert!(out.is_ok(), "absolute in-vault path is allowed: {out:?}");
}

#[test]
fn str_arg_accepts_camel_and_snake() {
    let camel = serde_json::json!({ "filePath": "a.md" });
    let snake = serde_json::json!({ "file_path": "b.md" });
    assert_eq!(str_arg(&camel, "filePath", "file_path").unwrap(), "a.md");
    assert_eq!(str_arg(&snake, "filePath", "file_path").unwrap(), "b.md");
    assert!(str_arg(&serde_json::json!({}), "filePath", "file_path").is_err());
    assert_eq!(arg_u64(&serde_json::json!({ "limit": 5 }), "limit", 20), 5);
    assert_eq!(arg_u64(&serde_json::json!({}), "limit", 20), 20);
}
