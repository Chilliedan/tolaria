use super::expand_tilde;

#[cfg(desktop)]
#[tauri::command]
pub async fn clone_git_repo(url: String, local_path: String) -> Result<String, String> {
    let url = crate::git::validate_user_remote_url(&url)?.to_string();
    let local_path = expand_tilde(&local_path).into_owned();

    tokio::task::spawn_blocking(move || crate::git::clone_repo(&url, &local_path))
        .await
        .map_err(|e| format!("Task panicked: {e}"))?
}

#[cfg(mobile)]
#[tauri::command]
pub async fn clone_git_repo(_url: String, _local_path: String) -> Result<String, String> {
    Err("Git clone is not available on mobile".into())
}

#[cfg(all(test, desktop))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn clone_git_repo_rejects_invalid_url_before_touching_disk() {
        let dir = tempfile::TempDir::new().unwrap();
        let dest = dir.path().join("cloned-vault");

        let result =
            clone_git_repo("not-a-url".to_string(), dest.to_string_lossy().to_string()).await;

        assert!(result.is_err(), "an invalid URL should be rejected");
        assert!(
            !dest.exists(),
            "clone should not touch disk when the URL fails validation"
        );
    }
}
