-- Opt-in until the pgvector database-image rollout is reviewed (#25).
CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE search_embeddings (
    document_id UUID NOT NULL REFERENCES search_documents(id) ON DELETE CASCADE,
    model_fingerprint TEXT NOT NULL CHECK (model_fingerprint ~ '^[0-9a-f]{64}$'),
    model_name TEXT NOT NULL CHECK (model_name <> ''),
    model_repository TEXT NOT NULL CHECK (model_repository <> ''),
    model_revision TEXT NOT NULL CHECK (model_revision <> ''),
    pipeline_version TEXT NOT NULL CHECK (pipeline_version <> ''),
    dimension INTEGER NOT NULL CHECK (dimension BETWEEN 1 AND 16000),
    source_content_hash TEXT NOT NULL CHECK (source_content_hash ~ '^[0-9a-f]{64}$'),
    embedding vector NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (document_id, model_fingerprint),
    CHECK (vector_dims(embedding) = dimension),
    CHECK (vector_norm(embedding) > 0)
);
CREATE INDEX search_embeddings_model_idx ON search_embeddings(model_fingerprint);

-- Changing prepared text immediately invalidates the old mathematical version.
CREATE FUNCTION invalidate_search_embeddings() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    DELETE FROM search_embeddings WHERE document_id = OLD.id;
    RETURN NEW;
END;
$$;
CREATE TRIGGER search_documents_invalidate_embeddings
    BEFORE UPDATE OF content_hash ON search_documents
    FOR EACH ROW WHEN (OLD.content_hash IS DISTINCT FROM NEW.content_hash)
    EXECUTE FUNCTION invalidate_search_embeddings();
