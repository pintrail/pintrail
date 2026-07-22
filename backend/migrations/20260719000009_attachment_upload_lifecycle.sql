-- The rest of the upload lifecycle, separated from the ALTER TYPE in the
-- previous migration because these statements use the new enum value.

ALTER TABLE attachments ALTER COLUMN status SET DEFAULT 'pending_upload';

-- Recorded from the bucket at completion rather than trusted from the client,
-- and used to enforce the size limit after the fact: a presigned PUT cannot
-- reject an oversized body mid-flight.
ALTER TABLE attachments ADD COLUMN IF NOT EXISTS size_bytes BIGINT;

-- Finds abandoned intents -- an author who requested a URL and never uploaded.
-- Those rows hold a storage key and a position forever otherwise.
CREATE INDEX IF NOT EXISTS attachments_pending_upload_idx ON attachments (created_at)
    WHERE status = 'pending_upload';
