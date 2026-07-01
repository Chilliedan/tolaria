//! Tolaria web server: serves the SPA and a read-only vault RPC over tolaria-core.

use tolaria_server::config;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("useradd") {
        // tolaria-server useradd <username> <git_name> <git_email>   (password via TOLARIA_NEW_PASSWORD)
        let cfg = config::ServerConfig::from_env().unwrap_or_else(|e| {
            eprintln!("config error: {e}");
            std::process::exit(1);
        });
        let username = args.get(2).cloned().unwrap_or_default();
        let git_name = args.get(3).cloned().unwrap_or_else(|| username.clone());
        let git_email = args.get(4).cloned().unwrap_or_default();
        let password = std::env::var("TOLARIA_NEW_PASSWORD").unwrap_or_default();
        let users = tolaria_server::users::UsersDb::open(&cfg.users_db_path).unwrap_or_else(|e| {
            eprintln!("users db: {e}");
            std::process::exit(1);
        });
        match tolaria_server::users::run_useradd(&users, &username, &password, &git_name, &git_email) {
            Ok(()) => {
                println!("created user '{username}'");
            }
            Err(e) => {
                eprintln!("useradd failed: {e}");
                std::process::exit(1);
            }
        }
        return;
    }
    serve();
}

#[tokio::main]
async fn serve() {
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

    let users = tolaria_server::users::UsersDb::open(&cfg.users_db_path).unwrap_or_else(|e| {
        tracing::error!("users db: {e}");
        std::process::exit(1);
    });
    let sessions = tolaria_server::session::SessionStore::new(std::time::Duration::from_secs(
        60 * 60 * 24 * 7,
    ));
    let app = tolaria_server::build_router(
        cfg.vault_path,
        cfg.static_dir,
        users,
        sessions,
        cfg.cookie_secure,
    );
    let listener = tokio::net::TcpListener::bind(cfg.listen_addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
