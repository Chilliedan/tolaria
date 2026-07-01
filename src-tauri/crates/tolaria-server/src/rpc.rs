use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;

use crate::session::SessionStore;
use crate::users::UsersDb;

/// Shared application state threaded through Axum handlers.
#[derive(Clone)]
pub struct AppState {
    pub vault_root: Arc<PathBuf>,
    pub users: UsersDb,
    pub sessions: SessionStore,
    pub cookie_secure: bool,
}

impl AppState {
    pub fn new(
        vault_root: PathBuf,
        users: UsersDb,
        sessions: SessionStore,
        cookie_secure: bool,
    ) -> Self {
        Self {
            vault_root: Arc::new(vault_root),
            users,
            sessions,
            cookie_secure,
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

/// `POST /api/cmd/:command` — body is the JSON args object.
pub async fn command_route(
    State(state): State<AppState>,
    AxumPath(command): AxumPath<String>,
    body: Option<Json<Value>>,
) -> Response {
    let args = body.map(|Json(v)| v).unwrap_or(Value::Null);
    match crate::handlers::dispatch(&state.vault_root, &command, args) {
        Ok(value) => ok_json(value),
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
