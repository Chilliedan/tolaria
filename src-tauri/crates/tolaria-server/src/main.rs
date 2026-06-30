//! Tolaria web server: serves the SPA and a read-only vault RPC over tolaria-core.

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "tolaria_server=info,tower_http=info".into()),
        )
        .init();
    tracing::info!("tolaria-server starting (bootstrap stub)");
}
