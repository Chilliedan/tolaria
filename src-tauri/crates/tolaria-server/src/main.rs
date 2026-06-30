//! Tolaria web server: serves the SPA and a read-only vault RPC over tolaria-core.

use axum::routing::post;
use axum::Router;
use tolaria_server::{config, rpc, static_files};

fn build_router(cfg: &config::ServerConfig) -> Router {
    Router::new()
        .route("/api/cmd/:command", post(rpc::command_route))
        .fallback_service(static_files::service(cfg.static_dir.clone()))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "tolaria_server=info,tower_http=info".into()),
        )
        .init();

    let cfg = match config::ServerConfig::from_env() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("configuration error: {e}");
            std::process::exit(1);
        }
    };
    tracing::info!("serving vault {:?} on {}", cfg.vault_path, cfg.listen_addr);

    let app = build_router(&cfg);
    let listener = tokio::net::TcpListener::bind(cfg.listen_addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
