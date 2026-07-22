-- Attachments: media hung off an artifact, and simultaneously the worker's
-- job queue (docs/DESIGN.md 2.5). `status` is the queue column -- there is no
-- Redis in this system.

CREATE TABLE attachments (
    id                    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    artifact_id           UUID NOT NULL REFERENCES artifacts (id) ON DELETE CASCADE,
    kind                  attachment_kind NOT NULL,
    position              INT NOT NULL DEFAULT 0,
    caption               VARCHAR(500),

    original_filename     VARCHAR(512) NOT NULL,
    original_mime_type    VARCHAR(255) NOT NULL,
    original_storage_key  VARCHAR(1024) NOT NULL,

    status                processing_status NOT NULL DEFAULT 'queued',
    processed_storage_key VARCHAR(1024),
    processed_mime_type   VARCHAR(255),

    width                 INT,               -- image/video
    height                INT,               -- image/video
    duration_seconds      DOUBLE PRECISION,  -- audio/video

    error_message         TEXT,

    -- Set when a worker claims the row; the requeue sweep uses it to find
    -- jobs abandoned by a worker that died mid-processing.
    claimed_at            TIMESTAMPTZ,
    attempts              INT NOT NULL DEFAULT 0,

    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT attachments_position_non_negative CHECK (position >= 0),
    -- A failed row should say why; a processed row should have output. Enforced
    -- here because "processed but processed_storage_key is NULL" is a silent
    -- corruption the API would happily serve as a broken media URL.
    CONSTRAINT attachments_failed_has_reason CHECK (
        status <> 'failed' OR error_message IS NOT NULL
    )
);

CREATE UNIQUE INDEX attachments_storage_key_key ON attachments (original_storage_key);
CREATE INDEX attachments_artifact_id_position_idx ON attachments (artifact_id, position);

-- The queue index. Partial, because only queued rows are ever selected by the
-- claim query, and the processed rows will vastly outnumber them over time:
--
--   SELECT ... FROM attachments WHERE status = 'queued'
--   ORDER BY created_at FOR UPDATE SKIP LOCKED LIMIT 10;
CREATE INDEX attachments_queue_idx ON attachments (created_at)
    WHERE status = 'queued';

-- Supports the sweep that re-queues rows stuck in 'processing' past a timeout.
CREATE INDEX attachments_stuck_idx ON attachments (claimed_at)
    WHERE status = 'processing';

CREATE TRIGGER attachments_set_updated_at
    BEFORE UPDATE ON attachments
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
