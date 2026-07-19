-- Artifacts: the nestable things explorers discover (docs/DESIGN.md 2.2).
--
-- Two deliberate deviations from the column list in the design doc, both
-- noted in backend/README.md:
--
--   1. `desc` is renamed `description`. `desc` is a reserved SQL keyword that
--      would need quoting at every reference, and `trails` already spells the
--      same concept `description`.
--   2. `sync_version` and `deleted_at` are added to support the incremental
--      GET /artifacts/sync?since=<version> contract. Without a tombstone, a
--      client that cached an artifact has no way to learn it was removed and
--      would keep showing it forever.

-- Global monotonic counter shared by every artifact row. A sequence rather
-- than a timestamp because two rows written in the same microsecond must
-- still get distinct, ordered versions, and sequences are immune to clock
-- skew and NTP steps.
CREATE SEQUENCE artifact_sync_seq;

CREATE TABLE artifacts (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    kind         artifact_kind NOT NULL DEFAULT 'other',
    name         VARCHAR(200) NOT NULL DEFAULT '',
    description  TEXT NOT NULL DEFAULT '',

    -- NULL means "inherit from the nearest ancestor that has coordinates".
    -- Indoor artifacts (a room, a painting) genuinely have no useful GPS fix.
    lat          DOUBLE PRECISION,
    lng          DOUBLE PRECISION,

    parent_id    UUID REFERENCES artifacts (id) ON DELETE CASCADE,
    beacon_id    VARCHAR(128),   -- reserved for v2 BLE precision; unused in v1

    sync_version BIGINT NOT NULL DEFAULT nextval('artifact_sync_seq'),
    deleted_at   TIMESTAMPTZ,

    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- Coordinates are meaningful only as a pair; one without the other is a
    -- data-entry bug, not an inheritance signal.
    CONSTRAINT artifacts_latlng_paired CHECK (
        (lat IS NULL AND lng IS NULL) OR (lat IS NOT NULL AND lng IS NOT NULL)
    ),
    CONSTRAINT artifacts_lat_range CHECK (lat IS NULL OR (lat BETWEEN -90 AND 90)),
    CONSTRAINT artifacts_lng_range CHECK (lng IS NULL OR (lng BETWEEN -180 AND 180)),
    CONSTRAINT artifacts_not_own_parent CHECK (parent_id IS DISTINCT FROM id)
);

CREATE INDEX artifacts_parent_id_idx ON artifacts (parent_id);
CREATE INDEX artifacts_sync_version_idx ON artifacts (sync_version);
CREATE UNIQUE INDEX artifacts_beacon_id_key ON artifacts (beacon_id)
    WHERE beacon_id IS NOT NULL;

CREATE TRIGGER artifacts_set_updated_at
    BEFORE UPDATE ON artifacts
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- Every mutation advances the row's sync version so incremental sync sees it.
-- Soft deletes go through this path too: the client receives the row with
-- deleted_at set and evicts it from its local cache.
CREATE FUNCTION bump_artifact_sync_version() RETURNS TRIGGER AS $$
BEGIN
    NEW.sync_version = nextval('artifact_sync_seq');
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER artifacts_bump_sync_version
    BEFORE UPDATE ON artifacts
    FOR EACH ROW EXECUTE FUNCTION bump_artifact_sync_version();

-- The parent chain must stay a tree: coordinate inheritance walks up
-- parent_id, and a cycle would make that walk loop forever. The CHECK above
-- only catches self-parenting; this catches longer cycles (A->B->A).
CREATE FUNCTION assert_artifact_acyclic() RETURNS TRIGGER AS $$
DECLARE
    cursor_id UUID := NEW.parent_id;
    hops INT := 0;
BEGIN
    WHILE cursor_id IS NOT NULL LOOP
        IF cursor_id = NEW.id THEN
            RAISE EXCEPTION 'artifact parent cycle: % cannot be its own ancestor', NEW.id
                USING ERRCODE = 'check_violation';
        END IF;

        hops := hops + 1;
        IF hops > 64 THEN
            RAISE EXCEPTION 'artifact parent chain exceeds 64 levels'
                USING ERRCODE = 'check_violation';
        END IF;

        SELECT parent_id INTO cursor_id FROM artifacts WHERE id = cursor_id;
    END LOOP;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER artifacts_assert_acyclic
    BEFORE INSERT OR UPDATE OF parent_id ON artifacts
    FOR EACH ROW WHEN (NEW.parent_id IS NOT NULL)
    EXECUTE FUNCTION assert_artifact_acyclic();
