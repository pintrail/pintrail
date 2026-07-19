-- Trails: ordered sequences of artifacts, owned by either an author (curated)
-- or a reader (user-built) -- docs/DESIGN.md 2.2.

CREATE TABLE trails (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title       VARCHAR(200) NOT NULL,
    description TEXT NOT NULL DEFAULT '',

    owner_type  owner_type NOT NULL,
    owner_id    UUID NOT NULL,   -- authors.id or readers.id per owner_type

    visibility  trail_visibility NOT NULL DEFAULT 'private',
    share_token VARCHAR(64),     -- opaque; only meaningful when unlisted

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT trails_title_not_blank CHECK (length(btrim(title)) > 0),
    -- An unlisted trail is reachable only by its link, so it must have one.
    CONSTRAINT trails_unlisted_has_token CHECK (
        visibility <> 'unlisted' OR share_token IS NOT NULL
    )
);

CREATE UNIQUE INDEX trails_share_token_key ON trails (share_token)
    WHERE share_token IS NOT NULL;

-- Covers "show me my trails" for either kind of owner.
CREATE INDEX trails_owner_idx ON trails (owner_type, owner_id);
-- Covers the public trail listing.
CREATE INDEX trails_public_idx ON trails (created_at DESC)
    WHERE visibility = 'public';

CREATE TRIGGER trails_set_updated_at
    BEFORE UPDATE ON trails
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- Polymorphic ownership buys a single trails table, but costs the declarative
-- foreign key. These two triggers buy the integrity back: one rejects a trail
-- pointing at a nonexistent owner, the other cleans up when an owner is
-- deleted (what ON DELETE CASCADE would have done for us).
CREATE FUNCTION assert_trail_owner_exists() RETURNS TRIGGER AS $$
DECLARE
    owner_exists BOOLEAN;
BEGIN
    IF NEW.owner_type = 'author' THEN
        SELECT EXISTS (SELECT 1 FROM authors WHERE id = NEW.owner_id) INTO owner_exists;
    ELSE
        SELECT EXISTS (SELECT 1 FROM readers WHERE id = NEW.owner_id) INTO owner_exists;
    END IF;

    IF NOT owner_exists THEN
        RAISE EXCEPTION 'trail owner %/% does not exist', NEW.owner_type, NEW.owner_id
            USING ERRCODE = 'foreign_key_violation';
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trails_assert_owner_exists
    BEFORE INSERT OR UPDATE OF owner_type, owner_id ON trails
    FOR EACH ROW EXECUTE FUNCTION assert_trail_owner_exists();

CREATE FUNCTION delete_trails_for_author() RETURNS TRIGGER AS $$
BEGIN
    DELETE FROM trails WHERE owner_type = 'author' AND owner_id = OLD.id;
    RETURN OLD;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER authors_delete_trails
    BEFORE DELETE ON authors
    FOR EACH ROW EXECUTE FUNCTION delete_trails_for_author();

CREATE FUNCTION delete_trails_for_reader() RETURNS TRIGGER AS $$
BEGIN
    DELETE FROM trails WHERE owner_type = 'reader' AND owner_id = OLD.id;
    RETURN OLD;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER readers_delete_trails
    BEFORE DELETE ON readers
    FOR EACH ROW EXECUTE FUNCTION delete_trails_for_reader();

CREATE TABLE trail_stops (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    trail_id    UUID NOT NULL REFERENCES trails (id) ON DELETE CASCADE,
    artifact_id UUID NOT NULL REFERENCES artifacts (id) ON DELETE CASCADE,
    position    INT NOT NULL,
    note        TEXT,   -- the trail creator's own gloss on this stop

    CONSTRAINT trail_stops_position_non_negative CHECK (position >= 0)
);

CREATE INDEX trail_stops_trail_id_idx ON trail_stops (trail_id, position);
CREATE INDEX trail_stops_artifact_id_idx ON trail_stops (artifact_id);

-- DEFERRABLE so a reorder can renumber stops inside one transaction without
-- tripping over transient duplicates; a non-deferred constraint would force
-- the API to shuffle rows through a temporary position range instead.
ALTER TABLE trail_stops
    ADD CONSTRAINT trail_stops_trail_position_key
    UNIQUE (trail_id, position) DEFERRABLE INITIALLY IMMEDIATE;
