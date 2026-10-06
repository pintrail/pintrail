-- Removes the example artifacts loaded by seeds/examples.sql.
--
-- Deletes the two example buildings by their fixed ids; their child
-- artifacts go with them (parent_id is ON DELETE CASCADE). Anything an
-- author attached to an example (photos, comments, trail stops) goes too.
--
-- Run on the server from ~/pintrail/backend:
--   docker compose exec -T postgres psql -U pintrail -d pintrail -v ON_ERROR_STOP=1 < seeds/remove-examples.sql

DELETE FROM artifacts
WHERE id IN ('5eed0000-0000-4000-8000-000000000001',
             '5eed0000-0000-4000-8000-000000000010');
