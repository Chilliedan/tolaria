use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;

use axum_extra::extract::cookie::CookieJar;
use tolaria_core::git::CommitIdentity;

use crate::locks::PathLocks;
use crate::session::SessionStore;
use crate::users::UsersDb;

/// Server-wide settings that do not vary per request: cookie transport
/// security and the fixed git committer identity/autogit toggle used for
/// server-authored commits. Grouped so `AppState::new` stays within
/// clippy's argument-count limit.
#[derive(Debug, Clone)]
pub struct AppStateConfig {
    pub cookie_secure: bool,
    pub committer_name: String,
    pub committer_email: String,
    pub autogit: bool,
}

/// Shared application state threaded through Axum handlers.
#[derive(Clone)]
pub struct AppState {
    pub vault_root: Arc<PathBuf>,
    pub users: UsersDb,
    pub sessions: SessionStore,
    pub cookie_secure: bool,
    pub locks: PathLocks,
    /// Serializes all index-mutating git operations across the vault.
    pub repo_lock: Arc<tokio::sync::Mutex<()>>,
    pub committer_name: Arc<str>,
    pub committer_email: Arc<str>,
    pub autogit: bool,
}

impl AppState {
    pub fn new(
        vault_root: PathBuf,
        users: UsersDb,
        sessions: SessionStore,
        locks: PathLocks,
        config: AppStateConfig,
    ) -> Self {
        Self {
            vault_root: Arc::new(vault_root),
            users,
            sessions,
            cookie_secure: config.cookie_secure,
            locks,
            repo_lock: Arc::new(tokio::sync::Mutex::new(())),
            committer_name: config.committer_name.into(),
            committer_email: config.committer_email.into(),
            autogit: config.autogit,
        }
    }

    /// Resolve the acting user's commit identity from the session cookie.
    /// Author = the user's git identity; committer = the fixed server
    /// identity. Returns `None` if unauthenticated — should not happen
    /// behind `require_auth`, but handled defensively.
    pub fn acting_user(&self, jar: &CookieJar) -> Option<CommitIdentity> {
        let session = jar
            .get(crate::auth_routes::SESSION_COOKIE)
            .and_then(|c| self.sessions.get(c.value()))?;
        let user = self.users.find_by_id(session.user_id)?;
        Some(CommitIdentity {
            author_name: user.git_name,
            author_email: user.git_email,
            committer_name: self.committer_name.to_string(),
            committer_email: self.committer_email.to_string(),
        })
    }
}

/// An RPC failure rendered as `{ "error": "..." }` with a status code.
#[derive(Debug)]
pub struct RpcError {
    pub status: StatusCode,
    pub message: String,
}

impl RpcError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }
}

/// Marker for a command the read-only web server does not implement.
pub fn unsupported(command: &str) -> RpcError {
    RpcError {
        status: StatusCode::NOT_IMPLEMENTED,
        message: format!("command '{command}' is not supported on web"),
    }
}

impl IntoResponse for RpcError {
    fn into_response(self) -> Response {
        (self.status, axum::Json(json!({ "error": self.message }))).into_response()
    }
}

/// Serialize a value as a 200 JSON response.
pub fn ok_json<T: Serialize>(value: T) -> Response {
    axum::Json(value).into_response()
}

/// Render a `409 CONFLICT` `RpcError` whose message is the JSON conflict body
/// (`{"error":"conflict","currentContent":...}`) produced by `write_handlers`.
fn conflict_response(message: &str) -> Response {
    let body: Value =
        serde_json::from_str(message).unwrap_or_else(|_| json!({ "error": "conflict" }));
    (StatusCode::CONFLICT, axum::Json(body)).into_response()
}

/// `POST /api/cmd/:command` — body is the JSON args object.
///
/// Write commands are routed to the async `write_handlers::dispatch_write`
/// (per-path locked, optimistic-concurrency checked); everything else uses
/// the synchronous read-only `handlers::dispatch`.
pub async fn command_route(
    State(state): State<AppState>,
    AxumPath(command): AxumPath<String>,
    jar: axum_extra::extract::cookie::CookieJar,
    headers: axum::http::HeaderMap,
    body: Option<Json<Value>>,
) -> Response {
    if !crate::csrf::verify(&jar, &headers) {
        return (StatusCode::FORBIDDEN, axum::Json(json!({ "error": "csrf" }))).into_response();
    }
    // Special-cased here (not in handlers::dispatch) because it reflects the
    // logged-in user's identity, which requires the session cookie jar.
    if command == "git_author_identity" {
        return match state.acting_user(&jar) {
            Some(id) => ok_json(json!({
                "name": id.author_name,
                "email": id.author_email,
                "source": "web-session",
                "warning": Value::Null,
            })),
            None => (
                StatusCode::UNAUTHORIZED,
                axum::Json(json!({ "error": "not authenticated" })),
            )
                .into_response(),
        };
    }
    let args = body.map(|Json(v)| v).unwrap_or(Value::Null);
    if crate::git_handlers::is_git_command(&command) {
        let Some(identity) = state.acting_user(&jar) else {
            return (
                StatusCode::UNAUTHORIZED,
                axum::Json(json!({ "error": "not authenticated" })),
            )
                .into_response();
        };
        return match crate::git_handlers::dispatch_git(&state, &identity, &command, args).await {
            Ok(value) => ok_json(value),
            Err(err) => err.into_response(),
        };
    }
    let result = if crate::write_handlers::is_write_command(&command) {
        let identity = state.acting_user(&jar);
        crate::write_handlers::dispatch_write(&state, identity.as_ref(), &command, args).await
    } else {
        crate::handlers::dispatch(&state.vault_root, &command, args)
    };
    match result {
        Ok(value) => ok_json(value),
        Err(err) if err.status == StatusCode::CONFLICT => conflict_response(&err.message),
        Err(err) => err.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::SessionStore;
    use crate::users::UsersDb;
    use axum_extra::extract::cookie::Cookie;
    use std::time::Duration;

    #[test]
    fn unsupported_is_501_with_message() {
        let err = unsupported("save_note_content");
        assert_eq!(err.status, StatusCode::NOT_IMPLEMENTED);
        assert!(err.message.contains("save_note_content"));
        assert!(err.message.contains("not supported on web"));
    }

    fn state_for_identity() -> (AppState, String) {
        let users = UsersDb::open_in_memory().unwrap();
        users
            .create_user("dora", "pw-dora-1234", "Dora D", "dora@example.com")
            .unwrap();
        let rec = users.verify_credentials("dora", "pw-dora-1234").unwrap();
        let sessions = SessionStore::new(Duration::from_secs(60));
        let token = sessions.create(rec.id, "dora");
        let state = AppState::new(
            std::path::PathBuf::from("/tmp"),
            users,
            sessions,
            PathLocks::new(),
            AppStateConfig {
                cookie_secure: false,
                committer_name: "Tolaria Server".to_string(),
                committer_email: "server@tolaria.local".to_string(),
                autogit: true,
            },
        );
        (state, token)
    }

    #[test]
    fn acting_user_resolves_author_from_session() {
        let (state, token) = state_for_identity();
        let jar = CookieJar::new().add(Cookie::new(crate::auth_routes::SESSION_COOKIE, token));
        let id = state.acting_user(&jar).expect("resolves");
        assert_eq!(id.author_name, "Dora D");
        assert_eq!(id.author_email, "dora@example.com");
        assert_eq!(id.committer_name, "Tolaria Server");
        assert_eq!(id.committer_email, "server@tolaria.local");
    }

    #[test]
    fn acting_user_none_without_session() {
        let (state, _t) = state_for_identity();
        assert!(state.acting_user(&CookieJar::new()).is_none());
    }
}
