-- Source IP for runs created via HTTP webhook (X-Forwarded-For / X-Real-IP / direct connect).
-- Nullable for API, retry, and historical rows.

ALTER TABLE runs
    ADD COLUMN IF NOT EXISTS webhook_remote_addr TEXT;

COMMENT ON COLUMN runs.webhook_remote_addr IS
    'Observed HTTP client address when the run was created from a webhook (may reflect a trusted proxy chain via X-Forwarded-For).';
