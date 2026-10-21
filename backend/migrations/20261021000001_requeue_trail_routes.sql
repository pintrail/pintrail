-- One-time repair (#91). When routing was first turned on in production,
-- the worker took the queued route jobs while Valhalla was still building
-- its data, used up their three attempts in seconds, and marked them failed,
-- and nothing retries a failed job. The worker now waits for the router to
-- report ready before taking route jobs; this puts the casualties back.
UPDATE trail_route_jobs
SET status = 'queued', attempts = 0, error_message = NULL, available_at = now()
WHERE status = 'failed';

-- And makes sure every trail without a stored route has a job waiting.
SELECT enqueue_trail_route(t.id)
FROM trails t
WHERE NOT EXISTS (SELECT 1 FROM trail_routes r WHERE r.trail_id = t.id);
