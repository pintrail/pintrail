mod admin;
mod artifacts;
mod attachments;
mod authors;
mod cli;
mod client_ip;
mod comments;
mod config;
mod crypto;
mod error;
mod identity;
mod mail;
mod rate_limit;
mod readers;
mod serde_util;
mod state;
mod trails;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{json, Value};
use tower_http::trace::TraceLayer;

use crate::config::Settings;
use crate::error::AppResult;
use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // .env is a developer convenience; in Docker the vars come from compose.
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pintrail_api=debug,tower_http=debug,info".into()),
        )
        .init();

    let settings = Settings::from_env()?;
    let bind_addr = settings.bind_addr.clone();

    let state = AppState::connect(settings).await?;
    sqlx::migrate!("../../migrations").run(&state.db).await?;

    // Operator commands run against the same migrated database, then exit.
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        [] => {}
        ["create-author", email, role] => {
            return cli::create_author(&state.db, email, role).await;
        }
        ["reset-password", email] => {
            return cli::reset_password(&state.db, email).await;
        }
        ["help" | "--help" | "-h"] => {
            print!("{}", cli::USAGE);
            return Ok(());
        }
        other => {
            eprint!("unrecognized command: {}\n\n{}", other.join(" "), cli::USAGE);
            std::process::exit(2);
        }
    }

    // Stale rate-limit buckets would otherwise accumulate one entry per
    // client address ever seen.
    rate_limit::spawn_sweeper(state.limiter.clone(), std::time::Duration::from_secs(3600));

    let app = router(state);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!(addr = %bind_addr, "pintrail-api listening");

    // ConnectInfo carries the peer address, which rate limiting keys on when
    // no trusted proxy header is present.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    Ok(())
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/health/ready", get(ready))
        .merge(authors::router())
        .merge(readers::router())
        .merge(artifacts::router())
        .merge(attachments::router())
        .merge(trails::router())
        .merge(comments::router())
        // The panel is merged last and wrapped so an expired session lands on
        // the sign-in page rather than a bare JSON 401.
        .merge(admin::router().layer(axum::middleware::from_fn(
            admin::redirect_unauthenticated,
        )))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Liveness: the process is up. Deliberately does not touch the database, so
/// an orchestrator does not restart a healthy API during a database blip.
async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "pintrail-api" }))
}

/// Readiness: the process can actually serve requests, database included.
async fn ready(State(state): State<AppState>) -> AppResult<Json<Value>> {
    sqlx::query("SELECT 1").execute(&state.db).await?;
    Ok(Json(json!({ "status": "ready" })))
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

    tracing::info!("shutdown signal received");
}
