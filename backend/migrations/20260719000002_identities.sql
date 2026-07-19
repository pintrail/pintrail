-- Three identity tiers, kept as separate tables rather than one table with a
-- role enum (docs/DESIGN.md 2.2). Authors and readers are different products
-- with different auth flows; admins are authors with role='admin'.

-- Small, trusted, admin-provisioned group.
CREATE TABLE authors (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email         VARCHAR(320) NOT NULL,
    password_hash VARCHAR NOT NULL,   -- scrypt, "salt_hex:dk_hex"
    role          author_role NOT NULL DEFAULT 'viewer',
    is_active     BOOLEAN NOT NULL DEFAULT true,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Case-insensitive uniqueness: "Tim@umass.edu" and "tim@umass.edu" are the
-- same person, and treating them as two accounts is an account-takeover
-- vector at worst and a support ticket at best.
CREATE UNIQUE INDEX authors_email_lower_key ON authors (lower(email));

CREATE TRIGGER authors_set_updated_at
    BEFORE UPDATE ON authors
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TABLE author_sessions (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    author_id  UUID NOT NULL REFERENCES authors (id) ON DELETE CASCADE,
    token_hash VARCHAR NOT NULL,   -- sha256 of the cookie value; raw token never stored
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX author_sessions_token_hash_key ON author_sessions (token_hash);
CREATE INDEX author_sessions_author_id_idx ON author_sessions (author_id);
-- Supports the periodic sweep that deletes expired sessions.
CREATE INDEX author_sessions_expires_at_idx ON author_sessions (expires_at);

-- Large, self-service group.
CREATE TABLE readers (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email          VARCHAR(320) NOT NULL,
    email_verified BOOLEAN NOT NULL DEFAULT false,
    password_hash  VARCHAR NOT NULL,   -- argon2id; no legacy hashes on this tier
    is_active      BOOLEAN NOT NULL DEFAULT true,   -- admin can suspend
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX readers_email_lower_key ON readers (lower(email));

CREATE TRIGGER readers_set_updated_at
    BEFORE UPDATE ON readers
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- Long-lived bearer tokens for the mobile app, not cookies.
CREATE TABLE reader_sessions (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    reader_id  UUID NOT NULL REFERENCES readers (id) ON DELETE CASCADE,
    token_hash VARCHAR NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX reader_sessions_token_hash_key ON reader_sessions (token_hash);
CREATE INDEX reader_sessions_reader_id_idx ON reader_sessions (reader_id);
CREATE INDEX reader_sessions_expires_at_idx ON reader_sessions (expires_at);

-- Single-use, expiring tokens for email verification and password reset.
-- Separate from sessions because their lifecycle and revocation rules differ:
-- a used token must die immediately, a session lives until it expires.
CREATE TYPE reader_token_purpose AS ENUM ('verify_email', 'reset_password');

CREATE TABLE reader_verification_tokens (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    reader_id  UUID NOT NULL REFERENCES readers (id) ON DELETE CASCADE,
    purpose    reader_token_purpose NOT NULL,
    token_hash VARCHAR NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX reader_verification_tokens_token_hash_key
    ON reader_verification_tokens (token_hash);
CREATE INDEX reader_verification_tokens_reader_id_idx
    ON reader_verification_tokens (reader_id, purpose);
