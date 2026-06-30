use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::json;

/// An RPC failure rendered as `{ "error": "..." }` with a status code.
#[derive(Debug)]
pub struct RpcError {
    pub status: StatusCode,
    pub message: String,
}

impl RpcError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self { status: StatusCode::BAD_REQUEST, message: message.into() }
    }
    pub fn internal(message: impl Into<String>) -> Self {
        Self { status: StatusCode::INTERNAL_SERVER_ERROR, message: message.into() }
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
