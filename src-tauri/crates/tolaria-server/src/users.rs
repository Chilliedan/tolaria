use argon2::password_hash::{
    rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use argon2::Argon2;
use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// A user account (password hash intentionally excluded).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserRecord {
    pub id: i64,
    pub username: String,
    pub git_name: String,
    pub git_email: String,
}

/// Schema for the `users` table, shared by the persistent and in-memory
/// stores so the column set can never drift between them.
const CREATE_USERS_SQL: &str = "CREATE TABLE IF NOT EXISTS users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    git_name TEXT NOT NULL,
    git_email TEXT NOT NULL
)";

/// SQLite-backed user store. Cheap to clone (shares one connection).
#[derive(Clone)]
pub struct UsersDb {
    conn: Arc<Mutex<Connection>>,
}

impl UsersDb {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("users db dir: {e}"))?;
        }
        let conn = Connection::open(path).map_err(|e| format!("open users db: {e}"))?;
        conn.execute(CREATE_USERS_SQL, [])
            .map_err(|e| format!("create users table: {e}"))?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Open an in-memory store for tests.
    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory().map_err(|e| e.to_string())?;
        conn.execute(CREATE_USERS_SQL, [])
            .map_err(|e| e.to_string())?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn create_user(
        &self,
        username: &str,
        password: &str,
        git_name: &str,
        git_email: &str,
    ) -> Result<(), String> {
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| format!("hash: {e}"))?
            .to_string();
        let conn = self
            .conn
            .lock()
            .map_err(|_| "users db poisoned".to_string())?;
        conn.execute(
            "INSERT INTO users (username, password_hash, git_name, git_email) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![username, hash, git_name, git_email],
        )
        .map_err(|e| format!("insert user: {e}"))?;
        Ok(())
    }

    pub fn verify_credentials(&self, username: &str, password: &str) -> Option<UserRecord> {
        let conn = self.conn.lock().ok()?;
        let mut stmt = conn
            .prepare("SELECT id, username, password_hash, git_name, git_email FROM users WHERE username = ?1")
            .ok()?;
        let row = stmt
            .query_row(rusqlite::params![username], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })
            .ok()?;
        let (id, username, password_hash, git_name, git_email) = row;
        let parsed = PasswordHash::new(&password_hash).ok()?;
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .ok()?;
        Some(UserRecord {
            id,
            username,
            git_name,
            git_email,
        })
    }

    /// Look up a user's public record (including git identity) by id. Returns
    /// `None` if no such user exists.
    pub fn find_by_id(&self, id: i64) -> Option<UserRecord> {
        let conn = self.conn.lock().ok()?;
        conn.query_row(
            "SELECT id, username, git_name, git_email FROM users WHERE id = ?1",
            rusqlite::params![id],
            |r| {
                Ok(UserRecord {
                    id: r.get(0)?,
                    username: r.get(1)?,
                    git_name: r.get(2)?,
                    git_email: r.get(3)?,
                })
            },
        )
        .ok()
    }
}

/// Create a user, rejecting empty inputs. Used by the `useradd` CLI.
pub fn run_useradd(
    users: &UsersDb,
    username: &str,
    password: &str,
    git_name: &str,
    git_email: &str,
) -> Result<(), String> {
    if username.trim().is_empty() || password.is_empty() || git_email.trim().is_empty() {
        return Err("username, password, and git email must not be empty".to_string());
    }
    users.create_user(username, password, git_name, git_email)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_then_verify_correct_password() {
        let db = UsersDb::open_in_memory().unwrap();
        db.create_user("alice", "s3cret", "Alice", "alice@example.com")
            .unwrap();
        let user = db
            .verify_credentials("alice", "s3cret")
            .expect("correct password verifies");
        assert_eq!(user.username, "alice");
        assert_eq!(user.git_email, "alice@example.com");
    }

    #[test]
    fn wrong_password_rejected() {
        let db = UsersDb::open_in_memory().unwrap();
        db.create_user("bob", "right", "Bob", "bob@example.com")
            .unwrap();
        assert!(db.verify_credentials("bob", "wrong").is_none());
    }

    #[test]
    fn unknown_user_rejected() {
        let db = UsersDb::open_in_memory().unwrap();
        assert!(db.verify_credentials("nobody", "x").is_none());
    }

    #[test]
    fn duplicate_username_errors() {
        let db = UsersDb::open_in_memory().unwrap();
        db.create_user("carol", "p", "Carol", "c@example.com")
            .unwrap();
        assert!(db
            .create_user("carol", "p2", "Carol2", "c2@example.com")
            .is_err());
    }

    #[test]
    fn password_is_hashed_not_plaintext() {
        let db = UsersDb::open_in_memory().unwrap();
        db.create_user("dave", "plaintextpw", "Dave", "d@example.com")
            .unwrap();
        let conn = db.conn.lock().unwrap();
        let stored: String = conn
            .query_row(
                "SELECT password_hash FROM users WHERE username='dave'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            stored.starts_with("$argon2"),
            "stored hash must be argon2 PHC string"
        );
        assert!(!stored.contains("plaintextpw"));
    }

    #[test]
    fn run_useradd_creates_verifiable_user() {
        let db = UsersDb::open_in_memory().unwrap();
        run_useradd(&db, "eve", "pw12345", "Eve", "eve@example.com").unwrap();
        assert!(db.verify_credentials("eve", "pw12345").is_some());
    }

    #[test]
    fn run_useradd_rejects_empty_password() {
        let db = UsersDb::open_in_memory().unwrap();
        assert!(run_useradd(&db, "eve", "", "Eve", "eve@example.com").is_err());
    }

    #[test]
    fn run_useradd_rejects_empty_git_email() {
        let db = UsersDb::open_in_memory().unwrap();
        assert!(run_useradd(&db, "eve", "pw", "Eve", "").is_err());
    }

    #[test]
    fn find_by_id_returns_git_identity() {
        let db = UsersDb::open_in_memory().unwrap();
        db.create_user("carol", "pw-carol-123", "Carol Q", "carol@example.com")
            .unwrap();
        let created = db.verify_credentials("carol", "pw-carol-123").unwrap();

        let found = db.find_by_id(created.id).expect("user exists");
        assert_eq!(found.username, "carol");
        assert_eq!(found.git_name, "Carol Q");
        assert_eq!(found.git_email, "carol@example.com");

        assert!(db.find_by_id(9999).is_none());
    }
}
