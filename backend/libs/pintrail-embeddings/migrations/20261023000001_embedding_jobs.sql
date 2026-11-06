-- Separate embedding jobs: text preparation stays usable without a model.
CREATE TABLE search_embedding_jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    document_id UUID NOT NULL REFERENCES search_documents(id) ON DELETE CASCADE,
    model_fingerprint TEXT NOT NULL CHECK (model_fingerprint ~ '^[0-9a-f]{64}$'),
    source_content_hash TEXT NOT NULL CHECK (source_content_hash ~ '^[0-9a-f]{64}$'),
    status TEXT NOT NULL DEFAULT 'queued'
        CHECK (status IN ('queued','processing','processed','failed')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    claimed_at TIMESTAMPTZ,
    claim_token UUID,
    error_message TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (document_id, model_fingerprint),
    CHECK ((status = 'processing') = (claimed_at IS NOT NULL AND claim_token IS NOT NULL))
);
CREATE INDEX search_embedding_jobs_ready_idx
    ON search_embedding_jobs(model_fingerprint, available_at, id) WHERE status='queued';
CREATE INDEX search_embedding_jobs_stuck_idx
    ON search_embedding_jobs(claimed_at) WHERE status='processing';
CREATE INDEX search_embedding_jobs_processed_idx
    ON search_embedding_jobs(updated_at, id) WHERE status='processed';
