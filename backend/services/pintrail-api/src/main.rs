mod config;
mod error;
mod state;

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

    let app = router(state);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!(addr = %bind_addr, "pintrail-api listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/health/ready", get(ready))
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
