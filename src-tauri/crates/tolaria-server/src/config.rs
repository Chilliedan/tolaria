use std::net::SocketAddr;
use std::path::PathBuf;

/// Runtime configuration, sourced from environment variables.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub vault_path: PathBuf,
    pub static_dir: PathBuf,
    pub listen_addr: SocketAddr,
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
        Ok(Self { vault_path, static_dir, listen_addr })
    }

    pub fn from_env() -> Result<Self, String> {
        Self::from_lookup(|k| std::env::var(k).ok())
    }
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
}
