-- Profiles for authors: a name and a face, so the Studio can say who did
-- something instead of showing an email address.
--
-- Additive only. Every column has a default, so the previous release, which
-- names its columns explicitly, keeps working.

ALTER TABLE authors
    ADD COLUMN full_name    TEXT NOT NULL DEFAULT '',
    -- What to call them in the Studio ("Tim"); falls back to full_name.
    ADD COLUMN display_name TEXT NOT NULL DEFAULT '',
    ADD COLUMN pronouns     TEXT NOT NULL DEFAULT '',
    -- Their connection to the project: "COMPSCI 326 student",
    -- "Campus Sustainability".
    ADD COLUMN affiliation  TEXT NOT NULL DEFAULT '',
    ADD COLUMN bio          TEXT NOT NULL DEFAULT '',
    -- Storage key of the processed (square WebP) profile photo, or NULL.
    ADD COLUMN avatar_key   TEXT,
    ADD CONSTRAINT authors_profile_lengths CHECK (
        char_length(full_name) <= 120 AND char_length(display_name) <= 60
        AND char_length(pronouns) <= 40 AND char_length(affiliation) <= 120
        AND char_length(bio) <= 600
    );

-- How an author is named to everyone else: display name, else full name,
-- else (for an account with no profile yet) the email address.
CREATE FUNCTION author_label(p_id UUID) RETURNS TEXT AS $$
    SELECT COALESCE(NULLIF(display_name, ''), NULLIF(full_name, ''), email)
    FROM authors WHERE id = p_id
$$ LANGUAGE sql STABLE;
