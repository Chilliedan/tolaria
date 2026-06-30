use axum::http::StatusCode;
use axum::response::IntoResponse;
use std::path::PathBuf;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_status::SetStatus;

/// Serve `static_dir` with `index.html` as the SPA fallback for unknown paths.
pub fn service(static_dir: PathBuf) -> ServeDir<SetStatus<ServeFile>> {
    let index = static_dir.join("index.html");
    ServeDir::new(static_dir).not_found_service(ServeFile::new(index))
}

/// JSON 404 for unmatched `/api/...` routes so the SPA fallback never swallows them.
pub(crate) async fn api_not_found() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, axum::Json(serde_json::json!({ "error": "not found" })))
}
