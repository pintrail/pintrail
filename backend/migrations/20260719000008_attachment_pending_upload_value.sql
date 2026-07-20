-- no-transaction
--
-- Adds the state an attachment occupies between "the author asked for an
-- upload URL" and "the bytes are actually in the bucket".
--
-- Without it, the row inserted at upload-intent time would already be
-- 'queued', and the worker -- which polls for exactly that -- would claim it
-- and fail against an object that does not exist yet. Every upload would race
-- its own processing job.
--
-- Alone in its own migration on purpose: Postgres refuses to use a new enum
-- value in the same transaction that adds it, and a multi-statement migration
-- runs as one implicit transaction even with `-- no-transaction`. The
-- statements that reference the value live in the next migration.

ALTER TYPE processing_status ADD VALUE IF NOT EXISTS 'pending_upload' BEFORE 'queued';
