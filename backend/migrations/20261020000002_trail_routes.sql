-- Walking routes for trails (#91). The worker asks Valhalla (#90) for a
-- pedestrian route through a trail's stops, in order, and stores the result
-- here, so a map just draws it. Routes are derived data: the trail and its
-- stops stay the source of truth, and a route can always be recomputed.

CREATE TABLE trail_routes (
    trail_id      UUID PRIMARY KEY REFERENCES trails (id) ON DELETE CASCADE,
    -- ok: geometry and legs are filled in.
    -- too_few_stops: fewer than two stops with a location; nothing to draw.
    -- failed: the router could not connect the stops (error_message says why).
    status        TEXT NOT NULL CHECK (status IN ('ok', 'too_few_stops', 'failed')),
    -- SHA-256 of the ordered stop coordinates the route was computed for, so
    -- a route that no longer matches its stops is easy to spot.
    stops_hash    TEXT NOT NULL CHECK (stops_hash ~ '^[0-9a-f]{64}$'),
    -- GeoJSON LineString, [lng, lat] pairs, the whole walk.
    geometry      JSONB,
    distance_m    DOUBLE PRECISION CHECK (distance_m >= 0),
    duration_s    DOUBLE PRECISION CHECK (duration_s >= 0),
    -- One entry per pair of consecutive stops:
    -- {"from_stop": uuid, "to_stop": uuid, "distance_m", "duration_s",
    --  "maneuvers": [{"instruction", "distance_m"}]}
    legs          JSONB NOT NULL DEFAULT '[]',
    error_message TEXT,
    computed_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (status <> 'ok' OR (geometry IS NOT NULL AND distance_m IS NOT NULL AND duration_s IS NOT NULL)),
    CHECK (status <> 'failed' OR error_message IS NOT NULL)
);

-- The queue, shaped like search_index_jobs so it behaves the same way:
-- claimed with FOR UPDATE SKIP LOCKED, retried with backoff, recovered when a
-- claim goes stale, pruned a week after it finishes.
CREATE TABLE trail_route_jobs (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    trail_id      UUID NOT NULL REFERENCES trails (id) ON DELETE CASCADE,
    status        TEXT NOT NULL DEFAULT 'queued'
        CHECK (status IN ('queued', 'processing', 'processed', 'failed')),
    attempts      INT NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    available_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    claimed_at    TIMESTAMPTZ,
    claim_token   UUID,
    error_message TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((status = 'processing' AND claimed_at IS NOT NULL AND claim_token IS NOT NULL)
        OR (status <> 'processing' AND claimed_at IS NULL AND claim_token IS NULL)),
    CHECK (status <> 'failed' OR error_message IS NOT NULL)
);

CREATE INDEX trail_route_jobs_queue_idx
    ON trail_route_jobs (available_at, created_at, id) WHERE status = 'queued';
CREATE INDEX trail_route_jobs_stuck_idx
    ON trail_route_jobs (claimed_at) WHERE status = 'processing';
CREATE INDEX trail_route_jobs_processed_idx
    ON trail_route_jobs (updated_at, id) WHERE status = 'processed';
CREATE INDEX trail_route_jobs_trail_idx ON trail_route_jobs (trail_id);

CREATE TRIGGER trail_route_jobs_set_updated_at
    BEFORE UPDATE ON trail_route_jobs
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- Queues a route job for a trail unless one is already waiting. Waiting is
-- enough: the job reads the stops when it runs, not when it was queued.
CREATE FUNCTION enqueue_trail_route(p_trail UUID) RETURNS VOID AS $$
BEGIN
    INSERT INTO trail_route_jobs (trail_id)
    SELECT p_trail
    WHERE EXISTS (SELECT 1 FROM trails WHERE id = p_trail)
      AND NOT EXISTS (SELECT 1 FROM trail_route_jobs
                      WHERE trail_id = p_trail AND status = 'queued');
END;
$$ LANGUAGE plpgsql;

-- Any change to which stops a trail has, or their order.
CREATE FUNCTION trail_stops_enqueue_route() RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP IN ('UPDATE', 'DELETE') THEN
        PERFORM enqueue_trail_route(OLD.trail_id);
    END IF;
    IF TG_OP IN ('INSERT', 'UPDATE') AND (TG_OP = 'INSERT' OR NEW.trail_id <> OLD.trail_id) THEN
        PERFORM enqueue_trail_route(NEW.trail_id);
    END IF;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trail_stops_enqueue_route_insert_delete
    AFTER INSERT OR DELETE ON trail_stops
    FOR EACH ROW EXECUTE FUNCTION trail_stops_enqueue_route();

CREATE TRIGGER trail_stops_enqueue_route_update
    AFTER UPDATE ON trail_stops
    FOR EACH ROW
    WHEN (OLD.position IS DISTINCT FROM NEW.position
       OR OLD.artifact_id IS DISTINCT FROM NEW.artifact_id
       OR OLD.trail_id IS DISTINCT FROM NEW.trail_id)
    EXECUTE FUNCTION trail_stops_enqueue_route();

-- A stop's location is its artifact's own coordinates or, failing those, the
-- nearest ancestor's. So moving an artifact, or moving it under a different
-- parent, moves every stop at it or inside it. Deleting one makes its stops
-- drop out of the route.
CREATE FUNCTION artifacts_enqueue_routes() RETURNS TRIGGER AS $$
BEGIN
    PERFORM enqueue_trail_route(t.trail_id)
    FROM (
        WITH RECURSIVE subtree AS (
            SELECT NEW.id AS id
            UNION ALL
            SELECT a.id FROM artifacts a JOIN subtree s ON a.parent_id = s.id
        )
        SELECT DISTINCT ts.trail_id
        FROM trail_stops ts JOIN subtree st ON st.id = ts.artifact_id
    ) t;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER artifacts_enqueue_routes
    AFTER UPDATE ON artifacts
    FOR EACH ROW
    WHEN (OLD.lat IS DISTINCT FROM NEW.lat
       OR OLD.lng IS DISTINCT FROM NEW.lng
       OR OLD.parent_id IS DISTINCT FROM NEW.parent_id
       OR OLD.deleted_at IS DISTINCT FROM NEW.deleted_at)
    EXECUTE FUNCTION artifacts_enqueue_routes();

-- Backfill: every existing trail gets a job.
INSERT INTO trail_route_jobs (trail_id) SELECT id FROM trails;
