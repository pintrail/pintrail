//! The System tab: which build is running, and what the search indexer is
//! doing (#35).
//!
//! Two front ends over the same queries: the HTML page at `/admin/system`
//! (forms, CSRF-protected like the rest of the panel) and a JSON API under
//! `/api/admin/search/...` for scripts. Both are admin-only.
//!
//! The indexer itself runs in the worker (pintrail-worker/src/search_index.rs);
//! this module only reads its tables and queues work for it. Retrying a job
//! puts it back in the queue with a fresh attempt count; re-indexing queues a
//! new job. The worker skips the write when an artifact's text is unchanged,
//! so a rebuild after nothing changed is cheap.

use axum::extract::{Path, Query, State};
use axum::response::{Html, Redirect};
use axum::routing::{get, post};
use axum::{Form, Json, Router};
use axum_extra::extract::cookie::CookieJar;
use chrono::{DateTime, Utc};
use minijinja::context;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::csrf;
use super::routes::{author_context, render, require_csrf, session_token, CsrfForm};
use crate::authors::extractors::RequireAdmin;
use crate::build_info;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// How many failed jobs and recent jobs the page lists.
const FAILED_LIMIT: i64 = 50;
const RECENT_LIMIT: i64 = 20;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/system", get(system_page))
        .route("/admin/system/search/jobs/{id}/retry", post(retry_job_form))
        .route("/admin/system/search/retry-failed", post(retry_failed_form))
        .route(
            "/admin/system/search/artifacts/{id}/reindex",
            post(reindex_form),
        )
        .route("/admin/system/search/rebuild", post(rebuild_form))
        .route(
            "/admin/system/routes/retry-failed",
            post(retry_failed_routes_form),
        )
        .route(
            "/admin/system/routes/recompute",
            post(recompute_routes_form),
        )
}

/// JSON twins of the page's actions. Merged outside the panel's redirect
/// layer, so an expired session gets a 401, not a sign-in page.
pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/system", get(system_json))
        .route("/api/admin/search/status", get(status_json))
        .route("/api/admin/search/jobs/{id}/retry", post(retry_job_json))
        .route("/api/admin/search/retry-failed", post(retry_failed_json))
        .route(
            "/api/admin/search/artifacts/{id}/reindex",
            post(reindex_json),
        )
        .route("/api/admin/search/rebuild", post(rebuild_json))
}

// --- queries -----------------------------------------------------------------

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SearchCounts {
    /// Artifacts that should have a search document (not deleted).
    pub eligible: i64,
    /// Of those, how many have their artifact document.
    pub indexed: i64,
    /// Eligible artifacts with no document and no queued or running job.
    /// Nothing will index these until someone re-indexes them.
    pub missing: i64,
    pub queued: i64,
    /// Queued jobs that already failed at least once and are waiting to retry.
    pub retrying: i64,
    pub processing: i64,
    pub failed: i64,
    /// Completed jobs still on record (the worker prunes them after 7 days).
    pub processed_recent: i64,
    pub last_processed_at: Option<DateTime<Utc>>,
    pub oldest_queued_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct JobRow {
    pub id: Uuid,
    pub artifact_id: Uuid,
    pub artifact_name: Option<String>,
    pub status: String,
    pub attempts: i32,
    pub error_message: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct MigrationRow {
    pub version: i64,
    pub description: String,
    pub installed_on: DateTime<Utc>,
}

pub async fn search_counts(db: &PgPool) -> AppResult<SearchCounts> {
    Ok(sqlx::query_as::<_, SearchCounts>(
        r#"
        SELECT
            (SELECT count(*) FROM artifacts WHERE deleted_at IS NULL) AS eligible,
            (SELECT count(DISTINCT d.artifact_id) FROM search_documents d
               JOIN artifacts a ON a.id = d.artifact_id
              WHERE d.source_kind = 'artifact' AND a.deleted_at IS NULL) AS indexed,
            (SELECT count(*) FROM artifacts a
              WHERE a.deleted_at IS NULL
                AND NOT EXISTS (SELECT 1 FROM search_documents d
                                 WHERE d.artifact_id = a.id AND d.source_kind = 'artifact')
                AND NOT EXISTS (SELECT 1 FROM search_index_jobs j
                                 WHERE j.artifact_id = a.id AND j.status IN ('queued', 'processing'))
            ) AS missing,
            count(*) FILTER (WHERE status = 'queued') AS queued,
            count(*) FILTER (WHERE status = 'queued' AND attempts > 0) AS retrying,
            count(*) FILTER (WHERE status = 'processing') AS processing,
            count(*) FILTER (WHERE status = 'failed') AS failed,
            count(*) FILTER (WHERE status = 'processed') AS processed_recent,
            max(updated_at) FILTER (WHERE status = 'processed') AS last_processed_at,
            min(created_at) FILTER (WHERE status = 'queued') AS oldest_queued_at
        FROM search_index_jobs
        "#,
    )
    .fetch_one(db)
    .await?)
}

async fn jobs(db: &PgPool, statuses: &[&str], limit: i64) -> AppResult<Vec<JobRow>> {
    Ok(sqlx::query_as::<_, JobRow>(
        r#"
        SELECT j.id, j.artifact_id, a.name AS artifact_name, j.status, j.attempts,
               j.error_message, j.updated_at
        FROM search_index_jobs j
        LEFT JOIN artifacts a ON a.id = j.artifact_id
        WHERE j.status = ANY($1)
        ORDER BY j.updated_at DESC, j.id
        LIMIT $2
        "#,
    )
    .bind(statuses.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    .bind(limit)
    .fetch_all(db)
    .await?)
}

async fn latest_migration(db: &PgPool) -> AppResult<(Option<MigrationRow>, i64)> {
    let latest = sqlx::query_as::<_, MigrationRow>(
        "SELECT version, description, installed_on FROM _sqlx_migrations \
         WHERE success ORDER BY version DESC LIMIT 1",
    )
    .fetch_optional(db)
    .await?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE success")
        .fetch_one(db)
        .await?;
    Ok((latest, count))
}

/// Puts a failed job back in the queue with a fresh attempt count.
async fn retry_job(db: &PgPool, id: Uuid) -> AppResult<()> {
    let n = sqlx::query(
        "UPDATE search_index_jobs \
         SET status = 'queued', attempts = 0, error_message = NULL, available_at = now() \
         WHERE id = $1 AND status = 'failed'",
    )
    .bind(id)
    .execute(db)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(AppError::NotFound("failed search job"));
    }
    Ok(())
}

async fn retry_all_failed(db: &PgPool) -> AppResult<u64> {
    Ok(sqlx::query(
        "UPDATE search_index_jobs \
         SET status = 'queued', attempts = 0, error_message = NULL, available_at = now() \
         WHERE status = 'failed'",
    )
    .execute(db)
    .await?
    .rows_affected())
}

/// Queues one artifact, unless a job for it is already waiting.
async fn reindex(db: &PgPool, artifact_id: Uuid) -> AppResult<bool> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM artifacts WHERE id = $1 AND deleted_at IS NULL)",
    )
    .bind(artifact_id)
    .fetch_one(db)
    .await?;
    if !exists {
        return Err(AppError::NotFound("artifact"));
    }
    let queued = sqlx::query(
        "INSERT INTO search_index_jobs (artifact_id) SELECT $1 \
         WHERE NOT EXISTS (SELECT 1 FROM search_index_jobs WHERE artifact_id = $1 AND status = 'queued')",
    )
    .bind(artifact_id)
    .execute(db)
    .await?
    .rows_affected();
    Ok(queued > 0)
}

/// Queues every live artifact that has no job already waiting.
async fn rebuild(db: &PgPool) -> AppResult<u64> {
    Ok(sqlx::query(
        "INSERT INTO search_index_jobs (artifact_id) \
         SELECT a.id FROM artifacts a WHERE a.deleted_at IS NULL \
           AND NOT EXISTS (SELECT 1 FROM search_index_jobs j \
                            WHERE j.artifact_id = a.id AND j.status = 'queued')",
    )
    .execute(db)
    .await?
    .rows_affected())
}

// --- walking routes (#91) ------------------------------------------------------

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct RouteCounts {
    pub trails: i64,
    pub routed: i64,
    pub no_path: i64,
    pub too_few_stops: i64,
    /// Trails with no stored route at all.
    pub missing: i64,
    pub queued: i64,
    pub processing: i64,
    /// Jobs that ran out of attempts (the router errored or was unreachable).
    pub failed_jobs: i64,
    pub last_computed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct RouteProblem {
    pub trail_id: Uuid,
    pub title: String,
    /// "no path" (the router answered) or "job failed" (it errored).
    pub kind: String,
    pub error: Option<String>,
    pub at: DateTime<Utc>,
}

pub async fn route_counts(db: &PgPool) -> AppResult<RouteCounts> {
    Ok(sqlx::query_as::<_, RouteCounts>(
        r#"
        SELECT
            (SELECT count(*) FROM trails) AS trails,
            (SELECT count(*) FROM trail_routes WHERE status = 'ok') AS routed,
            (SELECT count(*) FROM trail_routes WHERE status = 'failed') AS no_path,
            (SELECT count(*) FROM trail_routes WHERE status = 'too_few_stops') AS too_few_stops,
            (SELECT count(*) FROM trails t WHERE NOT EXISTS
                (SELECT 1 FROM trail_routes r WHERE r.trail_id = t.id)) AS missing,
            (SELECT count(*) FROM trail_route_jobs WHERE status = 'queued') AS queued,
            (SELECT count(*) FROM trail_route_jobs WHERE status = 'processing') AS processing,
            (SELECT count(*) FROM trail_route_jobs WHERE status = 'failed') AS failed_jobs,
            (SELECT max(computed_at) FROM trail_routes) AS last_computed_at
        "#,
    )
    .fetch_one(db)
    .await?)
}

async fn route_problems(db: &PgPool) -> AppResult<Vec<RouteProblem>> {
    Ok(sqlx::query_as::<_, RouteProblem>(
        r#"
        SELECT r.trail_id, t.title, 'no path' AS kind, r.error_message AS error, r.computed_at AS at
        FROM trail_routes r JOIN trails t ON t.id = r.trail_id WHERE r.status = 'failed'
        UNION ALL
        SELECT j.trail_id, t.title, 'job failed', j.error_message, j.updated_at
        FROM trail_route_jobs j JOIN trails t ON t.id = j.trail_id WHERE j.status = 'failed'
        ORDER BY at DESC LIMIT 50
        "#,
    )
    .fetch_all(db)
    .await?)
}

async fn retry_failed_routes(db: &PgPool) -> AppResult<u64> {
    Ok(sqlx::query(
        "UPDATE trail_route_jobs \
         SET status = 'queued', attempts = 0, error_message = NULL, available_at = now() \
         WHERE status = 'failed'",
    )
    .execute(db)
    .await?
    .rows_affected())
}

/// Queues every trail. Unchanged stops are skipped by the worker, so this
/// mostly retries the ones with no route or a "no path" answer.
async fn recompute_routes(db: &PgPool) -> AppResult<u64> {
    // A failed route is only recomputed if its fingerprint is cleared, since
    // unchanged stops are skipped; clearing it is the point of asking.
    sqlx::query("DELETE FROM trail_routes WHERE status = 'failed'")
        .execute(db)
        .await?;
    Ok(sqlx::query(
        "INSERT INTO trail_route_jobs (trail_id) SELECT t.id FROM trails t \
         WHERE NOT EXISTS (SELECT 1 FROM trail_route_jobs j WHERE j.trail_id = t.id AND j.status = 'queued')",
    )
    .execute(db)
    .await?
    .rows_affected())
}

/// Whether the routing service answers, as the API sees it. None when this
/// API has no VALHALLA_URL (routing is off here).
async fn router_status() -> Option<Result<(), String>> {
    let url = std::env::var("VALHALLA_URL")
        .ok()
        .filter(|u| !u.trim().is_empty())?;
    let status_url = format!("{}/status", url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .ok()?;
    Some(match client.get(&status_url).send().await {
        Ok(r) if r.status().is_success() => Ok(()),
        Ok(r) => Err(format!("answered HTTP {}", r.status().as_u16())),
        Err(e) if e.is_timeout() => Err("did not answer within 3 seconds".into()),
        Err(_) => Err(format!(
            "not reachable at {url} (still starting, or not running)"
        )),
    })
}

// --- formatting ----------------------------------------------------------------

/// "12 s ago", "4 min ago", "3 h ago", "2 days ago".
fn ago(t: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let secs = (now - t).num_seconds().max(0);
    match secs {
        0..=59 => format!("{secs} s ago"),
        60..=3599 => format!("{} min ago", secs / 60),
        3600..=86_399 => format!("{} h ago", secs / 3600),
        _ => {
            let days = secs / 86_400;
            format!("{days} day{} ago", if days == 1 { "" } else { "s" })
        }
    }
}

fn stamp(t: DateTime<Utc>) -> String {
    t.format("%Y-%m-%d %H:%M UTC").to_string()
}

fn job_context(j: &JobRow, now: DateTime<Utc>) -> minijinja::Value {
    context! {
        id => j.id.to_string(),
        artifact_id => j.artifact_id.to_string(),
        artifact_name => j.artifact_name.clone().unwrap_or_else(|| "(untitled)".into()),
        status => j.status.clone(),
        attempts => j.attempts,
        error => j.error_message.clone(),
        when => ago(j.updated_at, now),
        when_full => stamp(j.updated_at),
    }
}

// --- HTML ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct PageQuery {
    done: Option<String>,
    n: Option<u64>,
}

async fn system_page(
    admin: RequireAdmin,
    State(state): State<AppState>,
    jar: CookieJar,
    Query(q): Query<PageQuery>,
) -> AppResult<Html<String>> {
    let now = Utc::now();
    let build = build_info::current();
    let (migration, migration_count) = latest_migration(&state.db).await?;
    let counts = search_counts(&state.db).await?;
    let failed = jobs(&state.db, &["failed"], FAILED_LIMIT).await?;
    let recent = jobs(&state.db, &["processed", "processing"], RECENT_LIMIT).await?;

    let percent = if counts.eligible > 0 {
        (counts.indexed * 100 / counts.eligible).min(100)
    } else {
        100
    };
    let routes = route_counts(&state.db).await?;
    let route_problems = route_problems(&state.db).await?;
    let router = router_status().await;
    let busy = counts.queued + counts.processing > 0 || routes.queued + routes.processing > 0;

    let flash = match (q.done.as_deref(), q.n) {
        (Some("retried"), _) => Some("Job queued again.".to_string()),
        (Some("retried-all"), Some(n)) => Some(format!(
            "{n} failed job{} queued again.",
            if n == 1 { "" } else { "s" }
        )),
        (Some("reindexed"), _) => Some("Artifact queued for indexing.".to_string()),
        (Some("already-queued"), _) => {
            Some("That artifact is already waiting in the queue.".to_string())
        }
        (Some("routes-retried"), Some(n)) => Some(format!(
            "{n} failed route job{} queued again.",
            if n == 1 { "" } else { "s" }
        )),
        (Some("routes-recomputed"), Some(n)) => Some(format!(
            "{n} trail{} queued for a new route.",
            if n == 1 { "" } else { "s" }
        )),
        (Some("rebuilt"), Some(n)) => Some(format!(
            "{n} artifact{} queued for indexing.",
            if n == 1 { "" } else { "s" }
        )),
        _ => None,
    };

    render(
        "system.html",
        context! {
            title => "System",
            section => "system",
            author => author_context(&admin.0),
            csrf_token => csrf::token_for_session(&session_token(&jar)),
            flash => flash,
            // Reload while there is work in flight, so progress is visible.
            refresh_seconds => if busy { Some(10) } else { None::<u32> },
            build => context! {
                commit => build.commit.clone(),
                commit_short => build.commit_short.clone(),
                commit_url => build_info::is_git_sha(&build.commit)
                    .then(|| format!("https://github.com/pintrail/pintrail/commit/{}", build.commit)),
                built_at => build.built_at.clone(),
                started_at => stamp(build.started_at),
                started_ago => ago(build.started_at, now),
            },
            migration => migration.map(|m| context! {
                version => m.version,
                description => m.description,
                installed => stamp(m.installed_on),
            }),
            migration_count => migration_count,
            counts => context! {
                eligible => counts.eligible,
                indexed => counts.indexed,
                missing => counts.missing,
                queued => counts.queued,
                retrying => counts.retrying,
                processing => counts.processing,
                failed => counts.failed,
                processed_recent => counts.processed_recent,
                percent => percent,
                last_processed => counts.last_processed_at.map(|t| ago(t, now)),
                last_processed_full => counts.last_processed_at.map(stamp),
                oldest_queued => counts.oldest_queued_at.map(|t| ago(t, now)),
            },
            busy => busy,
            failed => failed.iter().map(|j| job_context(j, now)).collect::<Vec<_>>(),
            recent => recent.iter().map(|j| job_context(j, now)).collect::<Vec<_>>(),
            router => match &router {
                None => context! { state => "off" },
                Some(Ok(())) => context! { state => "ready" },
                Some(Err(e)) => context! { state => "down", error => e.clone() },
            },
            routes => context! {
                trails => routes.trails,
                routed => routes.routed,
                no_path => routes.no_path,
                too_few_stops => routes.too_few_stops,
                missing => routes.missing,
                queued => routes.queued,
                processing => routes.processing,
                failed_jobs => routes.failed_jobs,
                last_computed => routes.last_computed_at.map(|t| ago(t, now)),
            },
            route_problems => route_problems.iter().map(|p| context! {
                trail_id => p.trail_id.to_string(),
                title => p.title.clone(),
                kind => p.kind.clone(),
                error => p.error.clone(),
                when => ago(p.at, now),
            }).collect::<Vec<_>>(),
        },
    )
}

async fn retry_job_form(
    admin: RequireAdmin,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<Uuid>,
    Form(form): Form<CsrfForm>,
) -> AppResult<Redirect> {
    require_csrf(&jar, &form.csrf_token)?;
    retry_job(&state.db, id).await?;
    tracing::info!(actor = %admin.0.id, job_id = %id, "search job retried");
    Ok(Redirect::to("/admin/system?done=retried"))
}

async fn retry_failed_form(
    admin: RequireAdmin,
    State(state): State<AppState>,
    jar: CookieJar,
    Form(form): Form<CsrfForm>,
) -> AppResult<Redirect> {
    require_csrf(&jar, &form.csrf_token)?;
    let n = retry_all_failed(&state.db).await?;
    tracing::info!(actor = %admin.0.id, jobs = n, "failed search jobs retried");
    Ok(Redirect::to(&format!(
        "/admin/system?done=retried-all&n={n}"
    )))
}

async fn reindex_form(
    admin: RequireAdmin,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<Uuid>,
    Form(form): Form<CsrfForm>,
) -> AppResult<Redirect> {
    require_csrf(&jar, &form.csrf_token)?;
    let queued = reindex(&state.db, id).await?;
    tracing::info!(actor = %admin.0.id, artifact_id = %id, queued, "artifact re-index requested");
    Ok(Redirect::to(if queued {
        "/admin/system?done=reindexed"
    } else {
        "/admin/system?done=already-queued"
    }))
}

async fn rebuild_form(
    admin: RequireAdmin,
    State(state): State<AppState>,
    jar: CookieJar,
    Form(form): Form<CsrfForm>,
) -> AppResult<Redirect> {
    require_csrf(&jar, &form.csrf_token)?;
    let n = rebuild(&state.db).await?;
    tracing::info!(actor = %admin.0.id, jobs = n, "search index rebuild queued");
    Ok(Redirect::to(&format!("/admin/system?done=rebuilt&n={n}")))
}

async fn retry_failed_routes_form(
    admin: RequireAdmin,
    State(state): State<AppState>,
    jar: CookieJar,
    Form(form): Form<CsrfForm>,
) -> AppResult<Redirect> {
    require_csrf(&jar, &form.csrf_token)?;
    let n = retry_failed_routes(&state.db).await?;
    tracing::info!(actor = %admin.0.id, jobs = n, "failed route jobs retried");
    Ok(Redirect::to(&format!(
        "/admin/system?done=routes-retried&n={n}"
    )))
}

async fn recompute_routes_form(
    admin: RequireAdmin,
    State(state): State<AppState>,
    jar: CookieJar,
    Form(form): Form<CsrfForm>,
) -> AppResult<Redirect> {
    require_csrf(&jar, &form.csrf_token)?;
    let n = recompute_routes(&state.db).await?;
    tracing::info!(actor = %admin.0.id, jobs = n, "trail routes recompute queued");
    Ok(Redirect::to(&format!(
        "/admin/system?done=routes-recomputed&n={n}"
    )))
}

// --- JSON ---------------------------------------------------------------------

async fn system_json(
    _admin: RequireAdmin,
    State(state): State<AppState>,
) -> AppResult<Json<Value>> {
    let (migration, migration_count) = latest_migration(&state.db).await?;
    Ok(Json(json!({
        "build": build_info::current(),
        "migrations": { "latest": migration, "applied": migration_count },
        "search": search_counts(&state.db).await?,
        "routes": route_counts(&state.db).await?,
        "router": match router_status().await {
            None => json!({ "state": "off" }),
            Some(Ok(())) => json!({ "state": "ready" }),
            Some(Err(e)) => json!({ "state": "down", "error": e }),
        },
    })))
}

async fn status_json(
    _admin: RequireAdmin,
    State(state): State<AppState>,
) -> AppResult<Json<Value>> {
    Ok(Json(json!({
        "counts": search_counts(&state.db).await?,
        "failed": jobs(&state.db, &["failed"], FAILED_LIMIT).await?,
        "recent": jobs(&state.db, &["processed", "processing"], RECENT_LIMIT).await?,
    })))
}

async fn retry_job_json(
    admin: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    retry_job(&state.db, id).await?;
    tracing::info!(actor = %admin.0.id, job_id = %id, "search job retried");
    Ok(Json(json!({ "retried": 1 })))
}

async fn retry_failed_json(
    admin: RequireAdmin,
    State(state): State<AppState>,
) -> AppResult<Json<Value>> {
    let n = retry_all_failed(&state.db).await?;
    tracing::info!(actor = %admin.0.id, jobs = n, "failed search jobs retried");
    Ok(Json(json!({ "retried": n })))
}

async fn reindex_json(
    admin: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let queued = reindex(&state.db, id).await?;
    tracing::info!(actor = %admin.0.id, artifact_id = %id, queued, "artifact re-index requested");
    Ok(Json(json!({ "queued": queued })))
}

async fn rebuild_json(
    admin: RequireAdmin,
    State(state): State<AppState>,
) -> AppResult<Json<Value>> {
    let n = rebuild(&state.db).await?;
    tracing::info!(actor = %admin.0.id, jobs = n, "search index rebuild queued");
    Ok(Json(json!({ "queued": n })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn ago_reads_naturally() {
        let now = Utc::now();
        assert_eq!(ago(now - Duration::seconds(12), now), "12 s ago");
        assert_eq!(ago(now - Duration::minutes(4), now), "4 min ago");
        assert_eq!(ago(now - Duration::hours(3), now), "3 h ago");
        assert_eq!(ago(now - Duration::days(1), now), "1 day ago");
        assert_eq!(ago(now - Duration::days(9), now), "9 days ago");
        // Clock skew between database and app never shows "-3 s ago".
        assert_eq!(ago(now + Duration::seconds(3), now), "0 s ago");
    }

    async fn artifact(db: &PgPool, name: &str) -> anyhow::Result<Uuid> {
        Ok(sqlx::query_scalar(
            "INSERT INTO artifacts (kind, name, description) VALUES ('other', $1, 'x') RETURNING id",
        )
        .bind(name)
        .fetch_one(db)
        .await?)
    }

    /// Stands in for the worker: finishes every queued job for an artifact
    /// by writing its document.
    async fn index(db: &PgPool, id: Uuid) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO search_documents (artifact_id, content, content_hash) \
             VALUES ($1, 'text', repeat('a', 64))",
        )
        .bind(id)
        .execute(db)
        .await?;
        sqlx::query("UPDATE search_index_jobs SET status = 'processed' WHERE artifact_id = $1")
            .bind(id)
            .execute(db)
            .await?;
        Ok(())
    }

    async fn fail(db: &PgPool, id: Uuid) -> anyhow::Result<Uuid> {
        Ok(sqlx::query_scalar(
            "UPDATE search_index_jobs SET status = 'failed', attempts = 3, error_message = 'boom' \
             WHERE artifact_id = $1 RETURNING id",
        )
        .bind(id)
        .fetch_one(db)
        .await?)
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn counts_track_progress_failures_and_gaps(db: PgPool) -> anyhow::Result<()> {
        let done = artifact(&db, "Done").await?;
        let broken = artifact(&db, "Broken").await?;
        let waiting = artifact(&db, "Waiting").await?;
        index(&db, done).await?;
        fail(&db, broken).await?;

        let c = search_counts(&db).await?;
        assert_eq!((c.eligible, c.indexed), (3, 1));
        assert_eq!(
            (c.queued, c.processing, c.failed, c.processed_recent),
            (1, 0, 1, 1)
        );
        assert!(c.last_processed_at.is_some() && c.oldest_queued_at.is_some());
        // Only the failed one has no document and nothing pending; the
        // waiting one is still queued.
        assert_eq!(c.missing, 1);

        let failed = jobs(&db, &["failed"], 10).await?;
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].artifact_name.as_deref(), Some("Broken"));
        assert_eq!(failed[0].error_message.as_deref(), Some("boom"));
        let _ = waiting;
        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn retry_requeues_failed_jobs_with_fresh_attempts(db: PgPool) -> anyhow::Result<()> {
        let a = artifact(&db, "A").await?;
        let b = artifact(&db, "B").await?;
        let job_a = fail(&db, a).await?;
        fail(&db, b).await?;

        retry_job(&db, job_a).await?;
        let (status, attempts, error): (String, i32, Option<String>) = sqlx::query_as(
            "SELECT status, attempts, error_message FROM search_index_jobs WHERE id = $1",
        )
        .bind(job_a)
        .fetch_one(&db)
        .await?;
        assert_eq!((status.as_str(), attempts, error), ("queued", 0, None));
        // Retrying a job that is no longer failed is a 404, not a silent no-op.
        assert!(matches!(
            retry_job(&db, job_a).await,
            Err(AppError::NotFound(_))
        ));

        assert_eq!(retry_all_failed(&db).await?, 1);
        assert_eq!(search_counts(&db).await?.failed, 0);
        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn reindex_and_rebuild_do_not_pile_up_duplicate_jobs(db: PgPool) -> anyhow::Result<()> {
        let a = artifact(&db, "A").await?;
        let b = artifact(&db, "B").await?;
        index(&db, a).await?;
        index(&db, b).await?;

        assert!(reindex(&db, a).await?);
        // A second request while the first is still waiting adds nothing.
        assert!(!reindex(&db, a).await?);
        // Rebuild queues the rest, skipping what is already waiting.
        assert_eq!(rebuild(&db).await?, 1);
        assert_eq!(rebuild(&db).await?, 0);
        assert_eq!(search_counts(&db).await?.queued, 2);

        sqlx::query("UPDATE artifacts SET deleted_at = now() WHERE id = $1")
            .bind(b)
            .execute(&db)
            .await?;
        assert!(matches!(reindex(&db, b).await, Err(AppError::NotFound(_))));
        assert!(matches!(
            reindex(&db, Uuid::new_v4()).await,
            Err(AppError::NotFound(_))
        ));
        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn route_counts_and_recovery_actions(db: PgPool) -> anyhow::Result<()> {
        let author: Uuid = sqlx::query_scalar(
            "INSERT INTO authors (email, password_hash, role) VALUES ('r@example.test', 'x', 'editor') RETURNING id",
        )
        .fetch_one(&db)
        .await?;
        let mut trails = Vec::new();
        for title in ["A", "B"] {
            let t: Uuid = sqlx::query_scalar(
                "INSERT INTO trails (title, owner_type, owner_id) VALUES ($1, 'author', $2) RETURNING id",
            )
            .bind(title)
            .bind(author)
            .fetch_one(&db)
            .await?;
            trails.push(t);
        }
        // A: its job ran out of attempts while the router was down.
        // B: the router answered "no path".
        sqlx::query("INSERT INTO trail_route_jobs (trail_id, status, attempts, error_message) VALUES ($1, 'failed', 3, 'connection refused')")
            .bind(trails[0])
            .execute(&db)
            .await?;
        sqlx::query("INSERT INTO trail_routes (trail_id, status, stops_hash, error_message) VALUES ($1, 'failed', repeat('a', 64), 'No path')")
            .bind(trails[1])
            .execute(&db)
            .await?;

        let c = route_counts(&db).await?;
        assert_eq!(
            (c.trails, c.routed, c.no_path, c.failed_jobs, c.missing),
            (2, 0, 1, 1, 1)
        );
        let problems = route_problems(&db).await?;
        assert_eq!(problems.len(), 2);

        assert_eq!(retry_failed_routes(&db).await?, 1);
        assert_eq!(route_counts(&db).await?.failed_jobs, 0);

        // Recompute clears the "no path" answer so it is tried again, and
        // queues B (A already has a job waiting).
        assert_eq!(recompute_routes(&db).await?, 1);
        let c = route_counts(&db).await?;
        assert_eq!((c.no_path, c.queued), (0, 2));
        Ok(())
    }
}
