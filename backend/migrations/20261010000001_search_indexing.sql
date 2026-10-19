-- Search content is derived from artifacts; artifacts remain the source of truth.
-- The worker initially creates one text document per artifact. Attachment and
-- chunk references let later PDF/image indexing use the same source records.
ALTER TABLE attachments ADD CONSTRAINT attachments_id_artifact_unique
    UNIQUE (id, artifact_id);

CREATE TABLE search_documents (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    artifact_id   UUID NOT NULL REFERENCES artifacts (id) ON DELETE CASCADE,
    attachment_id UUID,
    source_kind   TEXT NOT NULL DEFAULT 'artifact',
    chunk_index   INT NOT NULL DEFAULT 0 CHECK (chunk_index >= 0),
    page_number   INT CHECK (page_number > 0),
    content       TEXT NOT NULL CHECK (length(btrim(content)) > 0),
    content_hash  TEXT NOT NULL CHECK (content_hash ~ '^[0-9a-f]{64}$'),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- A document cannot claim an attachment belonging to another artifact.
    FOREIGN KEY (attachment_id, artifact_id)
        REFERENCES attachments (id, artifact_id) ON DELETE CASCADE,
    CONSTRAINT search_documents_source_kind CHECK (
        source_kind IN ('artifact', 'pdf', 'image_caption', 'text')
    ),
    CONSTRAINT search_documents_source_shape CHECK (
        (source_kind = 'artifact' AND attachment_id IS NULL
            AND chunk_index = 0 AND page_number IS NULL)
        OR (source_kind <> 'artifact' AND attachment_id IS NOT NULL)
    ),
    -- NULLS NOT DISTINCT also prevents duplicate artifact-only documents,
    -- whose attachment_id is NULL. Supported by our PostgreSQL 17 baseline.
    UNIQUE NULLS NOT DISTINCT (artifact_id, attachment_id, source_kind, chunk_index)
);

CREATE TRIGGER search_documents_set_updated_at
    BEFORE UPDATE ON search_documents
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TABLE search_index_jobs (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    artifact_id   UUID NOT NULL REFERENCES artifacts (id) ON DELETE CASCADE,
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

CREATE INDEX search_index_jobs_queue_idx
    ON search_index_jobs (available_at, created_at, id) WHERE status = 'queued';
CREATE INDEX search_index_jobs_stuck_idx
    ON search_index_jobs (claimed_at) WHERE status = 'processing';
CREATE INDEX search_index_jobs_artifact_idx ON search_index_jobs (artifact_id);

CREATE TRIGGER search_index_jobs_set_updated_at
    BEFORE UPDATE ON search_index_jobs
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- Append jobs rather than overwriting in-flight claims. The worker reads the
-- latest artifact and skips document writes when its content hash is unchanged.
CREATE FUNCTION enqueue_artifact_search() RETURNS TRIGGER AS $$
DECLARE
    include_descendants BOOLEAN := false;
BEGIN
    IF TG_OP = 'UPDATE' THEN
        -- Children include ancestor names in their searchable text.
        include_descendants := OLD.name IS DISTINCT FROM NEW.name
            OR OLD.parent_id IS DISTINCT FROM NEW.parent_id
            OR OLD.deleted_at IS DISTINCT FROM NEW.deleted_at;
    END IF;

    WITH RECURSIVE affected AS (
        SELECT NEW.id AS id
        UNION ALL
        SELECT a.id FROM artifacts a JOIN affected p ON a.parent_id = p.id
        WHERE include_descendants
    )
    INSERT INTO search_index_jobs (artifact_id)
    SELECT id FROM affected;

    -- Do not leave soft-deleted content visible while waiting for the worker.
    IF NEW.deleted_at IS NOT NULL THEN
        DELETE FROM search_documents WHERE artifact_id = NEW.id;
    END IF;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER artifacts_enqueue_search_insert
    AFTER INSERT ON artifacts
    FOR EACH ROW EXECUTE FUNCTION enqueue_artifact_search();

CREATE TRIGGER artifacts_enqueue_search_update
    AFTER UPDATE ON artifacts
    FOR EACH ROW
    WHEN (OLD.name IS DISTINCT FROM NEW.name
        OR OLD.kind IS DISTINCT FROM NEW.kind
        OR OLD.description IS DISTINCT FROM NEW.description
        OR OLD.parent_id IS DISTINCT FROM NEW.parent_id
        OR OLD.deleted_at IS DISTINCT FROM NEW.deleted_at)
    EXECUTE FUNCTION enqueue_artifact_search();

-- Initial backfill: existing artifacts get jobs when this migration runs.
INSERT INTO search_index_jobs (artifact_id)
SELECT id FROM artifacts WHERE deleted_at IS NULL;
