-- Comments: written by readers, moderated by admins (docs/DESIGN.md 2.2).

CREATE TABLE comments (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    artifact_id UUID NOT NULL REFERENCES artifacts (id) ON DELETE CASCADE,
    reader_id   UUID NOT NULL REFERENCES readers (id) ON DELETE CASCADE,
    body        TEXT NOT NULL,
    status      comment_status NOT NULL DEFAULT 'visible',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT comments_body_not_blank CHECK (length(btrim(body)) > 0),
    CONSTRAINT comments_body_length CHECK (length(body) <= 4000)
);

-- The reader-facing listing: visible comments on one artifact, newest first.
CREATE INDEX comments_artifact_visible_idx ON comments (artifact_id, created_at DESC)
    WHERE status = 'visible';

-- The admin moderation queue.
CREATE INDEX comments_flagged_idx ON comments (created_at DESC)
    WHERE status = 'flagged';

-- Backs the per-reader rate limit ("how many comments has this reader posted
-- in the last N minutes") without scanning the whole table.
CREATE INDEX comments_reader_created_idx ON comments (reader_id, created_at DESC);

CREATE TRIGGER comments_set_updated_at
    BEFORE UPDATE ON comments
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
