use std::net::SocketAddr;
use std::path::PathBuf;

/// Runtime configuration, sourced from environment variables.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub vault_path: PathBuf,
    pub static_dir: PathBuf,
    pub listen_addr: SocketAddr,
    pub users_db_path: PathBuf,
    pub cookie_secure: bool,
}

impl ServerConfig {
    /// Build config from a key lookup function (env in production, a map in tests).
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let vault_path = get("TOLARIA_VAULT_PATH")
            .ok_or_else(|| "TOLARIA_VAULT_PATH is required".to_string())?
            .into();
        let static_dir = get("TOLARIA_STATIC_DIR").unwrap_or_else(|| "/app/dist".into()).into();
        let host = get("TOLARIA_HOST").unwrap_or_else(|| "0.0.0.0".into());
        let port = get("TOLARIA_PORT").unwrap_or_else(|| "8787".into());
        let listen_addr = format!("{host}:{port}")
            .parse()
            .map_err(|e| format!("invalid listen address {host}:{port}: {e}"))?;
        let users_db_path = get("TOLARIA_USERS_DB")
            .unwrap_or_else(|| "/app/data/users.db".into())
            .into();
        let cookie_secure = !matches!(
            get("TOLARIA_COOKIE_SECURE").as_deref(),
            Some("false") | Some("0")
        );
        Ok(Self { vault_path, static_dir, listen_addr, users_db_path, cookie_secure })
    }

    pub fn from_env() -> Result<Self, String> {
        Self::from_lookup(|k| std::env::var(k).ok())
    }
}

/// Fixed git committer identity for server-authored commits, from
/// `TOLARIA_COMMITTER_NAME` / `TOLARIA_COMMITTER_EMAIL` (with sane defaults).
pub fn committer_identity() -> (String, String) {
    let name =
        std::env::var("TOLARIA_COMMITTER_NAME").unwrap_or_else(|_| "Tolaria Server".to_string());
    let email = std::env::var("TOLARIA_COMMITTER_EMAIL")
        .unwrap_or_else(|_| "server@tolaria.local".to_string());
    (name, email)
}

/// Whether to auto-commit each write as the acting user. Default on; set
/// `TOLARIA_AUTOGIT=false` to disable.
pub fn autogit_enabled() -> bool {
    !matches!(
        std::env::var("TOLARIA_AUTOGIT").ok().as_deref(),
        Some("false") | Some("0")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn lookup<'a>(map: &'a HashMap<&str, &str>) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| map.get(k).map(|v| v.to_string())
    }

    #[test]
    fn requires_vault_path() {
        let map = HashMap::new();
        assert!(ServerConfig::from_lookup(lookup(&map)).is_err());
    }

    #[test]
    fn defaults_host_port_static_dir() {
        let map = HashMap::from([("TOLARIA_VAULT_PATH", "/vault")]);
        let cfg = ServerConfig::from_lookup(lookup(&map)).unwrap();
        assert_eq!(cfg.vault_path, PathBuf::from("/vault"));
        assert_eq!(cfg.static_dir, PathBuf::from("/app/dist"));
        assert_eq!(cfg.listen_addr.to_string(), "0.0.0.0:8787");
    }

    #[test]
    fn rejects_bad_address() {
        let map = HashMap::from([("TOLARIA_VAULT_PATH", "/vault"), ("TOLARIA_PORT", "notaport")]);
        assert!(ServerConfig::from_lookup(lookup(&map)).is_err());
    }

    #[test]
    fn defaults_users_db_and_cookie_secure() {
        let map = HashMap::from([("TOLARIA_VAULT_PATH", "/vault")]);
        let cfg = ServerConfig::from_lookup(lookup(&map)).unwrap();
        assert_eq!(cfg.users_db_path, PathBuf::from("/app/data/users.db"));
        assert!(cfg.cookie_secure);
    }

    #[test]
    fn cookie_secure_can_be_disabled() {
        let map = HashMap::from([("TOLARIA_VAULT_PATH", "/vault"), ("TOLARIA_COOKIE_SECURE", "false")]);
        let cfg = ServerConfig::from_lookup(lookup(&map)).unwrap();
        assert!(!cfg.cookie_secure);
    }

    /// Serializes tests that mutate process-global env vars so they cannot
    /// interleave and observe each other's values.
    static ENV_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn committer_identity_defaults_when_unset() {
        let _guard = ENV_GUARD.lock().unwrap();
        std::env::remove_var("TOLARIA_COMMITTER_NAME");
        std::env::remove_var("TOLARIA_COMMITTER_EMAIL");
        let (name, email) = committer_identity();
        assert_eq!(name, "Tolaria Server");
        assert_eq!(email, "server@tolaria.local");
    }

    #[test]
    fn committer_identity_reads_env_overrides() {
        let _guard = ENV_GUARD.lock().unwrap();
        std::env::set_var("TOLARIA_COMMITTER_NAME", "Custom Bot");
        std::env::set_var("TOLARIA_COMMITTER_EMAIL", "bot@example.com");
        let (name, email) = committer_identity();
        assert_eq!(name, "Custom Bot");
        assert_eq!(email, "bot@example.com");
        std::env::remove_var("TOLARIA_COMMITTER_NAME");
        std::env::remove_var("TOLARIA_COMMITTER_EMAIL");
    }

    #[test]
    fn autogit_enabled_defaults_true_when_unset() {
        let _guard = ENV_GUARD.lock().unwrap();
        std::env::remove_var("TOLARIA_AUTOGIT");
        assert!(autogit_enabled());
    }

    #[test]
    fn autogit_enabled_false_for_false_or_zero() {
        let _guard = ENV_GUARD.lock().unwrap();
        std::env::set_var("TOLARIA_AUTOGIT", "false");
        assert!(!autogit_enabled());
        std::env::set_var("TOLARIA_AUTOGIT", "0");
        assert!(!autogit_enabled());
        std::env::remove_var("TOLARIA_AUTOGIT");
    }
}
