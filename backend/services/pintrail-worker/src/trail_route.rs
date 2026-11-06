//! Computes walking routes for trails (#91) with Valhalla (#90).
//!
//! A job is queued whenever a trail's stops change, or a stop's location
//! does (database triggers, migrations/20261020000002_trail_routes.sql). The
//! job reads the trail's stops in order with their effective coordinates,
//! asks Valhalla for a pedestrian route through them, and stores the line,
//! distance, time and per-leg maneuvers in trail_routes, where maps read it.
//!
//! - The router is called outside any transaction. The write that follows
//!   checks the job's claim and serializes per trail, like search indexing.
//! - A route whose stops have not changed (same fingerprint) is not
//!   recomputed.
//! - "No path between these stops" is an answer, stored as a failed route;
//!   a router that is down or erroring is retried with backoff.
//! - Valhalla takes at most 50 locations per pedestrian request; longer
//!   trails are routed in overlapping windows and joined.
//! - Consecutive stops at the same spot (two features of one building) get a
//!   zero-length leg rather than a request Valhalla would reject.

use std::future::Future;
use std::time::Duration;

use anyhow::Context;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

/// Valhalla's default service_limits.pedestrian.max_locations.
const MAX_LOCATIONS: usize = 50;

#[derive(Debug, sqlx::FromRow)]
struct Job {
    id: Uuid,
    trail_id: Uuid,
    attempts: i32,
    claim_token: Uuid,
}

/// A stop that can be routed to: live and with a location.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct Stop {
    pub id: Uuid,
    pub lat: f64,
    pub lng: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Maneuver {
    pub instruction: String,
    pub distance_m: f64,
}

/// The route between two consecutive points sent to the router.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    /// [lng, lat] pairs.
    pub coordinates: Vec<[f64; 2]>,
    pub distance_m: f64,
    pub duration_s: f64,
    pub maneuvers: Vec<Maneuver>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Leg {
    pub from_stop: Uuid,
    pub to_stop: Uuid,
    pub distance_m: f64,
    pub duration_s: f64,
    pub maneuvers: Vec<Maneuver>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    pub coordinates: Vec<[f64; 2]>,
    pub distance_m: f64,
    pub duration_s: f64,
    pub legs: Vec<Leg>,
}

/// Why the router gave no route.
#[derive(Debug, thiserror::Error)]
pub enum RouteError {
    /// The router answered: these points cannot be connected on foot (or one
    /// is too far from any path). Stored on the route; not retried.
    #[error("{0}")]
    NoRoute(String),
    /// The router is unreachable or failed. The job is retried.
    #[error(transparent)]
    Unavailable(#[from] anyhow::Error),
}

/// Something that routes between points: Valhalla in production, a stub in
/// tests. Returns one segment per consecutive pair.
pub trait Router: Send + Sync {
    fn route(
        &self,
        points: &[[f64; 2]],
    ) -> impl Future<Output = Result<Vec<Segment>, RouteError>> + Send;

    /// Whether the router can take requests now. Route jobs wait, without
    /// using up attempts, while it can't: Valhalla answers nothing useful
    /// until its first data build finishes, which takes minutes.
    fn ready(&self) -> impl Future<Output = bool> + Send;
}

pub struct Valhalla {
    client: reqwest::Client,
    url: String,
    status_url: String,
}

impl Valhalla {
    pub fn new(base_url: &str) -> anyhow::Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(60))
                .build()?,
            url: format!("{}/route", base_url.trim_end_matches('/')),
            status_url: format!("{}/status", base_url.trim_end_matches('/')),
        })
    }
}

impl Router for Valhalla {
    async fn route(&self, points: &[[f64; 2]]) -> Result<Vec<Segment>, RouteError> {
        let resp = self
            .client
            .post(&self.url)
            .header("content-type", "application/json")
            .body(valhalla_request(points).to_string())
            .send()
            .await
            .context("could not reach the routing service")?;
        let status = resp.status();
        let body = resp.bytes().await.context("reading the routing response")?;
        parse_valhalla(status.as_u16(), &body)
    }

    async fn ready(&self) -> bool {
        matches!(
            self.client.get(&self.status_url).timeout(Duration::from_secs(5)).send().await,
            Ok(r) if r.status().is_success()
        )
    }
}

/// The JSON body for a pedestrian route through `points` ([lng, lat]).
pub fn valhalla_request(points: &[[f64; 2]]) -> Value {
    json!({
        "locations": points.iter().map(|p| json!({ "lat": p[1], "lon": p[0], "type": "break" })).collect::<Vec<_>>(),
        "costing": "pedestrian",
        "directions_options": { "units": "kilometers", "language": "en-US" },
    })
}

/// Turns a Valhalla /route response into one segment per leg.
pub fn parse_valhalla(status: u16, body: &[u8]) -> Result<Vec<Segment>, RouteError> {
    let v: Value = serde_json::from_slice(body).with_context(|| {
        format!("routing service sent HTTP {status} with a body that is not JSON")
    })?;
    if !(200..300).contains(&status) {
        let message = v["error"].as_str().unwrap_or("unknown error").to_string();
        let code = v["error_code"].as_i64();
        // 4xx with a Valhalla error code is an answer about the input (no
        // path, a point too far from any path). Anything else is the
        // service failing, worth retrying.
        return if (400..500).contains(&status) && code.is_some() {
            Err(RouteError::NoRoute(format!(
                "{message} (Valhalla error {})",
                code.unwrap_or_default()
            )))
        } else {
            Err(RouteError::Unavailable(anyhow::anyhow!(
                "routing service failed: HTTP {status}: {message}"
            )))
        };
    }
    let legs = v["trip"]["legs"]
        .as_array()
        .context("routing response has no trip.legs")?;
    legs.iter()
        .map(|leg| {
            let shape = leg["shape"].as_str().context("routing leg has no shape")?;
            Ok(Segment {
                coordinates: decode_polyline6(shape)?,
                distance_m: leg["summary"]["length"].as_f64().unwrap_or(0.0) * 1000.0,
                duration_s: leg["summary"]["time"].as_f64().unwrap_or(0.0),
                maneuvers: leg["maneuvers"]
                    .as_array()
                    .map(|ms| {
                        ms.iter()
                            .filter_map(|m| {
                                Some(Maneuver {
                                    instruction: m["instruction"].as_str()?.to_string(),
                                    distance_m: m["length"].as_f64().unwrap_or(0.0) * 1000.0,
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()
        .map_err(RouteError::Unavailable)
}

/// Decodes a Google-style encoded polyline at 1e-6 precision, Valhalla's
/// default shape format, into [lng, lat] pairs.
pub fn decode_polyline6(encoded: &str) -> anyhow::Result<Vec<[f64; 2]>> {
    let bytes = encoded.as_bytes();
    let (mut i, mut lat, mut lng) = (0usize, 0i64, 0i64);
    let mut out = Vec::new();
    let next = |i: &mut usize| -> anyhow::Result<i64> {
        let (mut result, mut shift) = (0i64, 0u32);
        loop {
            let b = *bytes.get(*i).context("polyline ends mid-value")? as i64 - 63;
            *i += 1;
            anyhow::ensure!((0..64).contains(&b), "polyline has an invalid character");
            result |= (b & 0x1f) << shift;
            shift += 5;
            if b < 0x20 {
                break;
            }
            anyhow::ensure!(shift < 64, "polyline value too long");
        }
        Ok(if result & 1 != 0 {
            !(result >> 1)
        } else {
            result >> 1
        })
    };
    while i < bytes.len() {
        lat += next(&mut i)?;
        lng += next(&mut i)?;
        out.push([lng as f64 / 1e6, lat as f64 / 1e6]);
    }
    Ok(out)
}

/// Fingerprint of the ordered stops: which stops, in what order, where.
pub fn stops_hash(stops: &[Stop]) -> String {
    let mut h = Sha256::new();
    for s in stops {
        h.update(format!("{}:{:.6},{:.6}\n", s.id, s.lat, s.lng).as_bytes());
    }
    format!("{:x}", h.finalize())
}

fn same_spot(a: &Stop, b: &Stop) -> bool {
    (a.lat - b.lat).abs() < 1e-7 && (a.lng - b.lng).abs() < 1e-7
}

/// Routes through every stop, in order. Consecutive stops at the same spot
/// become zero-length legs; the rest go to the router in windows of at most
/// MAX_LOCATIONS points that overlap by one, so the pieces join up.
pub async fn route_stops<R: Router>(router: &R, stops: &[Stop]) -> Result<Route, RouteError> {
    // Distinct consecutive points, and which point each stop sits at.
    let mut points: Vec<[f64; 2]> = Vec::new();
    let mut at: Vec<usize> = Vec::with_capacity(stops.len());
    for (i, s) in stops.iter().enumerate() {
        if i == 0 || !same_spot(&stops[i - 1], s) {
            points.push([s.lng, s.lat]);
        }
        at.push(points.len() - 1);
    }

    let mut segments: Vec<Segment> = Vec::new();
    let mut start = 0;
    while start + 1 < points.len() {
        let end = (start + MAX_LOCATIONS).min(points.len());
        let got = router.route(&points[start..end]).await?;
        if got.len() != end - start - 1 {
            return Err(RouteError::Unavailable(anyhow::anyhow!(
                "router returned {} legs for {} points",
                got.len(),
                end - start
            )));
        }
        segments.extend(got);
        start = end - 1;
    }

    let mut coordinates: Vec<[f64; 2]> = Vec::new();
    for seg in &segments {
        let skip = usize::from(
            coordinates.last().is_some() && coordinates.last() == seg.coordinates.first(),
        );
        coordinates.extend(seg.coordinates.iter().skip(skip).copied());
    }
    let legs = stops
        .windows(2)
        .zip(at.windows(2))
        .map(|(pair, idx)| {
            let seg = (idx[0] != idx[1]).then(|| &segments[idx[0]]);
            Leg {
                from_stop: pair[0].id,
                to_stop: pair[1].id,
                distance_m: seg.map_or(0.0, |s| s.distance_m),
                duration_s: seg.map_or(0.0, |s| s.duration_s),
                maneuvers: seg.map(|s| s.maneuvers.clone()).unwrap_or_default(),
            }
        })
        .collect::<Vec<_>>();
    Ok(Route {
        coordinates,
        distance_m: segments.iter().map(|s| s.distance_m).sum(),
        duration_s: segments.iter().map(|s| s.duration_s).sum(),
        legs,
    })
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Routed,
    Unchanged,
    TooFewStops,
    NoRoute,
    Superseded,
}

pub async fn poll_once<R: Router>(
    db: &PgPool,
    router: &R,
    batch_size: i64,
    max_attempts: i32,
    stuck_timeout_secs: i64,
) -> anyhow::Result<()> {
    requeue_stuck(db, stuck_timeout_secs, max_attempts).await?;
    prune_processed(db, 100).await?;
    // Leave the jobs queued, attempts untouched, until the router answers.
    let waiting: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM trail_route_jobs WHERE status = 'queued' AND available_at <= now())",
    )
    .fetch_one(db)
    .await?;
    if !waiting {
        return Ok(());
    }
    if !router.ready().await {
        tracing::debug!("routing service not ready; route jobs wait");
        return Ok(());
    }
    for job in claim(db, batch_size).await? {
        match process(db, router, &job).await {
            Ok(outcome) => {
                tracing::info!(job_id = %job.id, trail_id = %job.trail_id, ?outcome, "trail route finished");
            }
            Err(err) => {
                let message = format!("{err:#}");
                mark_failed(db, &job, max_attempts, &message).await?;
                tracing::warn!(job_id = %job.id, trail_id = %job.trail_id,
                    attempts = job.attempts, error = %message, "trail route failed; will retry");
            }
        }
    }
    Ok(())
}

/// The trail's live, located stops, in order, with coordinates resolved
/// through the parent chain the same way the API does.
async fn load_stops(db: &PgPool, trail_id: Uuid) -> anyhow::Result<Vec<Stop>> {
    Ok(sqlx::query_as::<_, Stop>(
        r#"
        WITH RECURSIVE resolved AS (
            SELECT id, parent_id, lat AS effective_lat, lng AS effective_lng
            FROM artifacts WHERE parent_id IS NULL
            UNION ALL
            SELECT a.id, a.parent_id, COALESCE(a.lat, r.effective_lat), COALESCE(a.lng, r.effective_lng)
            FROM artifacts a JOIN resolved r ON a.parent_id = r.id
        )
        SELECT s.id, r.effective_lat AS lat, r.effective_lng AS lng
        FROM trail_stops s
        JOIN artifacts a ON a.id = s.artifact_id
        JOIN resolved r ON r.id = a.id
        WHERE s.trail_id = $1 AND a.deleted_at IS NULL
          AND r.effective_lat IS NOT NULL AND r.effective_lng IS NOT NULL
        ORDER BY s.position
        "#,
    )
    .bind(trail_id)
    .fetch_all(db)
    .await?)
}

async fn process<R: Router>(db: &PgPool, router: &R, job: &Job) -> anyhow::Result<Outcome> {
    let stops = load_stops(db, job.trail_id).await?;
    let hash = stops_hash(&stops);

    let current: Option<(String, String)> =
        sqlx::query_as("SELECT stops_hash, status FROM trail_routes WHERE trail_id = $1")
            .bind(job.trail_id)
            .fetch_optional(db)
            .await?;
    let unchanged = matches!(&current, Some((h, st)) if *h == hash && st != "failed");

    let (outcome, write): (Outcome, Option<RouteRow>) = if unchanged {
        (Outcome::Unchanged, None)
    } else if stops.len() < 2 {
        (
            Outcome::TooFewStops,
            Some(RouteRow::status("too_few_stops", &hash, None)),
        )
    } else {
        match route_stops(router, &stops).await {
            Ok(route) => (Outcome::Routed, Some(RouteRow::ok(&hash, &route))),
            Err(RouteError::NoRoute(message)) => (
                Outcome::NoRoute,
                Some(RouteRow::status("failed", &hash, Some(message))),
            ),
            Err(RouteError::Unavailable(err)) => return Err(err),
        }
    };

    let mut tx = db.begin().await?;
    let owns_claim: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM trail_route_jobs WHERE id = $1 AND status = 'processing' AND claim_token = $2 FOR UPDATE",
    )
    .bind(job.id)
    .bind(job.claim_token)
    .fetch_optional(&mut *tx)
    .await?;
    if owns_claim.is_none() {
        tx.commit().await?;
        return Ok(Outcome::Superseded);
    }
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("trail-route:{}", job.trail_id))
        .execute(&mut *tx)
        .await?;
    let trail_exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM trails WHERE id = $1)")
            .bind(job.trail_id)
            .fetch_one(&mut *tx)
            .await?;
    if let (Some(row), true) = (write, trail_exists) {
        sqlx::query(
            r#"
            INSERT INTO trail_routes (trail_id, status, stops_hash, geometry, distance_m, duration_s, legs, error_message, computed_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now())
            ON CONFLICT (trail_id) DO UPDATE SET
                status = EXCLUDED.status, stops_hash = EXCLUDED.stops_hash, geometry = EXCLUDED.geometry,
                distance_m = EXCLUDED.distance_m, duration_s = EXCLUDED.duration_s, legs = EXCLUDED.legs,
                error_message = EXCLUDED.error_message, computed_at = EXCLUDED.computed_at
            "#,
        )
        .bind(job.trail_id)
        .bind(row.status)
        .bind(row.stops_hash)
        .bind(row.geometry)
        .bind(row.distance_m)
        .bind(row.duration_s)
        .bind(row.legs)
        .bind(row.error_message)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "UPDATE trail_route_jobs SET status = 'processed', claimed_at = NULL, claim_token = NULL, \
         error_message = NULL WHERE id = $1",
    )
    .bind(job.id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(outcome)
}

struct RouteRow {
    status: &'static str,
    stops_hash: String,
    geometry: Option<Value>,
    distance_m: Option<f64>,
    duration_s: Option<f64>,
    legs: Value,
    error_message: Option<String>,
}

impl RouteRow {
    fn status(status: &'static str, hash: &str, error: Option<String>) -> Self {
        Self {
            status,
            stops_hash: hash.to_string(),
            geometry: None,
            distance_m: None,
            duration_s: None,
            legs: json!([]),
            error_message: error.map(|e| e.chars().take(1000).collect()),
        }
    }

    fn ok(hash: &str, route: &Route) -> Self {
        let round = |x: f64| (x * 1e6).round() / 1e6;
        Self {
            status: "ok",
            stops_hash: hash.to_string(),
            geometry: Some(json!({
                "type": "LineString",
                "coordinates": route.coordinates.iter().map(|c| [round(c[0]), round(c[1])]).collect::<Vec<_>>(),
            })),
            distance_m: Some(route.distance_m.round()),
            duration_s: Some(route.duration_s.round()),
            legs: serde_json::to_value(&route.legs).unwrap_or_else(|_| json!([])),
            error_message: None,
        }
    }
}

async fn claim(db: &PgPool, limit: i64) -> anyhow::Result<Vec<Job>> {
    Ok(sqlx::query_as::<_, Job>(
        r#"
        WITH next_jobs AS (
            SELECT id FROM trail_route_jobs
            WHERE status = 'queued' AND available_at <= now()
            ORDER BY available_at, created_at, id
            FOR UPDATE SKIP LOCKED
            LIMIT $1
        )
        UPDATE trail_route_jobs j
        SET status = 'processing', attempts = j.attempts + 1,
            claimed_at = now(), claim_token = gen_random_uuid()
        FROM next_jobs n WHERE j.id = n.id
        RETURNING j.id, j.trail_id, j.attempts, j.claim_token
        "#,
    )
    .bind(limit)
    .fetch_all(db)
    .await?)
}

async fn mark_failed(db: &PgPool, job: &Job, max_attempts: i32, error: &str) -> anyhow::Result<()> {
    let message: String = error.chars().take(1000).collect();
    sqlx::query(
        r#"
        UPDATE trail_route_jobs
        SET status = CASE WHEN attempts >= $3 THEN 'failed' ELSE 'queued' END,
            available_at = now() + make_interval(secs => LEAST(300, power(2, attempts))::double precision),
            claimed_at = NULL, claim_token = NULL, error_message = $4
        WHERE id = $1 AND status = 'processing' AND claim_token = $2
        "#,
    )
    .bind(job.id)
    .bind(job.claim_token)
    .bind(max_attempts)
    .bind(message)
    .execute(db)
    .await?;
    Ok(())
}

async fn requeue_stuck(db: &PgPool, timeout_secs: i64, max_attempts: i32) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE trail_route_jobs
        SET status = CASE WHEN attempts >= $2 THEN 'failed' ELSE 'queued' END,
            claimed_at = NULL, claim_token = NULL, available_at = now(),
            error_message = 'worker claim expired before completion'
        WHERE status = 'processing' AND claimed_at < now() - make_interval(secs => $1)
        "#,
    )
    .bind(timeout_secs as f64)
    .bind(max_attempts)
    .execute(db)
    .await?;
    Ok(())
}

async fn prune_processed(db: &PgPool, limit: i64) -> anyhow::Result<u64> {
    Ok(sqlx::query(
        r#"
        WITH expired AS (
            SELECT id FROM trail_route_jobs
            WHERE status = 'processed' AND updated_at < now() - interval '7 days'
            ORDER BY updated_at, id
            FOR UPDATE SKIP LOCKED
            LIMIT $1
        )
        DELETE FROM trail_route_jobs j USING expired e WHERE j.id = e.id
        "#,
    )
    .bind(limit)
    .execute(db)
    .await?
    .rows_affected())
}

#[cfg(test)]
mod tests;
