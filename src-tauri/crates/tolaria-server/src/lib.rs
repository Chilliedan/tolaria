//! Tolaria web server library — re-exports all modules for integration testing.

pub mod auth_middleware;
pub mod auth_routes;
pub mod config;
pub mod csrf;
pub mod git_handlers;
pub mod handlers;
pub mod locks;
pub mod rpc;
pub mod session;
pub mod static_files;
pub mod users;
pub mod version;
pub mod write_handlers;

use axum::routing::post;
use axum::Router;
use std::path::PathBuf;

use crate::locks::PathLocks;
use crate::session::SessionStore;
use crate::users::UsersDb;

/// Build the full application router.
///
/// `vault_root` is the directory the server is allowed to read.
/// `static_dir` is the compiled SPA directory served as the fallback.
///
/// Public routes (`GET /login`, `POST /api/auth/login`) are reachable without
/// a session. Everything else — the RPC surface, auth/me, auth/logout, the
/// JSON 404 catch-all, and the SPA fallback — is behind `require_auth`.
pub fn build_router(
    vault_root: PathBuf,
    static_dir: PathBuf,
    users: UsersDb,
    sessions: SessionStore,
    cookie_secure: bool,
) -> Router {
    let (committer_name, committer_email) = config::committer_identity();
    let state = rpc::AppState::new(
        vault_root,
        users,
        sessions,
        PathLocks::new(),
        rpc::AppStateConfig {
            cookie_secure,
            committer_name,
            committer_email,
            autogit: config::autogit_enabled(),
        },
    );

    // Public routes: reachable without a session.
    let public = Router::new()
        .route("/login", axum::routing::get(auth_routes::login_page))
        .route("/api/auth/login", post(auth_routes::login));

    // Protected routes: require a valid session (via require_auth layer).
    let protected = Router::new()
        .route("/api/cmd/:command", post(rpc::command_route))
        .route("/api/auth/logout", post(auth_routes::logout))
        .route("/api/auth/me", axum::routing::get(auth_routes::me))
        .route("/api/*path", axum::routing::any(static_files::api_not_found))
        .fallback_service(static_files::service(static_dir))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth_middleware::require_auth,
        ));

    public.merge(protected).with_state(state)
}
