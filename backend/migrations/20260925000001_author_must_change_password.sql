-- Set when an admin chooses an author's password in the panel. The author
-- must replace it before any other author route will serve them, so a
-- password the admin knows never stays in use.
ALTER TABLE authors
    ADD COLUMN must_change_password BOOLEAN NOT NULL DEFAULT false;
