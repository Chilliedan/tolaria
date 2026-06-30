use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::json;
use std::fs;
use tempfile::tempdir;
use tower::ServiceExt;

/// Build a test app using the real `build_router` so the integration test
/// exercises the same routing logic as the production binary.
fn test_app(vault_dir: &std::path::Path) -> axum::Router {
    // Use a non-existent static dir; the tests never hit the SPA fallback.
    let static_dir = vault_dir.join("_static_not_used");
    tolaria_server::build_router(vault_dir.to_path_buf(), static_dir)
}

#[tokio::test]
async fn list_vault_over_http_returns_200_array() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("n.md"), "# A\n\nbody\n").unwrap();

    let app = test_app(dir.path());

    let req = Request::builder()
        .method("POST")
        .uri("/api/cmd/list_vault")
        .header("content-type", "application/json")
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
    let app = test_app(dir.path());

    let req = Request::builder()
        .method("GET")
        .uri("/api/unknown/route")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(v.get("error").is_some(), "JSON 404 body has an 'error' field");
}
