-- Authorship, a review status, and a full change history for artifacts.
--
-- Additive only: new columns with defaults, new tables, new triggers. The
-- previous release ignores all of it, so a code rollback keeps working.
--
-- WHO MADE A CHANGE. A trigger cannot see the HTTP request, so the
-- application tells it: every write runs inside a transaction that first
-- calls set_config('pintrail.actor_id', <author id>, true) (see audit.rs).
-- `true` scopes the setting to that transaction, so it cannot leak into the
-- next request on the same pooled connection. A write made without it (a
-- seed script, psql, the media worker) is recorded with no author, which
-- the Studio shows as "system".

CREATE TYPE artifact_status AS ENUM ('draft', 'ready', 'approved');

ALTER TABLE artifacts
    ADD COLUMN created_by   UUID REFERENCES authors (id) ON DELETE SET NULL,
    ADD COLUMN updated_by   UUID REFERENCES authors (id) ON DELETE SET NULL,
    ADD COLUMN status       artifact_status NOT NULL DEFAULT 'draft',
    ADD COLUMN submitted_at TIMESTAMPTZ,
    ADD COLUMN reviewed_by  UUID REFERENCES authors (id) ON DELETE SET NULL,
    ADD COLUMN reviewed_at  TIMESTAMPTZ,
    -- The reviewer's note when sending an artifact back to its author.
    ADD COLUMN review_note  TEXT NOT NULL DEFAULT '';

CREATE INDEX artifacts_created_by_idx ON artifacts (created_by);
CREATE INDEX artifacts_status_idx ON artifacts (status) WHERE deleted_at IS NULL;

-- The worked examples (seeds/examples.sql, fixed ids) are reference
-- material, so they start approved. Everything else already in the table
-- starts as a draft with no recorded author: it predates authorship, and an
-- admin can approve it or assign it an owner.
UPDATE artifacts SET status = 'approved', reviewed_at = now()
WHERE id::text LIKE '5eed0000-0000-4000-8000-%';

-- The current actor, or NULL when the write didn't say who it was.
CREATE FUNCTION pintrail_actor() RETURNS UUID AS $$
    SELECT NULLIF(current_setting('pintrail.actor_id', true), '')::uuid
$$ LANGUAGE sql STABLE;

-- Fills created_by / updated_by from the actor. updated_by moves only when
-- the artifact's content changes, so a review decision or the location
-- propagation trigger's no-op write doesn't count as someone editing it.
CREATE FUNCTION artifacts_stamp_author() RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        NEW.created_by := COALESCE(NEW.created_by, pintrail_actor());
        NEW.updated_by := COALESCE(NEW.updated_by, NEW.created_by);
    ELSIF pintrail_actor() IS NOT NULL
      AND (NEW.kind, NEW.name, NEW.description, NEW.lat, NEW.lng, NEW.parent_id, NEW.beacon_id)
          IS DISTINCT FROM
          (OLD.kind, OLD.name, OLD.description, OLD.lat, OLD.lng, OLD.parent_id, OLD.beacon_id)
    THEN
        NEW.updated_by := pintrail_actor();
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER artifacts_stamp_author
    BEFORE INSERT OR UPDATE ON artifacts
    FOR EACH ROW EXECUTE FUNCTION artifacts_stamp_author();

-- The history itself: one row per change, never updated, never deleted.
-- No foreign key to artifacts on purpose, so the record outlives even a
-- hard delete (seeds/remove-examples.sql removes rows outright).
CREATE TABLE artifact_history (
    id          BIGSERIAL PRIMARY KEY,
    artifact_id UUID NOT NULL,
    actor_id    UUID,
    -- Who the actor was at the time, so the history still reads correctly if
    -- the account is later renamed or removed.
    actor_email TEXT,
    action      TEXT NOT NULL,
    -- {"field": {"from": old, "to": new}} for edits; the full field set for
    -- a creation; the item's details for link, tag, and media events.
    changes     JSONB NOT NULL DEFAULT '{}',
    at          TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);

CREATE INDEX artifact_history_artifact_idx ON artifact_history (artifact_id, at DESC, id DESC);

CREATE FUNCTION artifact_history_add(p_artifact UUID, p_action TEXT, p_changes JSONB)
RETURNS VOID AS $$
    INSERT INTO artifact_history (artifact_id, actor_id, actor_email, action, changes)
    VALUES (p_artifact, pintrail_actor(),
            (SELECT email FROM authors WHERE id = pintrail_actor()),
            p_action, p_changes)
$$ LANGUAGE sql;

-- Artifact rows: creation, field edits, review decisions, delete, restore.
CREATE FUNCTION artifacts_log_history() RETURNS TRIGGER AS $$
DECLARE
    diff   JSONB := '{}';
    action TEXT;
    o      JSONB;
    n      JSONB;
    f      TEXT;
BEGIN
    IF TG_OP = 'INSERT' THEN
        PERFORM artifact_history_add(NEW.id, 'created', jsonb_strip_nulls(jsonb_build_object(
            'kind', NEW.kind, 'name', NEW.name, 'description', NULLIF(NEW.description, ''),
            'lat', NEW.lat, 'lng', NEW.lng, 'parent_id', NEW.parent_id, 'status', NEW.status)));
        RETURN NULL;
    END IF;

    o := to_jsonb(OLD);
    n := to_jsonb(NEW);
    FOREACH f IN ARRAY ARRAY['kind', 'name', 'description', 'lat', 'lng', 'parent_id',
                             'beacon_id', 'status', 'review_note', 'created_by']
    LOOP
        IF o -> f IS DISTINCT FROM n -> f THEN
            diff := diff || jsonb_build_object(f, jsonb_build_object('from', o -> f, 'to', n -> f));
        END IF;
    END LOOP;

    IF OLD.deleted_at IS NULL AND NEW.deleted_at IS NOT NULL THEN
        action := 'deleted';
    ELSIF OLD.deleted_at IS NOT NULL AND NEW.deleted_at IS NULL THEN
        action := 'restored';
    ELSIF diff = '{}' THEN
        RETURN NULL;  -- bookkeeping only (sync version, a touch, propagation)
    ELSIF diff ? 'status' THEN
        action := 'status';
    ELSIF diff ? 'created_by' AND (diff - 'created_by') = '{}' THEN
        action := 'owner';
    ELSE
        action := 'edited';
    END IF;

    PERFORM artifact_history_add(NEW.id, action, diff);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER artifacts_log_history
    AFTER INSERT OR UPDATE ON artifacts
    FOR EACH ROW EXECUTE FUNCTION artifacts_log_history();

-- Tags.
CREATE FUNCTION artifact_tags_log_history() RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        PERFORM artifact_history_add(NEW.artifact_id, 'tag_added', jsonb_build_object('tag', NEW.tag));
    ELSE
        PERFORM artifact_history_add(OLD.artifact_id, 'tag_removed', jsonb_build_object('tag', OLD.tag));
    END IF;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER artifact_tags_log_history
    AFTER INSERT OR DELETE ON artifact_tags
    FOR EACH ROW EXECUTE FUNCTION artifact_tags_log_history();

-- Links. A reorder is logged once by the application rather than once per
-- moved row, and preview refreshes aren't changes anyone made.
CREATE FUNCTION artifact_links_log_history() RETURNS TRIGGER AS $$
DECLARE
    diff JSONB := '{}';
BEGIN
    IF TG_OP = 'INSERT' THEN
        PERFORM artifact_history_add(NEW.artifact_id, 'link_added',
            jsonb_build_object('url', NEW.url, 'note', NULLIF(NEW.note, '')));
    ELSIF TG_OP = 'DELETE' THEN
        PERFORM artifact_history_add(OLD.artifact_id, 'link_removed',
            jsonb_build_object('url', OLD.url, 'note', NULLIF(OLD.note, '')));
    ELSE
        IF OLD.url IS DISTINCT FROM NEW.url THEN
            diff := diff || jsonb_build_object('url', jsonb_build_object('from', OLD.url, 'to', NEW.url));
        END IF;
        IF OLD.note IS DISTINCT FROM NEW.note THEN
            diff := diff || jsonb_build_object('note', jsonb_build_object('from', OLD.note, 'to', NEW.note));
        END IF;
        IF diff <> '{}' THEN
            PERFORM artifact_history_add(NEW.artifact_id, 'link_edited',
                diff || jsonb_build_object('link', NEW.url));
        END IF;
    END IF;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER artifact_links_log_history
    AFTER INSERT OR UPDATE OR DELETE ON artifact_links
    FOR EACH ROW EXECUTE FUNCTION artifact_links_log_history();

-- Media. Logged when an upload is confirmed (pending_upload -> queued), when
-- a caption changes, and on removal. The worker's processing steps are not
-- changes to the artifact and are left out.
CREATE FUNCTION attachments_log_history() RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        IF OLD.status <> 'pending_upload' THEN
            PERFORM artifact_history_add(OLD.artifact_id, 'media_removed',
                jsonb_build_object('file', OLD.original_filename));
        END IF;
    ELSIF OLD.status = 'pending_upload' AND NEW.status = 'queued' THEN
        PERFORM artifact_history_add(NEW.artifact_id, 'media_added',
            jsonb_build_object('file', NEW.original_filename));
    ELSIF OLD.caption IS DISTINCT FROM NEW.caption THEN
        PERFORM artifact_history_add(NEW.artifact_id, 'media_edited',
            jsonb_build_object('file', NEW.original_filename,
                               'caption', jsonb_build_object('from', OLD.caption, 'to', NEW.caption)));
    END IF;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER attachments_log_history
    AFTER UPDATE OR DELETE ON attachments
    FOR EACH ROW EXECUTE FUNCTION attachments_log_history();

-- Every artifact already here gets a starting entry, so its history doesn't
-- begin blank. Earlier changes were never recorded and can't be recovered.
INSERT INTO artifact_history (artifact_id, action, changes, at)
SELECT id, 'imported',
       jsonb_build_object('note', 'History starts here. Earlier changes were not recorded.'),
       now()
FROM artifacts;
