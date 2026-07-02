use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

/// Registry of per-path async mutexes so writes to the same file serialize
/// while writes to different files run concurrently. Cheap to clone.
#[derive(Clone, Default)]
pub struct PathLocks {
    inner: Arc<Mutex<HashMap<PathBuf, Arc<AsyncMutex<()>>>>>,
}

impl PathLocks {
    pub fn new() -> Self {
        Self::default()
    }

    fn mutex_for(&self, path: &Path) -> Arc<AsyncMutex<()>> {
        let mut map = self.inner.lock().expect("path locks poisoned");
        map.entry(path.to_path_buf())
            .or_insert_with(|| Arc::new(AsyncMutex::new(())))
            .clone()
    }

    /// Acquire the lock for `path`, awaiting if another writer holds it.
    pub async fn lock(&self, path: &Path) -> OwnedMutexGuard<()> {
        self.mutex_for(path).lock_owned().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_path_shares_one_mutex() {
        let locks = PathLocks::new();
        let a = locks.mutex_for(Path::new("/v/n.md"));
        let b = locks.mutex_for(Path::new("/v/n.md"));
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn different_paths_independent() {
        let locks = PathLocks::new();
        let a = locks.mutex_for(Path::new("/v/a.md"));
        let b = locks.mutex_for(Path::new("/v/b.md"));
        assert!(!Arc::ptr_eq(&a, &b));
    }

    #[tokio::test]
    async fn lock_then_release_allows_reacquire() {
        let locks = PathLocks::new();
        let g = locks.lock(Path::new("/v/n.md")).await;
        drop(g);
        let _g2 = locks.lock(Path::new("/v/n.md")).await; // must not deadlock
    }
}
