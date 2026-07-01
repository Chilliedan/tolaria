//! Tolaria web server library — re-exports all modules for integration testing.

pub mod auth_routes;
pub mod config;
pub mod handlers;
pub mod rpc;
pub mod session;
pub mod static_files;
pub mod users;

use axum::routing::post;
use axum::Router;
use std::path::PathBuf;

/// Build the Axum router for the Tolaria web server.
///
/// `vault_root` is the directory the server is allowed to read.
/// `static_dir` is the compiled SPA directory served as the fallback.
pub fn build_router(vault_root: PathBuf, static_dir: PathBuf) -> Router {
    let users = crate::users::UsersDb::open(std::path::Path::new(":memory:")).expect("users db");
    let sessions =
        crate::session::SessionStore::new(std::time::Duration::from_secs(60 * 60 * 24 * 7));
    let state = rpc::AppState::new(vault_root, users, sessions, true);
    Router::new()
        .route("/api/cmd/:command", post(rpc::command_route))
        .route(
            "/api/*path",
            axum::routing::any(static_files::api_not_found),
        )
        .with_state(state)
        .fallback_service(static_files::service(static_dir))
}
