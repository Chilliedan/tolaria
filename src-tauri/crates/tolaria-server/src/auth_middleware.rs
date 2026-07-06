use crate::auth_routes::SESSION_COOKIE;
use crate::rpc::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::cookie::CookieJar;

/// Guards protected routes: forwards authenticated requests, otherwise returns
/// a 401 JSON error for `/api/*` paths or a 303 redirect to `/login` for pages.
pub async fn require_auth(
    State(state): State<AppState>,
    jar: CookieJar,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let authed = jar
        .get(SESSION_COOKIE)
        .and_then(|c| state.sessions.get(c.value()))
        .is_some();
    if authed {
        return next.run(request).await;
    }
    if request.uri().path().starts_with("/api/") {
        (
            StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({ "error": "not authenticated" })),
        )
            .into_response()
    } else {
        Redirect::to("/login").into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::SessionStore;
    use crate::users::UsersDb;
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::get;
    use std::time::Duration;
    use tower::ServiceExt;

    fn guarded_app(state: AppState) -> axum::Router {
        axum::Router::new()
            .route("/api/cmd/x", get(|| async { "ok" }))
            .route("/somepage", get(|| async { "page" }))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                require_auth,
            ))
            .with_state(state)
    }

    fn state_with_session() -> (AppState, String) {
        let state = AppState::new(
            std::path::PathBuf::from("/tmp"),
            UsersDb::open_in_memory().unwrap(),
            SessionStore::new(Duration::from_secs(60)),
            crate::locks::PathLocks::new(),
            crate::rpc::AppStateConfig {
                cookie_secure: false,
                committer_name: "Tolaria Server".to_string(),
                committer_email: "server@tolaria.local".to_string(),
                autogit: true,
            },
        );
        let token = state.sessions.create(1, "alice");
        (state, token)
    }

    #[tokio::test]
    async fn unauthenticated_api_is_401() {
        let (state, _t) = state_with_session();
        let resp = guarded_app(state)
            .oneshot(
                Request::builder()
                    .uri("/api/cmd/x")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn unauthenticated_page_redirects_to_login() {
        let (state, _t) = state_with_session();
        let resp = guarded_app(state)
            .oneshot(
                Request::builder()
                    .uri("/somepage")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            resp.headers().get("location").unwrap().to_str().unwrap(),
            "/login"
        );
    }

    #[tokio::test]
    async fn authenticated_request_passes() {
        let (state, token) = state_with_session();
        let resp = guarded_app(state)
            .oneshot(
                Request::builder()
                    .uri("/api/cmd/x")
                    .header("cookie", format!("{SESSION_COOKIE}={token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
