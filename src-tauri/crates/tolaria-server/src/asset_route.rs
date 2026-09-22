//! `GET /api/asset/*path` — serves vault image attachments to the browser.
//!
//! The web client's `convertFileSrc` maps an absolute vault file path to
//! `/api/asset/<percent-encoded path>` (the same shape as Tauri's
//! `asset://localhost/<encoded>`), so `<img>` tags load through this route
//! with the session cookie. Only image types are served, only from inside
//! the vault; every refusal is a plain 404 so the route cannot be used to
//! probe for files.

use axum::extract::{Path as AxumPath, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use std::path::{Path, PathBuf};

use crate::rpc::AppState;

/// Scripts in an SVG opened directly (not via `<img>`) must not run with the
/// app's origin, so SVGs are served under a no-script sandbox policy.
const SVG_POLICY: &str = "default-src 'none'; style-src 'unsafe-inline'; sandbox";

fn image_content_type(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    let content_type = match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "svg" => "image/svg+xml",
        _ => return None,
    };
    Some(content_type)
}

/// Canonicalize `requested` and return it only if it is a file inside the
/// vault. Canonicalizing first resolves `..` and symlinks before the check.
fn contained_file(vault_root: &Path, requested: &Path) -> Option<PathBuf> {
    let root = std::fs::canonicalize(vault_root).ok()?;
    let full = std::fs::canonicalize(requested).ok()?;
    (full.starts_with(&root) && full.is_file()).then_some(full)
}

fn not_found() -> Response {
    StatusCode::NOT_FOUND.into_response()
}

fn image_response(bytes: Vec<u8>, content_type: &'static str) -> Response {
    let mut response = bytes.into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-cache"),
    );
    if content_type == "image/svg+xml" {
        headers.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(SVG_POLICY),
        );
    }
    response
}

pub(crate) async fn serve_asset(
    State(state): State<AppState>,
    AxumPath(requested): AxumPath<String>,
) -> Response {
    let requested = PathBuf::from(format!("/{}", requested.trim_start_matches('/')));
    let Some(content_type) = image_content_type(&requested) else {
        return not_found();
    };
    let vault_root = state.vault_root.clone();
    let read = tokio::task::spawn_blocking(move || {
        contained_file(&vault_root, &requested).and_then(|path| std::fs::read(path).ok())
    })
    .await;
    match read {
        Ok(Some(bytes)) => image_response(bytes, content_type),
        _ => not_found(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_image_extensions_have_a_content_type() {
        assert_eq!(image_content_type(Path::new("/v/a.PNG")), Some("image/png"));
        assert_eq!(
            image_content_type(Path::new("/v/a.jpeg")),
            Some("image/jpeg")
        );
        assert_eq!(
            image_content_type(Path::new("/v/a.svg")),
            Some("image/svg+xml")
        );
        assert_eq!(image_content_type(Path::new("/v/a.md")), None);
        assert_eq!(image_content_type(Path::new("/v/users.db")), None);
        assert_eq!(image_content_type(Path::new("/v/noext")), None);
    }

    #[test]
    fn contained_file_rejects_directories_and_escapes() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        std::fs::create_dir_all(vault.join("attachments")).unwrap();
        std::fs::write(vault.join("attachments/a.png"), b"x").unwrap();
        std::fs::write(dir.path().join("b.png"), b"x").unwrap();

        assert!(contained_file(&vault, &vault.join("attachments/a.png")).is_some());
        assert!(contained_file(&vault, &vault.join("attachments")).is_none());
        assert!(contained_file(&vault, &vault.join("attachments/../../b.png")).is_none());
    }
}
