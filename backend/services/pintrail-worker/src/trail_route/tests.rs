use std::sync::Mutex;

use super::*;

/// Encodes [lng, lat] pairs the way Valhalla does (polyline, 1e-6).
fn encode_polyline6(points: &[[f64; 2]]) -> String {
    fn push(out: &mut String, v: i64) {
        let mut v = if v < 0 { !(v << 1) } else { v << 1 };
        while v >= 0x20 {
            out.push((((v & 0x1f) | 0x20) as u8 + 63) as char);
            v >>= 5;
        }
        out.push((v as u8 + 63) as char);
    }
    let (mut out, mut plat, mut plng) = (String::new(), 0i64, 0i64);
    for p in points {
        let (lat, lng) = ((p[1] * 1e6).round() as i64, (p[0] * 1e6).round() as i64);
        push(&mut out, lat - plat);
        push(&mut out, lng - plng);
        (plat, plng) = (lat, lng);
    }
    out
}

#[test]
fn polyline6_round_trips_including_negative_steps() {
    let pts = [
        [-72.537096, 42.389956],
        [-72.536, 42.3911],
        [-72.522155, 42.384618],
    ];
    let decoded = decode_polyline6(&encode_polyline6(&pts)).unwrap();
    assert_eq!(decoded.len(), 3);
    for (a, b) in decoded.iter().zip(pts.iter()) {
        assert!((a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6);
    }
    assert!(
        decode_polyline6("_").is_err(),
        "truncated input is an error, not a panic"
    );
}

#[test]
fn parses_a_valhalla_route_and_its_errors() {
    let shape = encode_polyline6(&[[-72.5371, 42.3900], [-72.5222, 42.3846]]);
    let body = json!({"trip": {"legs": [{"shape": shape, "summary": {"length": 1.749, "time": 1284.0},
        "maneuvers": [{"instruction": "Walk south.", "length": 0.065}, {"instruction": "Turn left onto the walkway.", "length": 0.178}]}],
        "summary": {"length": 1.749, "time": 1284.0}}});
    let segs = parse_valhalla(200, body.to_string().as_bytes()).unwrap();
    assert_eq!(segs.len(), 1);
    assert_eq!(segs[0].coordinates.len(), 2);
    assert!((segs[0].distance_m - 1749.0).abs() < 0.01);
    assert_eq!(
        segs[0].maneuvers[1].instruction,
        "Turn left onto the walkway."
    );
    assert!((segs[0].maneuvers[0].distance_m - 65.0).abs() < 0.01);

    let no_path =
        json!({"error_code": 442, "error": "No path could be found for input", "status_code": 400});
    assert!(
        matches!(parse_valhalla(400, no_path.to_string().as_bytes()), Err(RouteError::NoRoute(m)) if m.contains("No path"))
    );
    let down = json!({"error": "overloaded"});
    assert!(matches!(
        parse_valhalla(503, down.to_string().as_bytes()),
        Err(RouteError::Unavailable(_))
    ));
    assert!(matches!(
        parse_valhalla(502, b"<html>bad gateway</html>"),
        Err(RouteError::Unavailable(_))
    ));
}

#[test]
fn request_lists_points_as_lat_lon_for_walking() {
    let req = valhalla_request(&[[-72.53, 42.39], [-72.52, 42.38]]);
    assert_eq!(req["costing"], "pedestrian");
    assert_eq!(req["locations"][0]["lat"], 42.39);
    assert_eq!(req["locations"][0]["lon"], -72.53);
}

/// Straight lines, 100 m and 60 s a leg, recording each request's size.
#[derive(Default)]
struct FakeRouter {
    calls: Mutex<Vec<usize>>,
    answer: Option<fn() -> RouteError>,
}

impl Router for FakeRouter {
    async fn route(&self, points: &[[f64; 2]]) -> Result<Vec<Segment>, RouteError> {
        self.calls.lock().unwrap().push(points.len());
        if let Some(err) = self.answer {
            return Err(err());
        }
        Ok(points
            .windows(2)
            .map(|w| Segment {
                coordinates: vec![w[0], w[1]],
                distance_m: 100.0,
                duration_s: 60.0,
                maneuvers: vec![Maneuver {
                    instruction: "Walk.".into(),
                    distance_m: 100.0,
                }],
            })
            .collect())
    }
}

fn stop(n: u128, lat: f64, lng: f64) -> Stop {
    Stop {
        id: Uuid::from_u128(n),
        lat,
        lng,
    }
}

#[tokio::test]
async fn stops_at_the_same_spot_get_a_zero_leg_and_no_request() {
    let router = FakeRouter::default();
    // Two features of one building, then the next building.
    let stops = [
        stop(1, 42.39, -72.53),
        stop(2, 42.39, -72.53),
        stop(3, 42.38, -72.52),
    ];
    let route = route_stops(&router, &stops).await.unwrap();
    assert_eq!(
        *router.calls.lock().unwrap(),
        [2],
        "one request with the two distinct points"
    );
    assert_eq!(route.legs.len(), 2);
    assert_eq!(
        (route.legs[0].from_stop, route.legs[0].to_stop),
        (stops[0].id, stops[1].id)
    );
    assert_eq!(route.legs[0].distance_m, 0.0);
    assert!(route.legs[0].maneuvers.is_empty());
    assert_eq!(route.legs[1].distance_m, 100.0);
    assert_eq!(route.distance_m, 100.0);
    assert_eq!(route.coordinates.len(), 2);
}

#[tokio::test]
async fn long_trails_are_routed_in_overlapping_windows() {
    let router = FakeRouter::default();
    let stops: Vec<Stop> = (0..120)
        .map(|i| stop(i, 42.3 + i as f64 * 0.001, -72.5))
        .collect();
    let route = route_stops(&router, &stops).await.unwrap();
    // 120 points: 1..50, 50..99, 99..120 (each window repeats the last point).
    assert_eq!(*router.calls.lock().unwrap(), [50, 50, 22]);
    assert_eq!(route.legs.len(), 119);
    assert_eq!(route.distance_m, 119.0 * 100.0);
    // The joins are not duplicated in the line.
    assert_eq!(route.coordinates.len(), 120);
}

// --- with the database ------------------------------------------------------

async fn trail_with_stops(db: &PgPool, coords: &[(f64, f64)]) -> anyhow::Result<(Uuid, Vec<Uuid>)> {
    let author: Uuid = sqlx::query_scalar(
        "INSERT INTO authors (email, password_hash, role) VALUES ($1, 'x', 'editor') RETURNING id",
    )
    .bind(format!("{}@example.test", Uuid::new_v4()))
    .fetch_one(db)
    .await?;
    let trail: Uuid = sqlx::query_scalar(
        "INSERT INTO trails (title, owner_type, owner_id) VALUES ('Walk', 'author', $1) RETURNING id",
    )
    .bind(author)
    .fetch_one(db)
    .await?;
    let mut artifacts = Vec::new();
    for (i, (lat, lng)) in coords.iter().enumerate() {
        let a: Uuid = sqlx::query_scalar(
            "INSERT INTO artifacts (kind, name, description, lat, lng) VALUES ('other', $1, 'x', $2, $3) RETURNING id",
        )
        .bind(format!("Stop {i}"))
        .bind(lat)
        .bind(lng)
        .fetch_one(db)
        .await?;
        sqlx::query(
            "INSERT INTO trail_stops (trail_id, artifact_id, position) VALUES ($1, $2, $3)",
        )
        .bind(trail)
        .bind(a)
        .bind(i as i32)
        .execute(db)
        .await?;
        artifacts.push(a);
    }
    Ok((trail, artifacts))
}

async fn queued(db: &PgPool, trail: Uuid) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT count(*) FROM trail_route_jobs WHERE trail_id = $1 AND status = 'queued'",
    )
    .bind(trail)
    .fetch_one(db)
    .await?)
}

async fn route_row(
    db: &PgPool,
    trail: Uuid,
) -> anyhow::Result<(String, Option<f64>, Value, Option<String>)> {
    Ok(sqlx::query_as(
        "SELECT status, distance_m, legs, error_message FROM trail_routes WHERE trail_id = $1",
    )
    .bind(trail)
    .fetch_one(db)
    .await?)
}

#[sqlx::test(migrations = "../../migrations")]
async fn stop_changes_queue_one_job_and_the_worker_stores_the_route(
    db: PgPool,
) -> anyhow::Result<()> {
    let (trail, artifacts) = trail_with_stops(
        &db,
        &[(42.39, -72.537), (42.385, -72.522), (42.391, -72.526)],
    )
    .await?;
    assert_eq!(
        queued(&db, trail).await?,
        1,
        "three inserts, one waiting job"
    );

    let router = FakeRouter::default();
    poll_once(&db, &router, 10, 3, 300).await?;
    let (status, distance, legs, _) = route_row(&db, trail).await?;
    assert_eq!(status, "ok");
    assert_eq!(distance, Some(200.0));
    assert_eq!(legs.as_array().unwrap().len(), 2);
    let geometry: Value =
        sqlx::query_scalar("SELECT geometry FROM trail_routes WHERE trail_id = $1")
            .bind(trail)
            .fetch_one(&db)
            .await?;
    assert_eq!(geometry["type"], "LineString");

    // Nothing changed: a re-queued job doesn't call the router again.
    sqlx::query("SELECT enqueue_trail_route($1)")
        .bind(trail)
        .execute(&db)
        .await?;
    poll_once(&db, &router, 10, 3, 300).await?;
    assert_eq!(router.calls.lock().unwrap().len(), 1);

    // Moving an artifact re-routes; so does reordering.
    sqlx::query("UPDATE artifacts SET lat = 42.392 WHERE id = $1")
        .bind(artifacts[2])
        .execute(&db)
        .await?;
    assert_eq!(queued(&db, trail).await?, 1);
    poll_once(&db, &router, 10, 3, 300).await?;
    assert_eq!(router.calls.lock().unwrap().len(), 2);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn moving_a_parent_reroutes_trails_through_its_children(db: PgPool) -> anyhow::Result<()> {
    let (trail, artifacts) = trail_with_stops(&db, &[(42.39, -72.537), (42.385, -72.522)]).await?;
    // A child with no location of its own, inside the first stop's artifact,
    // becomes a third stop.
    let child: Uuid = sqlx::query_scalar(
        "INSERT INTO artifacts (kind, name, description, parent_id) VALUES ('other', 'Child', 'x', $1) RETURNING id",
    )
    .bind(artifacts[0])
    .fetch_one(&db)
    .await?;
    sqlx::query("INSERT INTO trail_stops (trail_id, artifact_id, position) VALUES ($1, $2, 2)")
        .bind(trail)
        .bind(child)
        .execute(&db)
        .await?;
    poll_once(&db, &FakeRouter::default(), 10, 3, 300).await?;
    assert_eq!(queued(&db, trail).await?, 0);
    sqlx::query("UPDATE artifacts SET lng = -72.530 WHERE id = $1")
        .bind(artifacts[0])
        .execute(&db)
        .await?;
    assert_eq!(queued(&db, trail).await?, 1);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn one_stop_is_too_few_and_no_path_is_stored_not_retried(db: PgPool) -> anyhow::Result<()> {
    let (lonely, _) = trail_with_stops(&db, &[(42.39, -72.537)]).await?;
    poll_once(&db, &FakeRouter::default(), 10, 3, 300).await?;
    assert_eq!(route_row(&db, lonely).await?.0, "too_few_stops");

    let (trail, _) = trail_with_stops(&db, &[(42.39, -72.537), (41.0, -70.0)]).await?;
    let router = FakeRouter {
        answer: Some(|| RouteError::NoRoute("No path could be found".into())),
        ..Default::default()
    };
    poll_once(&db, &router, 10, 3, 300).await?;
    let (status, _, _, error) = route_row(&db, trail).await?;
    assert_eq!(status, "failed");
    assert!(error.unwrap().contains("No path"));
    assert_eq!(queued(&db, trail).await?, 0, "an answer is not retried");
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_router_that_is_down_is_retried_with_backoff(db: PgPool) -> anyhow::Result<()> {
    let (trail, _) = trail_with_stops(&db, &[(42.39, -72.537), (42.385, -72.522)]).await?;
    let router = FakeRouter {
        answer: Some(|| RouteError::Unavailable(anyhow::anyhow!("connection refused"))),
        ..Default::default()
    };
    poll_once(&db, &router, 10, 3, 300).await?;
    let (status, attempts, waiting): (String, i32, bool) = sqlx::query_as(
        "SELECT status, attempts, available_at > now() FROM trail_route_jobs WHERE trail_id = $1",
    )
    .bind(trail)
    .fetch_one(&db)
    .await?;
    assert_eq!((status.as_str(), attempts, waiting), ("queued", 1, true));
    let none: Option<String> =
        sqlx::query_scalar("SELECT status FROM trail_routes WHERE trail_id = $1")
            .bind(trail)
            .fetch_optional(&db)
            .await?;
    assert!(
        none.is_none(),
        "no route is stored while the router is down"
    );
    Ok(())
}
