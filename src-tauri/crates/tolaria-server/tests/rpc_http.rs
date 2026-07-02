use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::json;
use std::fs;
use tempfile::tempdir;
use tower::ServiceExt;

/// Build a test app using the real `build_router` so the integration test
/// exercises the same routing logic as the production binary. Returns the
/// app, a session token, and a csrf token — both authorized to hit
/// protected routes (session cookie + matching csrf cookie/header).
fn test_app(vault_dir: &std::path::Path) -> (axum::Router, String, String) {
    // Use a non-existent static dir; the tests never hit the SPA fallback.
    let static_dir = vault_dir.join("_static_not_used");
    let users =
        tolaria_server::users::UsersDb::open(&vault_dir.join("users.db")).unwrap();
    let sessions =
        tolaria_server::session::SessionStore::new(std::time::Duration::from_secs(60));
    let token = sessions.create(1, "tester");
    let csrf_token = tolaria_server::csrf::generate_token();
    let app = tolaria_server::build_router(
        vault_dir.to_path_buf(),
        static_dir,
        users,
        sessions,
        false,
    );
    (app, token, csrf_token)
}

/// `cookie` header value carrying both the session and csrf cookies.
fn authed_cookie_header(token: &str, csrf_token: &str) -> String {
    format!("tolaria_session={token}; tolaria_csrf={csrf_token}")
}

#[tokio::test]
async fn list_vault_over_http_returns_200_array() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("n.md"), "# A\n\nbody\n").unwrap();

    let (app, token, csrf_token) = test_app(dir.path());

    let req = Request::builder()
        .method("POST")
        .uri("/api/cmd/list_vault")
        .header("content-type", "application/json")
        .header("cookie", authed_cookie_header(&token, &csrf_token))
        .header("x-csrf-token", &csrf_token)
        .body(Body::from(json!({}).to_string()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(v.is_array());
}

#[tokio::test]
async fn unmatched_api_route_returns_json_404() {
    let dir = tempdir().unwrap();
    let (app, token, csrf_token) = test_app(dir.path());

    let req = Request::builder()
        .method("GET")
        .uri("/api/unknown/route")
        .header("cookie", authed_cookie_header(&token, &csrf_token))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(v.get("error").is_some(), "JSON 404 body has an 'error' field");
}

#[tokio::test]
async fn command_route_without_csrf_header_is_403() {
    let dir = tempdir().unwrap();
    let (app, token, csrf_token) = test_app(dir.path());

    let req = Request::builder()
        .method("POST")
        .uri("/api/cmd/list_vault")
        .header("content-type", "application/json")
        // Cookie present, but no X-CSRF-Token header echoing it back.
        .header("cookie", authed_cookie_header(&token, &csrf_token))
        .body(Body::from(json!({}).to_string()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["error"], "csrf");
}

#[tokio::test]
async fn command_route_with_mismatched_csrf_header_is_403() {
    let dir = tempdir().unwrap();
    let (app, token, csrf_token) = test_app(dir.path());

    let req = Request::builder()
        .method("POST")
        .uri("/api/cmd/list_vault")
        .header("content-type", "application/json")
        .header("cookie", authed_cookie_header(&token, &csrf_token))
        .header("x-csrf-token", "not-the-right-token")
        .body(Body::from(json!({}).to_string()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn command_route_with_matching_csrf_is_200() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("n.md"), "# A\n\nbody\n").unwrap();
    let (app, token, csrf_token) = test_app(dir.path());

    let req = Request::builder()
        .method("POST")
        .uri("/api/cmd/list_vault")
        .header("content-type", "application/json")
        .header("cookie", authed_cookie_header(&token, &csrf_token))
        .header("x-csrf-token", &csrf_token)
        .body(Body::from(json!({}).to_string()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn list_vault_without_session_is_401() {
    let dir = tempdir().unwrap();
    let users = tolaria_server::users::UsersDb::open(&dir.path().join("users.db")).unwrap();
    let sessions =
        tolaria_server::session::SessionStore::new(std::time::Duration::from_secs(60));
    let app = tolaria_server::build_router(
        dir.path().to_path_buf(),
        dir.path().to_path_buf(),
        users,
        sessions,
        false,
    );
    let req = Request::builder()
        .method("POST")
        .uri("/api/cmd/list_vault")
        .header("content-type", "application/json")
        .body(Body::from("{\"path\":\"/\"}"))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
