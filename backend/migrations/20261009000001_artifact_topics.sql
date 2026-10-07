-- Topics (issue #49): a shared page that many artifacts link to.
--
-- A topic is something true of many artifacts rather than a place: "LEED
-- certification", "Valley Bike Share". It is an artifact row with
-- is_topic = true, so it gets authoring, review, publication, history,
-- links, media, and comments unchanged. What makes it different:
--
--   * it has no coordinates and no parent, and nothing may be nested in it,
--     so it never appears on the map or in the place hierarchy;
--   * it can't be a trail stop (you can't walk to "LEED");
--   * places link to it many-to-many through artifact_topics, each link with
--     an optional note ("Gold, 2019").
--
-- Additive only: the previous release ignores the column and the table.

ALTER TABLE artifacts ADD COLUMN is_topic BOOLEAN NOT NULL DEFAULT false;

ALTER TABLE artifacts ADD CONSTRAINT artifacts_topic_has_no_place CHECK (
    NOT is_topic OR (lat IS NULL AND lng IS NULL AND parent_id IS NULL AND beacon_id IS NULL)
);

CREATE INDEX artifacts_is_topic_idx ON artifacts (is_topic) WHERE deleted_at IS NULL;

-- A topic stays a topic and a place stays a place: flipping one would strand
-- its links or its children. And nothing may sit inside a topic.
CREATE FUNCTION assert_artifact_topic_rules() RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'UPDATE' AND NEW.is_topic IS DISTINCT FROM OLD.is_topic THEN
        RAISE EXCEPTION 'an artifact cannot be turned into a topic, or a topic into an artifact'
            USING ERRCODE = 'check_violation';
    END IF;
    IF NEW.parent_id IS NOT NULL
       AND (TG_OP = 'INSERT' OR NEW.parent_id IS DISTINCT FROM OLD.parent_id)
       AND EXISTS (SELECT 1 FROM artifacts WHERE id = NEW.parent_id AND is_topic)
    THEN
        RAISE EXCEPTION 'nothing can be nested inside a topic; link the artifact to the topic instead'
            USING ERRCODE = 'check_violation';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER artifacts_assert_topic_rules
    BEFORE INSERT OR UPDATE ON artifacts
    FOR EACH ROW EXECUTE FUNCTION assert_artifact_topic_rules();

-- The links. One row per (artifact, topic); the note says how the topic
-- applies to this artifact.
CREATE TABLE artifact_topics (
    artifact_id UUID NOT NULL REFERENCES artifacts (id) ON DELETE CASCADE,
    topic_id    UUID NOT NULL REFERENCES artifacts (id) ON DELETE CASCADE,
    note        VARCHAR(200) NOT NULL DEFAULT '',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (artifact_id, topic_id),
    CONSTRAINT artifact_topics_not_self CHECK (artifact_id <> topic_id)
);

CREATE INDEX artifact_topics_topic_idx ON artifact_topics (topic_id);

-- One end must be a place and the other a topic. A foreign key can't say
-- that, so a trigger does.
CREATE FUNCTION assert_artifact_topic_ends() RETURNS TRIGGER AS $$
BEGIN
    IF EXISTS (SELECT 1 FROM artifacts WHERE id = NEW.artifact_id AND is_topic) THEN
        RAISE EXCEPTION 'a topic cannot be linked to another topic'
            USING ERRCODE = 'check_violation';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM artifacts WHERE id = NEW.topic_id AND is_topic) THEN
        RAISE EXCEPTION 'artifacts can only be linked to a topic'
            USING ERRCODE = 'check_violation';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER artifact_topics_assert_ends
    BEFORE INSERT OR UPDATE ON artifact_topics
    FOR EACH ROW EXECUTE FUNCTION assert_artifact_topic_ends();

-- History, on both sides: the artifact's history says which topic it gained
-- or lost, and the topic's says which artifact. Names are recorded as they
-- were, so the entry still reads correctly after a rename or delete.
CREATE FUNCTION artifact_topics_log_history() RETURNS TRIGGER AS $$
DECLARE
    a_name TEXT;
    t_name TEXT;
BEGIN
    IF TG_OP = 'DELETE' THEN
        SELECT name INTO a_name FROM artifacts WHERE id = OLD.artifact_id;
        SELECT name INTO t_name FROM artifacts WHERE id = OLD.topic_id;
        PERFORM artifact_history_add(OLD.artifact_id, 'topic_removed', jsonb_build_object('topic', t_name));
        PERFORM artifact_history_add(OLD.topic_id, 'topic_artifact_removed', jsonb_build_object('artifact', a_name));
        RETURN NULL;
    END IF;

    SELECT name INTO a_name FROM artifacts WHERE id = NEW.artifact_id;
    SELECT name INTO t_name FROM artifacts WHERE id = NEW.topic_id;
    IF TG_OP = 'INSERT' THEN
        PERFORM artifact_history_add(NEW.artifact_id, 'topic_added',
            jsonb_strip_nulls(jsonb_build_object('topic', t_name, 'note', NULLIF(NEW.note, ''))));
        PERFORM artifact_history_add(NEW.topic_id, 'topic_artifact_added',
            jsonb_build_object('artifact', a_name));
    ELSIF OLD.note IS DISTINCT FROM NEW.note THEN
        PERFORM artifact_history_add(NEW.artifact_id, 'topic_edited',
            jsonb_build_object('topic', t_name, 'note', jsonb_build_object('from', OLD.note, 'to', NEW.note)));
    END IF;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER artifact_topics_log_history
    AFTER INSERT OR UPDATE OR DELETE ON artifact_topics
    FOR EACH ROW EXECUTE FUNCTION artifact_topics_log_history();

-- Sync: an artifact's sync entry lists its topics, so gaining or losing one
-- (or a topic being approved, withdrawn, or deleted) must move the
-- artifact's sync_version. The no-op write is the same one the location and
-- publication triggers use; the history trigger ignores it.
CREATE FUNCTION artifact_topics_touch_artifact() RETURNS TRIGGER AS $$
BEGIN
    UPDATE artifacts SET lat = lat
    WHERE id = CASE WHEN TG_OP = 'DELETE' THEN OLD.artifact_id ELSE NEW.artifact_id END;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER artifact_topics_touch_artifact
    AFTER INSERT OR UPDATE OR DELETE ON artifact_topics
    FOR EACH ROW EXECUTE FUNCTION artifact_topics_touch_artifact();

CREATE FUNCTION propagate_topic_publication_change() RETURNS TRIGGER AS $$
BEGIN
    UPDATE artifacts SET lat = lat
    WHERE id IN (SELECT artifact_id FROM artifact_topics WHERE topic_id = NEW.id);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER artifacts_propagate_topic_publication
    AFTER UPDATE ON artifacts
    FOR EACH ROW
    WHEN (NEW.is_topic AND (OLD.status IS DISTINCT FROM NEW.status
                            OR OLD.deleted_at IS DISTINCT FROM NEW.deleted_at))
    EXECUTE FUNCTION propagate_topic_publication_change();

-- A topic is not a place, so it can't be a stop on a walk.
CREATE FUNCTION assert_trail_stop_not_topic() RETURNS TRIGGER AS $$
BEGIN
    IF EXISTS (SELECT 1 FROM artifacts WHERE id = NEW.artifact_id AND is_topic) THEN
        RAISE EXCEPTION 'a topic cannot be a trail stop'
            USING ERRCODE = 'check_violation';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trail_stops_assert_not_topic
    BEFORE INSERT OR UPDATE ON trail_stops
    FOR EACH ROW EXECUTE FUNCTION assert_trail_stop_not_topic();
