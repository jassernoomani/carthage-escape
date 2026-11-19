use std::{net::SocketAddr, time::Duration};

use anyhow::Context;
use carthage_server::{AppState, config::Config, content, db};
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // A missing .env file is fine (production sets real env vars).
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("carthage_server=info,tower_http=info")),
        )
        .init();

    let config = Config::from_env().context("loading configuration")?;

    let db = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&config.database_url)
        .await
        .context("connecting to the database")?;

    sqlx::migrate!()
        .run(&db)
        .await
        .context("running database migrations")?;

    let abandoned = db::abandon_stale_sessions(&db)
        .await
        .context("closing sessions left over from the previous run")?;
    if abandoned > 0 {
        tracing::warn!(
            abandoned,
            "marked sessions from a previous run as abandoned"
        );
    }
    db::purge_expired_host_sessions(&db)
        .await
        .context("purging expired host sessions")?;
    let rooms = content::sync_rooms(&db, &config.content_dir)
        .await
        .with_context(|| format!("loading rooms from {}", config.content_dir.display()))?;
    tracing::info!(rooms, "room templates loaded");

    let state = AppState::new(db, config.jwt_secret.as_bytes(), config.cookie_secure);
    let app = carthage_server::app(state, config.static_dir.as_deref());

    let listener = TcpListener::bind(config.bind_addr)
        .await
        .with_context(|| format!("binding to {}", config.bind_addr))?;
    tracing::info!("listening on http://{}", config.bind_addr);

    // Connect info gives handlers the client IP (used for rate limiting).
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .context("server error")?;

    tracing::info!("shut down cleanly");
    Ok(())
}

/// Resolves on Ctrl+C, or on SIGTERM (what `docker stop` sends) on Unix.
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(err) = tokio::signal::ctrl_c().await {
            tracing::error!(%err, "failed to listen for Ctrl+C");
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(err) => {
                tracing::error!(%err, "failed to listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutdown signal received");
}
