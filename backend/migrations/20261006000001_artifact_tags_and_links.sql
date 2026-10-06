-- Tags and links on artifacts (issues #37 and #14).
--
-- Both are child tables rather than columns on artifacts: tags are queried
-- across artifacts ("everything tagged solar"), and links carry their own
-- order, note, and fetched preview. Purely additive, so a rollback to the
-- previous image keeps working against this schema.

-- Free-text tags. Stored as the author typed them (trimmed); uniqueness per
-- artifact is case-insensitive so "Solar" and "solar" cannot both be added.
CREATE TABLE artifact_tags (
    artifact_id UUID NOT NULL REFERENCES artifacts (id) ON DELETE CASCADE,
    tag         VARCHAR(60) NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT artifact_tags_not_blank CHECK (length(btrim(tag)) > 0)
);

CREATE UNIQUE INDEX artifact_tags_artifact_tag_key
    ON artifact_tags (artifact_id, lower(tag));
CREATE INDEX artifact_tags_tag_idx ON artifact_tags (lower(tag));

-- External links: a source, a project page, a news story. The preview fields
-- are fetched from the page's Open Graph / <title> metadata when the link is
-- added or its URL changes; `note` is the author's own words about it.
CREATE TABLE artifact_links (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    artifact_id         UUID NOT NULL REFERENCES artifacts (id) ON DELETE CASCADE,
    url                 TEXT NOT NULL,
    note                TEXT NOT NULL DEFAULT '',
    position            INT NOT NULL DEFAULT 0,

    preview_title       TEXT,
    preview_description TEXT,
    preview_image_url   TEXT,
    preview_site_name   TEXT,
    preview_fetched_at  TIMESTAMPTZ,

    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT artifact_links_http CHECK (url ~* '^https?://')
);

CREATE INDEX artifact_links_artifact_idx ON artifact_links (artifact_id, position);

CREATE TRIGGER artifact_links_set_updated_at
    BEFORE UPDATE ON artifact_links
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
