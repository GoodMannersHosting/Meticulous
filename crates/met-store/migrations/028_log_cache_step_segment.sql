-- Log lines were keyed only by (job_run_id, sequence). Workflow steps each restart sequence
-- from 0, so stdout/stderr from later steps overwrote earlier steps in log_cache.
-- Segment by step_run_id (NULL steps share a sentinel UUID for the key).

ALTER TABLE log_cache
    ADD COLUMN step_key UUID GENERATED ALWAYS AS (
        COALESCE(step_run_id, '00000000-0000-0000-0000-000000000000'::uuid)
    ) STORED;

ALTER TABLE log_cache DROP CONSTRAINT log_cache_pkey;

ALTER TABLE log_cache ADD PRIMARY KEY (job_run_id, step_key, sequence);
