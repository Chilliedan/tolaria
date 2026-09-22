use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use std::fs;
use std::path::Path;
use tempfile::tempdir;
use tower::ServiceExt;

const PNG_BYTES: &[u8] = b"\x89PNG\r\n\x1a\nfake";

fn test_app(vault_dir: &Path) -> (axum::Router, String) {
    let users = tolaria_server::users::UsersDb::open(&vault_dir.join("users.db")).unwrap();
    let sessions = tolaria_server::session::SessionStore::new(std::time::Duration::from_secs(60));
    let token = sessions.create(1, "tester");
    let app = tolaria_server::build_router(
        vault_dir.to_path_buf(),
        vault_dir.join("_static_not_used"),
        users,
        sessions,
        false,
    );
    (app, token)
}

/// The web client's `convertFileSrc` shape: `/api/asset/` + the
/// percent-encoded absolute path (slashes included).
fn asset_uri(path: &Path) -> String {
    let encoded: String = path
        .to_string_lossy()
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect();
    format!("/api/asset/{encoded}")
}

async fn get(app: axum::Router, uri: &str, token: Option<&str>) -> axum::response::Response {
    let mut req = Request::builder().method("GET").uri(uri);
    if let Some(token) = token {
        req = req.header("cookie", format!("tolaria_session={token}"));
    }
    app.oneshot(req.body(Body::empty()).unwrap()).await.unwrap()
}

fn vault_with_attachments(dir: &Path) -> std::path::PathBuf {
    let attachments = dir.join("attachments");
    fs::create_dir_all(&attachments).unwrap();
    fs::write(attachments.join("my image.png"), PNG_BYTES).unwrap();
    fs::write(
        attachments.join("logo.svg"),
        "<svg xmlns='http://www.w3.org/2000/svg'/>",
    )
    .unwrap();
    fs::write(dir.join("secret.md"), "# Secret\n").unwrap();
    attachments
}

#[tokio::test]
async fn serves_vault_image_with_type_and_nosniff() {
    let dir = tempdir().unwrap();
    let attachments = vault_with_attachments(dir.path());
    let (app, token) = test_app(dir.path());

    let resp = get(
        app,
        &asset_uri(&attachments.join("my image.png")),
        Some(&token),
    )
    .await;

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers()[header::CONTENT_TYPE], "image/png");
    assert_eq!(resp.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    assert!(resp.headers()[header::CACHE_CONTROL]
        .to_str()
        .unwrap()
        .contains("private"));
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&bytes[..], PNG_BYTES);
}

#[tokio::test]
async fn svg_is_served_sandboxed() {
    let dir = tempdir().unwrap();
    let attachments = vault_with_attachments(dir.path());
    let (app, token) = test_app(dir.path());

    let resp = get(app, &asset_uri(&attachments.join("logo.svg")), Some(&token)).await;

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers()[header::CONTENT_TYPE], "image/svg+xml");
    let csp = resp.headers()[header::CONTENT_SECURITY_POLICY]
        .to_str()
        .unwrap();
    assert!(
        csp.contains("sandbox") && csp.contains("default-src 'none'"),
        "got: {csp}"
    );
}

#[tokio::test]
async fn requires_a_session() {
    let dir = tempdir().unwrap();
    let attachments = vault_with_attachments(dir.path());
    let (app, _token) = test_app(dir.path());

    let resp = get(app, &asset_uri(&attachments.join("my image.png")), None).await;

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn refuses_non_image_files_inside_the_vault() {
    let dir = tempdir().unwrap();
    vault_with_attachments(dir.path());
    let (app, token) = test_app(dir.path());

    let resp = get(app, &asset_uri(&dir.path().join("secret.md")), Some(&token)).await;

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn refuses_images_outside_the_vault() {
    let dir = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let outside_image = outside.path().join("elsewhere.png");
    fs::write(&outside_image, PNG_BYTES).unwrap();
    let (app, token) = test_app(dir.path());

    let resp = get(app, &asset_uri(&outside_image), Some(&token)).await;

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn refuses_traversal_out_of_the_vault() {
    let dir = tempdir().unwrap();
    let vault = dir.path().join("vault");
    fs::create_dir_all(vault.join("attachments")).unwrap();
    fs::write(dir.path().join("sibling.png"), PNG_BYTES).unwrap();
    let (app, token) = test_app(&vault);

    let escaped = vault
        .join("attachments")
        .join("..")
        .join("..")
        .join("sibling.png");
    let resp = get(app, &asset_uri(&escaped), Some(&token)).await;

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn missing_file_is_404() {
    let dir = tempdir().unwrap();
    let attachments = vault_with_attachments(dir.path());
    let (app, token) = test_app(dir.path());

    let resp = get(app, &asset_uri(&attachments.join("nope.png")), Some(&token)).await;

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
