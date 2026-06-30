use axum::http::StatusCode;
use std::path::PathBuf;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_status::SetStatus;

/// Serve `static_dir` with `index.html` as the SPA fallback for unknown paths.
pub fn service(static_dir: PathBuf) -> ServeDir<SetStatus<ServeFile>> {
    let index = static_dir.join("index.html");
    ServeDir::new(static_dir).not_found_service(ServeFile::new(index))
}

/// 404 JSON for unmatched API routes (so the SPA fallback never swallows them).
pub async fn api_not_found() -> (StatusCode, &'static str) {
    (StatusCode::NOT_FOUND, "not found")
}
