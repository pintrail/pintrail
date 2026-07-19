//! Attachment processing worker.
//!
//! Stage 1 establishes the process, config, and shutdown-safe poll loop only.
//! The `SKIP LOCKED` claim query and the per-kind media pipelines land in
//! stage 7, once the `attachments` table exists (stage 2).

use std::time::Duration;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pintrail_worker=debug,info".into()),
        )
        .init();

    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("required environment variable DATABASE_URL is not set"))?;
    let poll_interval = Duration::from_secs(
        std::env::var("WORKER_POLL_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5),
    );

    let db = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await?;

    tracing::info!(?poll_interval, "pintrail-worker started");

    run(db, poll_interval).await;

    tracing::info!("pintrail-worker stopped");
    Ok(())
}

async fn run(db: PgPool, poll_interval: Duration) {
    let mut ticker = tokio::time::interval(poll_interval);
    // If processing overruns a tick, catch up by running once — not by firing
    // every missed tick back to back.
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                if let Err(err) = poll_once(&db).await {
                    // A failed poll must not kill the worker; the next tick retries.
                    tracing::error!(error = ?err, "poll cycle failed");
                }
            }
            _ = shutdown_signal() => break,
        }
    }
}

/// Claims and processes one batch. Stage 7 replaces the body with the
/// `FOR UPDATE SKIP LOCKED` claim documented in DESIGN.md section 2.5.
async fn poll_once(db: &PgPool) -> anyhow::Result<()> {
    sqlx::query("SELECT 1").execute(db).await?;
    tracing::trace!("poll cycle complete (no-op until stage 7)");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}
