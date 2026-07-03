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

#[tokio::test]
async fn save_stale_basehash_returns_409_conflict_body() {
    let dir = tempdir().unwrap();
    let note = dir.path().join("n.md");
    fs::write(&note, "current-server-content").unwrap();
    let (app, token, csrf_token) = test_app(dir.path());

    // baseHash of content the client THINKS it edited from, but the server has
    // "current-server-content" on disk — a genuine stale conflict.
    let stale = tolaria_server::version::content_version("stale");
    let req = Request::builder()
        .method("POST")
        .uri("/api/cmd/save_note_content")
        .header("content-type", "application/json")
        .header("cookie", authed_cookie_header(&token, &csrf_token))
        .header("x-csrf-token", &csrf_token)
        .body(Body::from(
            json!({ "path": note, "content": "my-edit", "baseHash": stale }).to_string(),
        ))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["error"], "conflict");
    assert_eq!(v["currentContent"], "current-server-content");
    // The stale write must NOT have touched the file.
    assert_eq!(fs::read_to_string(&note).unwrap(), "current-server-content");
}

#[tokio::test]
async fn save_matching_basehash_succeeds_over_http() {
    let dir = tempdir().unwrap();
    let note = dir.path().join("n.md");
    fs::write(&note, "current-server-content").unwrap();
    let (app, token, csrf_token) = test_app(dir.path());

    let base = tolaria_server::version::content_version("current-server-content");
    let req = Request::builder()
        .method("POST")
        .uri("/api/cmd/save_note_content")
        .header("content-type", "application/json")
        .header("cookie", authed_cookie_header(&token, &csrf_token))
        .header("x-csrf-token", &csrf_token)
        .body(Body::from(
            json!({ "path": note, "content": "my-edit", "baseHash": base }).to_string(),
        ))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["version"], json!(tolaria_server::version::content_version("my-edit")));
    assert_eq!(fs::read_to_string(&note).unwrap(), "my-edit");
}

#[tokio::test]
async fn git_author_identity_over_http_returns_session_user() {
    let dir = tempdir().unwrap();
    let static_dir = dir.path().join("_static_not_used");
    let users = tolaria_server::users::UsersDb::open(&dir.path().join("users.db")).unwrap();
    users
        .create_user("dora", "pw-dora-1234", "Dora D", "dora@example.com")
        .unwrap();
    let user_id = users
        .verify_credentials("dora", "pw-dora-1234")
        .unwrap()
        .id;
    let sessions =
        tolaria_server::session::SessionStore::new(std::time::Duration::from_secs(60));
    let token = sessions.create(user_id, "dora");
    let csrf_token = tolaria_server::csrf::generate_token();
    let app = tolaria_server::build_router(
        dir.path().to_path_buf(),
        static_dir,
        users,
        sessions,
        false,
    );

    let req = Request::builder()
        .method("POST")
        .uri("/api/cmd/git_author_identity")
        .header("content-type", "application/json")
        .header("cookie", authed_cookie_header(&token, &csrf_token))
        .header("x-csrf-token", &csrf_token)
        .body(Body::from(json!({}).to_string()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        v,
        json!({
            "name": "Dora D",
            "email": "dora@example.com",
            "source": "web-session",
            "warning": null,
        })
    );
}

#[tokio::test]
async fn git_author_identity_without_session_is_401() {
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
        .uri("/api/cmd/git_author_identity")
        .header("content-type", "application/json")
        .body(Body::from(json!({}).to_string()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v, json!({ "error": "not authenticated" }));
}
