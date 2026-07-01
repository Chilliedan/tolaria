use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::json;
use std::fs;
use tempfile::tempdir;
use tower::ServiceExt;

/// Build a test app using the real `build_router` so the integration test
/// exercises the same routing logic as the production binary. Returns the
/// app plus a session token authorized to hit protected routes.
fn test_app(vault_dir: &std::path::Path) -> (axum::Router, String) {
    // Use a non-existent static dir; the tests never hit the SPA fallback.
    let static_dir = vault_dir.join("_static_not_used");
    let users =
        tolaria_server::users::UsersDb::open(&vault_dir.join("users.db")).unwrap();
    let sessions =
        tolaria_server::session::SessionStore::new(std::time::Duration::from_secs(60));
    let token = sessions.create(1, "tester");
    let app = tolaria_server::build_router(
        vault_dir.to_path_buf(),
        static_dir,
        users,
        sessions,
        false,
    );
    (app, token)
}

#[tokio::test]
async fn list_vault_over_http_returns_200_array() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("n.md"), "# A\n\nbody\n").unwrap();

    let (app, token) = test_app(dir.path());

    let req = Request::builder()
        .method("POST")
        .uri("/api/cmd/list_vault")
        .header("content-type", "application/json")
        .header("cookie", format!("tolaria_session={token}"))
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
    let (app, token) = test_app(dir.path());

    let req = Request::builder()
        .method("GET")
        .uri("/api/unknown/route")
        .header("cookie", format!("tolaria_session={token}"))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(v.get("error").is_some(), "JSON 404 body has an 'error' field");
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
