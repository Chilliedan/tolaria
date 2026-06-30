use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::json;
use std::fs;
use tower::ServiceExt;

// Re-declare a tiny router using the crate's public router builder is not exposed;
// instead test the command route through a minimal Router mirroring main.rs.
#[tokio::test]
async fn list_vault_over_http_returns_200_array() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("n.md"), "# A\n\nbody\n").unwrap();

    let app = axum::Router::new().route(
        "/api/cmd/:command",
        axum::routing::post(tolaria_server::rpc::command_route),
    );

    let req = Request::builder()
        .method("POST")
        .uri("/api/cmd/list_vault")
        .header("content-type", "application/json")
        .body(Body::from(json!({ "path": dir.path() }).to_string()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(v.is_array());
}
