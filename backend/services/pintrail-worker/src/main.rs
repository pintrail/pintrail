//! Attachment processing worker.
//!
//! Claims queued attachments from Postgres with `FOR UPDATE SKIP LOCKED`,
//! turns originals into what the app displays, and writes the output back to
//! object storage. Runs as N independent replicas with no coordination beyond
//! the database (docs/DESIGN.md §2.5).

mod media;
mod queue;

use std::time::Duration;

use anyhow::Context;
use pintrail_storage::{Storage, StorageConfig};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use crate::queue::{AttachmentKind, Job};

struct Config {
    database_url: String,
    poll_interval: Duration,
    batch_size: i64,
    /// A job that has failed this many times stops being retried. Three covers
    /// transient storage faults without hammering a genuinely corrupt file.
    max_attempts: i32,
    /// How long a claim may sit in `processing` before another worker assumes
    /// its owner died.
    stuck_timeout_secs: i64,
    storage: StorageConfig,
}

impl Config {
    fn from_env() -> anyhow::Result<Self> {
        let var = |name: &str| -> anyhow::Result<String> {
            std::env::var(name)
                .ok()
                .filter(|v| !v.trim().is_empty())
                .with_context(|| format!("required environment variable {name} is not set"))
        };
        let opt = |name: &str, default: &str| -> String {
            std::env::var(name)
                .ok()
                .filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| default.to_string())
        };
        let num = |name: &str, default: i64| -> anyhow::Result<i64> {
            match std::env::var(name) {
                Err(_) => Ok(default),
                Ok(raw) if raw.trim().is_empty() => Ok(default),
                Ok(raw) => raw
                    .trim()
                    .parse()
                    .with_context(|| format!("{name} must be a number, got {raw:?}")),
            }
        };

        Ok(Self {
            database_url: var("DATABASE_URL")?,
            poll_interval: Duration::from_secs(num("WORKER_POLL_INTERVAL_SECS", 5)? as u64),
            batch_size: num("WORKER_BATCH_SIZE", 10)?,
            max_attempts: num("WORKER_MAX_ATTEMPTS", 3)? as i32,
            stuck_timeout_secs: num("WORKER_STUCK_TIMEOUT_SECS", 300)?,
            storage: StorageConfig {
                endpoint: std::env::var("S3_ENDPOINT")
                    .ok()
                    .filter(|v| !v.trim().is_empty()),
                region: opt("S3_REGION", "us-east-1"),
                bucket: var("S3_BUCKET")?,
                access_key_id: var("S3_ACCESS_KEY_ID")?,
                secret_access_key: var("S3_SECRET_ACCESS_KEY")?,
                // The worker never presigns; this only satisfies the shared type.
                presign_ttl: Duration::from_secs(900),
            },
        })
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pintrail_worker=debug,info".into()),
        )
        .init();

    let config = Config::from_env()?;

    let db = PgPoolOptions::new()
        .max_connections(4)
        .connect(&config.database_url)
        .await?;

    let storage = Storage::connect(&config.storage).await?;
    storage.check().await?;

    tracing::info!(
        poll_interval = ?config.poll_interval,
        batch_size = config.batch_size,
        "pintrail-worker started"
    );

    run(&db, &storage, &config).await;

    tracing::info!("pintrail-worker stopped");
    Ok(())
}

async fn run(db: &PgPool, storage: &Storage, config: &Config) {
    let mut ticker = tokio::time::interval(config.poll_interval);
    // If a batch overruns a tick, catch up with one run rather than firing
    // every missed tick back to back.
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                if let Err(err) = poll_once(db, storage, config).await {
                    // A failed cycle must not kill the worker; the next tick
                    // retries.
                    tracing::error!(error = ?err, "poll cycle failed");
                }
            }
            _ = shutdown_signal() => break,
        }
    }
}

async fn poll_once(db: &PgPool, storage: &Storage, config: &Config) -> anyhow::Result<()> {
    let requeued = queue::requeue_stuck(db, config.stuck_timeout_secs, config.max_attempts).await?;
    if requeued > 0 {
        tracing::warn!(requeued, "returned abandoned jobs to the queue");
    }

    let jobs = queue::claim(db, config.batch_size).await?;
    if jobs.is_empty() {
        return Ok(());
    }

    tracing::info!(count = jobs.len(), "claimed jobs");

    // Sequential on purpose. Processing is CPU-bound, so running a batch
    // concurrently on one worker would contend for the same cores; throughput
    // comes from running more replicas, which the queue already supports.
    for job in jobs {
        let id = job.id;
        let attempts = job.attempts;

        match process(storage, &job).await {
            Ok(success) => {
                queue::mark_processed(db, id, success).await?;
                tracing::info!(attachment_id = %id, "processed");
            }
            Err(err) => {
                // `{:#}` keeps the whole anyhow context chain, which is what
                // makes a subprocess failure diagnosable from the row.
                let message = format!("{err:#}");
                let gave_up =
                    queue::mark_failed(db, id, attempts, config.max_attempts, &message).await?;

                if gave_up {
                    tracing::error!(attachment_id = %id, attempts, error = %message, "giving up");
                } else {
                    tracing::warn!(attachment_id = %id, attempts, error = %message, "will retry");
                }
            }
        }
    }

    Ok(())
}

async fn process(storage: &Storage, job: &Job) -> anyhow::Result<queue::Success> {
    let original = storage.get(&job.original_storage_key).await?;

    // Decoding and re-encoding are CPU-bound and would stall the async
    // runtime; this hands them to the blocking pool.
    let kind = job.kind;
    let mime = job.original_mime_type.clone();
    let processed = tokio::task::spawn_blocking(move || -> anyhow::Result<Option<media::Processed>> {
        match kind {
            AttachmentKind::Image => Ok(Some(media::process_image(&original, &mime)?)),
            AttachmentKind::Pdf => Ok(Some(media::process_pdf(&original)?)),
            // A text block has nothing to render.
            AttachmentKind::Text => Ok(None),
            // Rejected at upload time; reaching here means the allowlist and
            // this dispatch have drifted apart.
            AttachmentKind::Audio | AttachmentKind::Video => {
                anyhow::bail!("{kind:?} processing is not implemented in v1")
            }
        }
    })
    .await
    .context("processing task panicked")??;

    let Some(processed) = processed else {
        // Nothing to render: the original is what gets served.
        return Ok(queue::Success {
            processed_storage_key: job.original_storage_key.clone(),
            processed_mime_type: job.original_mime_type.clone(),
            width: None,
            height: None,
        });
    };

    // Mirrors the originals/ layout so an object's purpose is obvious from its
    // key alone.
    let processed_key = format!(
        "processed/{}/{}.{}",
        job.artifact_id, job.id, processed.extension
    );

    storage
        .put(&processed_key, processed.bytes, processed.mime_type)
        .await?;

    Ok(queue::Success {
        processed_storage_key: processed_key,
        processed_mime_type: processed.mime_type.to_string(),
        width: processed.width,
        height: processed.height,
    })
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
