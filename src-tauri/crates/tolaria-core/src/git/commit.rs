use super::{ensure_author_config, git_command};
use std::path::Path;
use std::process::Command;

struct CommitFailure {
    stdout: String,
    stderr: String,
}

/// Author + committer identities applied to a server-side commit. The author is
/// the acting user; the committer is a fixed server identity (spec §8).
#[derive(Debug, Clone)]
pub struct CommitIdentity {
    pub author_name: String,
    pub author_email: String,
    pub committer_name: String,
    pub committer_email: String,
}

impl CommitIdentity {
    /// Apply the identity to a git subprocess via the standard git env vars.
    fn apply(&self, command: &mut Command) {
        command
            .env("GIT_AUTHOR_NAME", &self.author_name)
            .env("GIT_AUTHOR_EMAIL", &self.author_email)
            .env("GIT_COMMITTER_NAME", &self.committer_name)
            .env("GIT_COMMITTER_EMAIL", &self.committer_email);
    }
}

/// Stage exactly `paths` and commit them as `identity`. "Nothing to commit"
/// resolves to `Ok(String::new())` so the per-save autogit path is a no-op when
/// the file content did not actually change. Empty `paths` stages nothing new.
///
/// Unlike [`git_commit`], this does NOT call `ensure_author_config`: passing
/// `identity` through [`CommitIdentity::apply`] sets `GIT_AUTHOR_*` /
/// `GIT_COMMITTER_*` env vars, which fully override git config resolution, so
/// there is no local/global config to fall back to or ensure.
pub fn git_commit_paths_as(
    vault_path: &str,
    paths: &[String],
    message: &str,
    identity: &CommitIdentity,
) -> Result<String, String> {
    let vault = Path::new(vault_path);
    if !paths.is_empty() {
        let mut add = git_command();
        add.args(["add", "--"]);
        add.args(paths);
        let add = add
            .current_dir(vault)
            .output()
            .map_err(|e| format!("Failed to run git add: {}", e))?;
        if !add.status.success() {
            return Err(format!(
                "git add failed: {}",
                String::from_utf8_lossy(&add.stderr)
            ));
        }
    }
    match run_commit_as(vault, message, identity, false) {
        Ok(stdout) => Ok(stdout),
        Err(failure) if is_nothing_to_commit(&failure.detail()) => Ok(String::new()),
        Err(failure) if is_commit_signing_failure(&failure.detail()) => {
            run_commit_as(vault, message, identity, true).or_else(|f| {
                if is_nothing_to_commit(&f.detail()) {
                    Ok(String::new())
                } else {
                    Err(format!("git commit failed: {}", f.detail()))
                }
            })
        }
        Err(failure) => Err(format!("git commit failed: {}", failure.detail())),
    }
}

/// Stage all changes (`git add -A`) and commit as `identity`. Used by the
/// explicit web "commit" control; "nothing to commit" is a real error here.
///
/// Unlike [`git_commit`], this does NOT call `ensure_author_config`: passing
/// `identity` through [`CommitIdentity::apply`] sets `GIT_AUTHOR_*` /
/// `GIT_COMMITTER_*` env vars, which fully override git config resolution, so
/// there is no local/global config to fall back to or ensure.
pub fn git_commit_all_as(
    vault_path: &str,
    message: &str,
    identity: &CommitIdentity,
) -> Result<String, String> {
    let vault = Path::new(vault_path);
    let add = git_command()
        .args(["add", "-A"])
        .current_dir(vault)
        .output()
        .map_err(|e| format!("Failed to run git add: {}", e))?;
    if !add.status.success() {
        return Err(format!(
            "git add failed: {}",
            String::from_utf8_lossy(&add.stderr)
        ));
    }
    match run_commit_as(vault, message, identity, false) {
        Ok(stdout) => Ok(stdout),
        Err(failure) if is_commit_signing_failure(&failure.detail()) => {
            run_commit_as(vault, message, identity, true)
                .map_err(|f| format!("git commit failed: {}", f.detail()))
        }
        Err(failure) => Err(format!("git commit failed: {}", failure.detail())),
    }
}

fn run_commit_as(
    vault: &Path,
    message: &str,
    identity: &CommitIdentity,
    disable_signing: bool,
) -> Result<String, CommitFailure> {
    let mut command = git_command();
    identity.apply(&mut command);
    if disable_signing {
        command.args(["-c", "commit.gpgsign=false"]);
    }
    let commit = command
        .args(["commit", "-m", message])
        .current_dir(vault)
        .output()
        .map_err(|e| CommitFailure {
            stdout: String::new(),
            stderr: format!("Failed to run git commit: {}", e),
        })?;
    if commit.status.success() {
        return Ok(String::from_utf8_lossy(&commit.stdout).to_string());
    }
    Err(CommitFailure {
        stdout: String::from_utf8_lossy(&commit.stdout).to_string(),
        stderr: String::from_utf8_lossy(&commit.stderr).to_string(),
    })
}

fn is_nothing_to_commit(detail: &str) -> bool {
    detail.to_ascii_lowercase().contains("nothing to commit")
}

/// Commit all changes with a message.
pub fn git_commit(vault_path: &str, message: &str) -> Result<String, String> {
    let vault = Path::new(vault_path);

    // Stage all changes
    let add = git_command()
        .args(["add", "-A"])
        .current_dir(vault)
        .output()
        .map_err(|e| format!("Failed to run git add: {}", e))?;

    if !add.status.success() {
        let stderr = String::from_utf8_lossy(&add.stderr);
        return Err(format!("git add failed: {}", stderr));
    }

    ensure_author_config(vault)?;

    match run_commit(vault, message, false) {
        Ok(stdout) => Ok(stdout),
        Err(failure) if is_commit_signing_failure(&failure.detail()) => {
            run_commit(vault, message, true).map_err(|retry_failure| {
                format!(
                    "git commit signing failed; retried without signing but git commit still failed: {}",
                    retry_failure.detail()
                )
            })
        }
        Err(failure) => Err(format!("git commit failed: {}", failure.detail())),
    }
}

fn run_commit(vault: &Path, message: &str, disable_signing: bool) -> Result<String, CommitFailure> {
    let mut command = git_command();
    if disable_signing {
        command.args(["-c", "commit.gpgsign=false"]);
    }

    let commit = command
        .args(["commit", "-m", message])
        .current_dir(vault)
        .output()
        .map_err(|e| CommitFailure {
            stdout: String::new(),
            stderr: format!("Failed to run git commit: {}", e),
        })?;

    if commit.status.success() {
        return Ok(String::from_utf8_lossy(&commit.stdout).to_string());
    }

    Err(CommitFailure {
        stdout: String::from_utf8_lossy(&commit.stdout).to_string(),
        stderr: String::from_utf8_lossy(&commit.stderr).to_string(),
    })
}

impl CommitFailure {
    fn detail(&self) -> String {
        // git writes "nothing to commit" to stdout, not stderr.
        let detail = if self.stderr.trim().is_empty() {
            &self.stdout
        } else {
            &self.stderr
        };
        detail.trim().to_string()
    }
}

fn is_commit_signing_failure(detail: &str) -> bool {
    let lower = detail.to_ascii_lowercase();
    lower.contains("cannot run gpg")
        || lower.contains("gpg failed to sign")
        || lower.contains("failed to sign the data")
        || lower.contains("gpg.ssh")
        || (lower.contains("failed to write commit object")
            && (lower.contains("sign") || lower.contains("gpg")))
}

#[cfg(test)]
mod tests {
    use super::git_command;
    use super::*;
    use crate::git::tests::{setup_git_repo, GitConfigEnvGuard};
    use std::fs;
    use std::path::Path;

    fn unset_local_author_config(vault: &Path) {
        for key in ["user.name", "user.email"] {
            let status = git_command()
                .args(["config", "--local", "--unset-all", key])
                .current_dir(vault)
                .status()
                .unwrap();
            assert!(status.success(), "failed to unset {key}");
        }
    }

    fn local_config_value(vault: &Path, key: &str) -> Option<String> {
        let output = git_command()
            .args(["config", "--local", key])
            .current_dir(vault)
            .output()
            .unwrap();
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    #[test]
    fn test_git_commit() {
        let dir = setup_git_repo();
        let vault = dir.path();

        fs::write(vault.join("commit-test.md"), "# Test\n").unwrap();

        let result = git_commit(vault.to_str().unwrap(), "Test commit");
        assert!(result.is_ok());

        // Verify the commit exists
        let log = git_command()
            .args(["log", "--oneline", "-1"])
            .current_dir(vault)
            .output()
            .unwrap();
        let log_str = String::from_utf8_lossy(&log.stdout);
        assert!(log_str.contains("Test commit"));
    }

    #[test]
    fn test_git_commit_sets_missing_local_author_identity() {
        let _env = GitConfigEnvGuard::isolated();

        let dir = setup_git_repo();
        let vault = dir.path();
        unset_local_author_config(vault);

        fs::write(vault.join("identity-fallback.md"), "# Identity fallback\n").unwrap();

        let result = git_commit(vault.to_str().unwrap(), "Commit without local identity");
        assert!(
            result.is_ok(),
            "commit should set local fallback identity: {result:?}"
        );

        assert_eq!(
            local_config_value(vault, "user.name").as_deref(),
            Some("Tolaria")
        );
        assert_eq!(
            local_config_value(vault, "user.email").as_deref(),
            Some("vault@tolaria.default")
        );

        let author = git_command()
            .args(["log", "-1", "--format=%an <%ae>"])
            .current_dir(vault)
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&author.stdout).trim(),
            "Tolaria <vault@tolaria.default>"
        );
    }

    #[test]
    fn test_git_commit_respects_global_author_identity() {
        let _env =
            GitConfigEnvGuard::with_global_identity(Some(("Global User", "global@test.com")));

        let dir = setup_git_repo();
        let vault = dir.path();
        unset_local_author_config(vault);

        fs::write(vault.join("global-identity.md"), "# Global identity\n").unwrap();

        let result = git_commit(vault.to_str().unwrap(), "Commit with global identity");
        assert!(
            result.is_ok(),
            "commit should use the global identity: {result:?}"
        );

        // The global identity resolves, so no local override is written.
        assert_eq!(local_config_value(vault, "user.name"), None);
        assert_eq!(local_config_value(vault, "user.email"), None);

        let author = git_command()
            .args(["log", "-1", "--format=%an <%ae>"])
            .current_dir(vault)
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&author.stdout).trim(),
            "Global User <global@test.com>"
        );
    }

    #[test]
    fn test_commit_nothing_to_commit_returns_error() {
        let dir = setup_git_repo();
        let vault = dir.path();
        let vp = vault.to_str().unwrap();

        // Create and commit, so working tree is clean
        fs::write(vault.join("clean.md"), "# Clean\n").unwrap();
        git_commit(vp, "initial").unwrap();

        // Committing again with no changes should fail
        let result = git_commit(vp, "nothing here");
        assert!(result.is_err(), "Commit should fail when nothing to commit");
        assert!(
            result.unwrap_err().contains("nothing to commit"),
            "Error should mention 'nothing to commit'"
        );
    }

    #[test]
    fn test_git_commit_retries_without_signing_when_gpg_is_missing() {
        let dir = setup_git_repo();
        let vault = dir.path();
        let vp = vault.to_str().unwrap();

        git_command()
            .args(["config", "commit.gpgsign", "true"])
            .current_dir(vault)
            .output()
            .unwrap();
        git_command()
            .args(["config", "gpg.program", "/missing/tolaria-test-gpg"])
            .current_dir(vault)
            .output()
            .unwrap();
        fs::write(vault.join("signed-config.md"), "# Signed config\n").unwrap();

        let result = git_commit(vp, "Commit with broken signing config");
        assert!(
            result.is_ok(),
            "commit should retry unsigned when signing helper is missing: {result:?}"
        );

        let log = git_command()
            .args(["log", "--oneline", "-1"])
            .current_dir(vault)
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&log.stdout).contains("Commit with broken signing config"));

        let config = git_command()
            .args(["config", "commit.gpgsign"])
            .current_dir(vault)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&config.stdout).trim(), "true");
    }

    #[test]
    fn test_commit_signing_failure_detection_is_specific() {
        assert!(is_commit_signing_failure(
            "error: cannot run gpg: No such file or directory\nfatal: failed to write commit object"
        ));
        assert!(!is_commit_signing_failure(
            "On branch main\nnothing to commit, working tree clean"
        ));
    }

    #[test]
    fn git_commit_paths_as_uses_supplied_author_and_committer() {
        let _env = GitConfigEnvGuard::isolated();
        let dir = setup_git_repo();
        let vault = dir.path();
        let vp = vault.to_str().unwrap();

        fs::write(vault.join("alice-note.md"), "# Alice\n").unwrap();
        // A second unrelated edit must NOT be swept into Alice's scoped commit.
        fs::write(vault.join("other.md"), "# Other\n").unwrap();

        let identity = CommitIdentity {
            author_name: "Alice".into(),
            author_email: "alice@example.com".into(),
            committer_name: "Tolaria Server".into(),
            committer_email: "server@tolaria.local".into(),
        };
        let out = git_commit_paths_as(vp, &["alice-note.md".into()], "add alice note", &identity);
        assert!(out.is_ok(), "scoped commit should succeed: {out:?}");

        let author = git_command()
            .args(["log", "-1", "--format=%an <%ae> | %cn <%ce>"])
            .current_dir(vault)
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&author.stdout).trim(),
            "Alice <alice@example.com> | Tolaria Server <server@tolaria.local>"
        );

        // other.md must still be uncommitted (not staged by the scoped commit).
        let status = git_command()
            .args(["status", "--porcelain", "--", "other.md"])
            .current_dir(vault)
            .output()
            .unwrap();
        assert!(
            !String::from_utf8_lossy(&status.stdout).trim().is_empty(),
            "other.md must remain uncommitted"
        );
    }

    #[test]
    fn git_commit_paths_as_nothing_to_commit_is_ok_empty() {
        let _env = GitConfigEnvGuard::isolated();
        let dir = setup_git_repo();
        let vault = dir.path();
        let vp = vault.to_str().unwrap();
        fs::write(vault.join("x.md"), "# X\n").unwrap();
        let identity = CommitIdentity {
            author_name: "Bob".into(),
            author_email: "bob@example.com".into(),
            committer_name: "Tolaria Server".into(),
            committer_email: "server@tolaria.local".into(),
        };
        git_commit_paths_as(vp, &["x.md".into()], "first", &identity).unwrap();
        // No new changes → autogit no-op → Ok("").
        let again = git_commit_paths_as(vp, &["x.md".into()], "again", &identity).unwrap();
        assert_eq!(again, "");
    }
}
