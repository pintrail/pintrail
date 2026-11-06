-- Refresh prepared text when its tag/topic sources change. Invalidate old text
-- immediately: public search must not expose a topic withdrawn during a backfill.
CREATE FUNCTION refresh_artifact_search_sources(ids UUID[]) RETURNS VOID LANGUAGE plpgsql AS $$
BEGIN
    -- Same artifact row lock used by the text worker. Sorting avoids taking
    -- two moved-link endpoints in opposite order.
    PERFORM id FROM artifacts WHERE id = ANY(ids) ORDER BY id FOR UPDATE;
    DELETE FROM search_documents WHERE artifact_id = ANY(ids);
    INSERT INTO search_index_jobs(artifact_id)
        SELECT id FROM artifacts WHERE id = ANY(ids) AND deleted_at IS NULL ORDER BY id;
END;
$$;

CREATE FUNCTION search_tag_or_link_changed() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        PERFORM refresh_artifact_search_sources(ARRAY[NEW.artifact_id]);
    ELSIF TG_OP = 'DELETE' THEN
        PERFORM refresh_artifact_search_sources(ARRAY[OLD.artifact_id]);
    ELSE
        PERFORM refresh_artifact_search_sources(ARRAY[OLD.artifact_id, NEW.artifact_id]);
    END IF;
    RETURN NULL;
END;
$$;
CREATE TRIGGER artifact_tags_refresh_search
    AFTER INSERT OR DELETE ON artifact_tags FOR EACH ROW
    EXECUTE FUNCTION search_tag_or_link_changed();
CREATE TRIGGER artifact_tags_update_refresh_search
    AFTER UPDATE ON artifact_tags FOR EACH ROW
    WHEN (OLD.artifact_id IS DISTINCT FROM NEW.artifact_id OR OLD.tag IS DISTINCT FROM NEW.tag)
    EXECUTE FUNCTION search_tag_or_link_changed();
CREATE TRIGGER artifact_topics_refresh_search
    AFTER INSERT OR DELETE ON artifact_topics FOR EACH ROW
    EXECUTE FUNCTION search_tag_or_link_changed();
CREATE TRIGGER artifact_topics_update_refresh_search
    AFTER UPDATE ON artifact_topics FOR EACH ROW
    WHEN (OLD.artifact_id IS DISTINCT FROM NEW.artifact_id
       OR OLD.topic_id IS DISTINCT FROM NEW.topic_id OR OLD.note IS DISTINCT FROM NEW.note)
    EXECUTE FUNCTION search_tag_or_link_changed();

CREATE FUNCTION search_topic_changed() RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE affected UUID[];
BEGIN
    SELECT array_agg(artifact_id ORDER BY artifact_id) INTO affected
        FROM artifact_topics WHERE topic_id = NEW.id;
    IF affected IS NOT NULL THEN
        PERFORM refresh_artifact_search_sources(affected);
    END IF;
    RETURN NULL;
END;
$$;
CREATE TRIGGER topics_refresh_linked_search
    AFTER UPDATE ON artifacts FOR EACH ROW
    WHEN (NEW.is_topic AND (OLD.name IS DISTINCT FROM NEW.name
        OR OLD.status IS DISTINCT FROM NEW.status OR OLD.deleted_at IS DISTINCT FROM NEW.deleted_at))
    EXECUTE FUNCTION search_topic_changed();

-- Existing documents predate Tags/Topics lines. No model extension is required
-- for this migration; the optional embedding worker follows changed hashes.
INSERT INTO search_index_jobs(artifact_id)
    SELECT id FROM artifacts WHERE deleted_at IS NULL;
