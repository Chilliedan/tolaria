//! In-memory session store for authenticated web-server sessions.
//!
//! Sessions are keyed by an opaque, randomly generated token (never a
//! predictable identifier) and expire after a fixed TTL from creation.

use rand::rngs::OsRng;
use rand::RngCore;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// A resolved, still-valid session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub user_id: i64,
    pub username: String,
}

struct StoredSession {
    user_id: i64,
    username: String,
    expires_at: Instant,
}

/// In-memory session store keyed by opaque token. Cheap to clone.
#[derive(Clone)]
pub struct SessionStore {
    inner: Arc<RwLock<HashMap<String, StoredSession>>>,
    ttl: Duration,
}

impl SessionStore {
    pub fn new(ttl: Duration) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            ttl,
        }
    }

    fn new_token() -> String {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    pub fn create(&self, user_id: i64, username: &str) -> String {
        let token = Self::new_token();
        let stored = StoredSession {
            user_id,
            username: username.to_string(),
            expires_at: Instant::now() + self.ttl,
        };
        self.inner
            .write()
            .expect("session store poisoned")
            .insert(token.clone(), stored);
        token
    }

    pub fn get(&self, token: &str) -> Option<Session> {
        // Fast path: read lock.
        {
            let map = self.inner.read().ok()?;
            match map.get(token) {
                Some(s) if s.expires_at > Instant::now() => {
                    return Some(Session {
                        user_id: s.user_id,
                        username: s.username.clone(),
                    });
                }
                Some(_) => {} // expired: fall through to remove
                None => return None,
            }
        }
        // Expired: drop it under a write lock.
        self.inner.write().ok()?.remove(token);
        None
    }

    pub fn remove(&self, token: &str) {
        if let Ok(mut map) = self.inner.write() {
            map.remove(token);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_then_get_returns_session() {
        let store = SessionStore::new(Duration::from_secs(60));
        let token = store.create(7, "alice");
        let session = store.get(&token).expect("valid session");
        assert_eq!(session.user_id, 7);
        assert_eq!(session.username, "alice");
    }

    #[test]
    fn unknown_token_returns_none() {
        let store = SessionStore::new(Duration::from_secs(60));
        assert!(store.get("deadbeef").is_none());
    }

    #[test]
    fn removed_session_is_gone() {
        let store = SessionStore::new(Duration::from_secs(60));
        let token = store.create(1, "bob");
        store.remove(&token);
        assert!(store.get(&token).is_none());
    }

    #[test]
    fn expired_session_returns_none() {
        let store = SessionStore::new(Duration::from_millis(0));
        let token = store.create(1, "bob");
        std::thread::sleep(Duration::from_millis(5));
        assert!(store.get(&token).is_none());
    }

    #[test]
    fn tokens_are_unique_and_long() {
        let store = SessionStore::new(Duration::from_secs(60));
        let a = store.create(1, "x");
        let b = store.create(1, "x");
        assert_ne!(a, b);
        assert_eq!(a.len(), 64); // 32 bytes hex
    }
}
