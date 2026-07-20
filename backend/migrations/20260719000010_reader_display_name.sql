-- A public-facing name for readers.
--
-- The comment wireframe (docs/DESIGN.md §1.7) shows an author handle --
-- "jstudent42" -- but §2.2 gives readers only an email address. Rendering
-- comments with what the schema actually has would publish every commenter's
-- email address to every other user, which is a privacy breach, and one that
-- would be discovered only after real comments existed.
--
-- Nullable: readers who never choose a name get a stable pseudonym derived
-- from their id, never from their email.

ALTER TABLE readers ADD COLUMN IF NOT EXISTS display_name VARCHAR(60);

-- Case-insensitive uniqueness so two readers cannot be visually confused for
-- one another in a comment thread. Partial, because NULL means "no name
-- chosen" and any number of readers may be in that state.
CREATE UNIQUE INDEX IF NOT EXISTS readers_display_name_lower_key
    ON readers (lower(display_name))
    WHERE display_name IS NOT NULL;
