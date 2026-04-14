use clap::{Parser, Subcommand};
use console_api::{AppState, api_router};
use console_core::ConsoleConfig;
use console_core::config::RuntimeConfig;
use console_db::{create_pool_from_path, run_migrations};
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::net::TcpListener;
use tokio::signal;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

#[derive(Parser)]
#[command(name = "wacp-console", about = "WACP Console — coordination workbench")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the console server
    Serve {
        /// HTTP listen address
        #[arg(long, default_value = "[::1]:8080")]
        listen: SocketAddr,

        /// Path to SQLite database file
        #[arg(long)]
        database: Option<PathBuf>,

        /// Serve frontend from disk instead of embedded assets (development)
        #[arg(long)]
        frontend_path: Option<PathBuf>,

        /// AgentService gRPC address
        #[arg(long, default_value = "[::1]:9090")]
        agent_address: String,

        /// HighwayService gRPC address
        #[arg(long, default_value = "[::1]:9091")]
        highway_address: String,

        /// CoordinatorService gRPC address
        #[arg(long, default_value = "[::1]:9092")]
        coordinator_address: String,

        /// REST gateway address
        #[arg(long, default_value = "http://[::1]:9093")]
        rest_address: String,
    },

    /// Run database migrations and exit
    Migrate {
        /// Path to SQLite database file
        #[arg(long)]
        database: Option<PathBuf>,
    },

    /// Reset the admin password (recovery tool)
    ResetAdminPassword {
        /// Path to SQLite database file
        #[arg(long)]
        database: Option<PathBuf>,

        /// Username of the admin account to reset
        #[arg(long, default_value = "admin")]
        username: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Serve {
            listen,
            database,
            frontend_path: _,
            agent_address,
            highway_address,
            coordinator_address,
            rest_address,
        } => {
            init_tracing();

            let config = ConsoleConfig {
                listen_addr: listen,
                database_path: resolve_database_path(database),
                data_dir: resolve_data_dir(),
                runtime: RuntimeConfig {
                    agent_address,
                    highway_address,
                    coordinator_address,
                    rest_address,
                },
            };

            info!(
                listen = %config.listen_addr,
                database = %config.database_path.display(),
                "starting wacp-console"
            );

            let pool = create_pool_from_path(&config.database_path).await?;
            run_migrations(&pool).await?;
            info!("database migrations applied");

            let state = AppState { db: pool };
            let app = api_router(state);

            let listener = TcpListener::bind(config.listen_addr).await?;
            info!(addr = %config.listen_addr, "server listening");

            let cancel = CancellationToken::new();
            let cancel_clone = cancel.clone();

            tokio::spawn(async move {
                shutdown_signal().await;
                info!("shutdown signal received, draining connections");
                cancel_clone.cancel();
            });

            axum::serve(listener, app)
                .with_graceful_shutdown(cancel.cancelled_owned())
                .await?;

            info!("server stopped");
        }

        Commands::Migrate { database } => {
            init_tracing();

            let db_path = resolve_database_path(database);
            info!(database = %db_path.display(), "running migrations");

            let pool = create_pool_from_path(&db_path).await?;
            run_migrations(&pool).await?;

            info!("migrations applied successfully");
        }

        Commands::ResetAdminPassword {
            database,
            username,
        } => {
            init_tracing();

            let db_path = resolve_database_path(database);
            info!(
                database = %db_path.display(),
                username = %username,
                "reset-admin-password: not yet implemented (Phase 1)"
            );

            let _pool = create_pool_from_path(&db_path).await?;
            // Phase 1 will implement: generate new password, hash with Argon2id,
            // update user, set must_change_password = 1.
            error!("reset-admin-password will be implemented in Phase 1");
            std::process::exit(1);
        }
    }

    Ok(())
}

fn init_tracing() {
    use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn,tonic=info,tower_http=debug"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer())
        .init();
}

/// Resolves the database path: explicit flag → XDG data dir → current directory.
fn resolve_database_path(explicit: Option<PathBuf>) -> PathBuf {
    if let Some(path) = explicit {
        return path;
    }

    let data_dir = resolve_data_dir();
    std::fs::create_dir_all(&data_dir).ok();
    data_dir.join("console.db")
}

/// Resolves the XDG data directory for wacp-console.
fn resolve_data_dir() -> PathBuf {
    directories::ProjectDirs::from("", "", "wacp-console")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}
