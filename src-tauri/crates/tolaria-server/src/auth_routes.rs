//! Auth HTTP routes: server-rendered login page, login/logout, and session check.

use crate::rpc::AppState;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::Form;
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use serde::Deserialize;
use serde_json::json;

pub const SESSION_COOKIE: &str = "tolaria_session";

#[derive(Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct LoginQuery {
    pub error: Option<String>,
}

pub async fn login_page(Query(q): Query<LoginQuery>) -> Html<String> {
    let error_html = if q.error.is_some() {
        r#"<p class="err">Invalid username or password.</p>"#
    } else {
        ""
    };
    Html(format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><title>Tolaria — Sign in</title>
<meta name="viewport" content="width=device-width, initial-scale=1">
<style>body{{font-family:system-ui,sans-serif;background:#111;color:#eee;display:flex;min-height:100vh;align-items:center;justify-content:center;margin:0}}
form{{background:#1c1c1c;padding:2rem;border-radius:10px;width:300px}}
h1{{font-size:1.1rem;margin:0 0 1rem}} label{{display:block;font-size:.8rem;margin:.6rem 0 .2rem}}
input{{width:100%;padding:.5rem;border-radius:6px;border:1px solid #333;background:#111;color:#eee;box-sizing:border-box}}
button{{margin-top:1rem;width:100%;padding:.55rem;border:0;border-radius:6px;background:#4f46e5;color:#fff;font-weight:600;cursor:pointer}}
.err{{color:#f87171;font-size:.8rem}}</style></head>
<body><form method="post" action="/api/auth/login"><h1>Tolaria</h1>{error_html}
<label for="u">Username</label><input id="u" name="username" autocomplete="username" autofocus>
<label for="p">Password</label><input id="p" name="password" type="password" autocomplete="current-password">
<button type="submit">Sign in</button></form></body></html>"#
    ))
}

fn session_cookie(value: String, secure: bool) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, value))
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .path("/")
        .build()
}

pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    Form(form): Form<LoginForm>,
) -> Response {
    match state
        .users
        .verify_credentials(&form.username, &form.password)
    {
        Some(user) => {
            let token = state.sessions.create(user.id, &user.username);
            let jar = jar
                .add(session_cookie(token, state.cookie_secure))
                .add(crate::csrf::csrf_cookie(
                    crate::csrf::generate_token(),
                    state.cookie_secure,
                ));
            (jar, Redirect::to("/")).into_response()
        }
        None => Redirect::to("/login?error=1").into_response(),
    }
}

pub async fn logout(State(state): State<AppState>, jar: CookieJar) -> Response {
    if let Some(c) = jar.get(SESSION_COOKIE) {
        state.sessions.remove(c.value());
    }
    let jar = jar.remove(Cookie::build((SESSION_COOKIE, "")).path("/").build());
    (jar, Redirect::to("/login")).into_response()
}

pub async fn me(State(state): State<AppState>, jar: CookieJar) -> Response {
    let session = jar
        .get(SESSION_COOKIE)
        .and_then(|c| state.sessions.get(c.value()));
    match session {
        Some(s) => (
            StatusCode::OK,
            axum::Json(json!({ "username": s.username })),
        )
            .into_response(),
        None => (
            StatusCode::UNAUTHORIZED,
            axum::Json(json!({ "error": "not authenticated" })),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::SessionStore;
    use crate::users::UsersDb;
    use axum::body::Body;
    use axum::http::Request;
    use std::time::Duration;
    use tower::ServiceExt;

    fn test_state() -> AppState {
        let users = UsersDb::open_in_memory().unwrap();
        users
            .create_user("alice", "s3cret", "Alice", "a@example.com")
            .unwrap();
        AppState::new(
            std::path::PathBuf::from("/tmp"),
            users,
            SessionStore::new(Duration::from_secs(60)),
            false,
            crate::locks::PathLocks::new(),
        )
    }

    fn app(state: AppState) -> axum::Router {
        axum::Router::new()
            .route("/api/auth/login", axum::routing::post(login))
            .route("/api/auth/me", axum::routing::get(me))
            .with_state(state)
    }

    #[tokio::test]
    async fn good_login_sets_cookie_and_redirects() {
        let resp = app(test_state())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/login")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from("username=alice&password=s3cret"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
        let set_cookies: Vec<&str> = resp
            .headers()
            .get_all("set-cookie")
            .iter()
            .map(|v| v.to_str().unwrap())
            .collect();
        let session_cookie = set_cookies
            .iter()
            .find(|c| c.contains("tolaria_session="))
            .expect("session cookie set");
        assert!(session_cookie.contains("HttpOnly"));

        let csrf_cookie = set_cookies
            .iter()
            .find(|c| c.contains("tolaria_csrf="))
            .expect("csrf cookie set");
        assert!(!csrf_cookie.contains("HttpOnly"));
    }

    #[tokio::test]
    async fn bad_login_redirects_to_error() {
        let resp = app(test_state())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/login")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from("username=alice&password=WRONG"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
        let loc = resp.headers().get("location").unwrap().to_str().unwrap();
        assert_eq!(loc, "/login?error=1");
        assert!(resp.headers().get("set-cookie").is_none());
    }

    #[tokio::test]
    async fn me_without_cookie_is_401() {
        let resp = app(test_state())
            .oneshot(
                Request::builder()
                    .uri("/api/auth/me")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
