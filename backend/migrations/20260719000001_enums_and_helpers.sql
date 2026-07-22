-- Enums and shared helpers for the Pintrail schema (docs/DESIGN.md 2.2).
--
-- Postgres 17 provides gen_random_uuid() in core, so no pgcrypto extension.

CREATE TYPE author_role AS ENUM ('viewer', 'editor', 'admin');

CREATE TYPE artifact_kind AS ENUM (
    'building',
    'room',
    'artwork',
    'installation',
    'rooftop',
    'other'
);

CREATE TYPE attachment_kind AS ENUM ('image', 'audio', 'video', 'pdf', 'text');

CREATE TYPE processing_status AS ENUM ('queued', 'processing', 'processed', 'failed');

CREATE TYPE owner_type AS ENUM ('author', 'reader');

CREATE TYPE trail_visibility AS ENUM ('private', 'unlisted', 'public');

CREATE TYPE comment_status AS ENUM ('visible', 'hidden', 'flagged');

-- Keeps updated_at honest without every INSERT/UPDATE site remembering to set
-- it. Attached per-table below.
CREATE FUNCTION set_updated_at() RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
