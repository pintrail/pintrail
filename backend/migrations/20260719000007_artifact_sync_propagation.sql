-- Propagates a location change to the artifacts that inherit it.
--
-- Incremental sync hands the client every artifact with sync_version > since.
-- But an artifact with NULL coordinates reports its nearest ancestor's
-- position, so moving a building silently changes the effective location of
-- every room inside it -- without touching those rooms' own sync_version.
--
-- The result would be a phone that geofences rooms at the building's old
-- coordinates indefinitely, with nothing in the sync protocol able to
-- correct it. The rows have to be marked dirty explicitly.

CREATE FUNCTION propagate_artifact_location_change() RETURNS TRIGGER AS $$
BEGIN
    -- Descend only through artifacts that actually inherit. A child with its
    -- own coordinates is unaffected, and so is everything beneath it, since
    -- those inherit from the child rather than from here.
    WITH RECURSIVE inheritors AS (
        SELECT id
        FROM artifacts
        WHERE parent_id = NEW.id AND lat IS NULL

        UNION ALL

        SELECT a.id
        FROM artifacts a
        JOIN inheritors i ON a.parent_id = i.id
        WHERE a.lat IS NULL
    )
    -- A no-op write whose only effect is the BEFORE trigger's
    -- sync_version = nextval(). Assigning lat to itself keeps the value
    -- unchanged, which also means this statement cannot re-fire the trigger
    -- below and recurse: its WHEN clause requires lat, lng, or parent_id to
    -- have actually changed.
    UPDATE artifacts SET lat = lat
    WHERE id IN (SELECT id FROM inheritors);

    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER artifacts_propagate_location
    AFTER UPDATE ON artifacts
    FOR EACH ROW
    WHEN (
        OLD.lat IS DISTINCT FROM NEW.lat
        OR OLD.lng IS DISTINCT FROM NEW.lng
        OR OLD.parent_id IS DISTINCT FROM NEW.parent_id
    )
    EXECUTE FUNCTION propagate_artifact_location_change();
