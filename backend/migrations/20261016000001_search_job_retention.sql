-- Completed jobs are operational history, not search content. Keep seven days
-- and let each worker poll prune a bounded batch; retain failures for inspection.
CREATE INDEX search_index_jobs_processed_idx
    ON search_index_jobs (updated_at, id) WHERE status = 'processed';
