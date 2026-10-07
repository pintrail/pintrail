-- What the phone app may show: only approved artifacts.
--
-- An artifact is *published* when it and every artifact above it are
-- approved and not deleted. Requiring the whole chain means a draft building
-- hides the approved rooms inside it, which otherwise would turn up in the
-- app with no building to belong to.
--
-- Additive only. Authors (the Studio, the authoring API) still see every
-- artifact; readers see published ones (artifacts/routes.rs).

CREATE FUNCTION artifact_is_published(p_id UUID) RETURNS BOOLEAN AS $$
    WITH RECURSIVE chain AS (
        SELECT id, parent_id, status, deleted_at FROM artifacts WHERE id = p_id
        UNION ALL
        SELECT a.id, a.parent_id, a.status, a.deleted_at
        FROM artifacts a JOIN chain c ON a.id = c.parent_id
    )
    SELECT count(*) > 0 AND bool_and(status = 'approved' AND deleted_at IS NULL)
    FROM chain
$$ LANGUAGE sql STABLE;

-- Incremental sync sends every artifact whose sync_version moved. Approving
-- or un-approving a building, or moving an artifact to a different parent,
-- changes whether everything *beneath* it is published without touching
-- those rows, so they are marked dirty here, the same way the location
-- trigger does for inherited coordinates. The no-op write changes neither
-- status nor parent_id, so this cannot re-fire itself, and the history
-- trigger skips it because nothing it records changed.
CREATE FUNCTION propagate_artifact_publication_change() RETURNS TRIGGER AS $$
BEGIN
    WITH RECURSIVE descendants AS (
        SELECT id FROM artifacts WHERE parent_id = NEW.id
        UNION ALL
        SELECT a.id FROM artifacts a JOIN descendants d ON a.parent_id = d.id
    )
    UPDATE artifacts SET lat = lat WHERE id IN (SELECT id FROM descendants);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER artifacts_propagate_publication
    AFTER UPDATE ON artifacts
    FOR EACH ROW
    WHEN (OLD.status IS DISTINCT FROM NEW.status OR OLD.parent_id IS DISTINCT FROM NEW.parent_id)
    EXECUTE FUNCTION propagate_artifact_publication_change();
