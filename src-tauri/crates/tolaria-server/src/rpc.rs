use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;

use crate::locks::PathLocks;
use crate::session::SessionStore;
use crate::users::UsersDb;

/// Shared application state threaded through Axum handlers.
#[derive(Clone)]
pub struct AppState {
    pub vault_root: Arc<PathBuf>,
    pub users: UsersDb,
    pub sessions: SessionStore,
    pub cookie_secure: bool,
    pub locks: PathLocks,
}

impl AppState {
    pub fn new(
        vault_root: PathBuf,
        users: UsersDb,
        sessions: SessionStore,
        cookie_secure: bool,
        locks: PathLocks,
    ) -> Self {
        Self {
            vault_root: Arc::new(vault_root),
            users,
            sessions,
            cookie_secure,
            locks,
        }
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
    body: Option<Json<Value>>,
) -> Response {
    let args = body.map(|Json(v)| v).unwrap_or(Value::Null);
    let result = if crate::write_handlers::is_write_command(&command) {
        crate::write_handlers::dispatch_write(&state, &command, args).await
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

    #[test]
    fn unsupported_is_501_with_message() {
        let err = unsupported("save_note_content");
        assert_eq!(err.status, StatusCode::NOT_IMPLEMENTED);
        assert!(err.message.contains("save_note_content"));
        assert!(err.message.contains("not supported on web"));
    }
}
