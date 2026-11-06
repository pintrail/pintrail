-- PostgreSQL-native lexical indexing; no model or vector extension required.
-- A generated column follows prepared text automatically, including backfills.
ALTER TABLE search_documents ADD COLUMN search_terms TSVECTOR
    GENERATED ALWAYS AS (to_tsvector('english'::regconfig, content)) STORED;
CREATE INDEX search_documents_terms_idx ON search_documents USING GIN (search_terms);
