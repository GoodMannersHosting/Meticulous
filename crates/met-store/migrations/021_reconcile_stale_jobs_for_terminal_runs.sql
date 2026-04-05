-- Align job_runs and step_runs with terminal runs where rows were left pending/queued/running
-- (engine/controller desync, crashes, missed updates).

-- ---------------------------------------------------------------------------
-- step_runs (must run before job_runs so nested execution state stays coherent)
-- ---------------------------------------------------------------------------

UPDATE step_runs sr
SET status = 'cancelled',
    finished_at = COALESCE(sr.finished_at, r.finished_at, NOW()),
    error_message = COALESCE(sr.error_message, 'Run cancelled')
FROM job_runs jr
JOIN runs r ON r.id = jr.run_id
WHERE sr.job_run_id = jr.id
  AND r.status = 'cancelled'
  AND sr.status IN ('pending', 'queued', 'running');

UPDATE step_runs sr
SET status = 'timed_out',
    finished_at = COALESCE(sr.finished_at, r.finished_at, NOW()),
    error_message = COALESCE(sr.error_message, 'Run timed out')
FROM job_runs jr
JOIN runs r ON r.id = jr.run_id
WHERE sr.job_run_id = jr.id
  AND r.status = 'timed_out'
  AND sr.status IN ('pending', 'queued', 'running');

UPDATE step_runs sr
SET status = 'failed',
    exit_code = COALESCE(sr.exit_code, 1),
    finished_at = COALESCE(sr.finished_at, r.finished_at, NOW()),
    error_message = COALESCE(sr.error_message, 'Run ended while step was running')
FROM job_runs jr
JOIN runs r ON r.id = jr.run_id
WHERE sr.job_run_id = jr.id
  AND r.status = 'failed'
  AND sr.status = 'running';

UPDATE step_runs sr
SET status = 'cancelled',
    finished_at = COALESCE(sr.finished_at, r.finished_at, NOW()),
    error_message = COALESCE(sr.error_message, 'Run failed before this step executed')
FROM job_runs jr
JOIN runs r ON r.id = jr.run_id
WHERE sr.job_run_id = jr.id
  AND r.status = 'failed'
  AND sr.status IN ('pending', 'queued');

UPDATE step_runs sr
SET status = 'failed',
    exit_code = COALESCE(sr.exit_code, 1),
    finished_at = COALESCE(sr.finished_at, r.finished_at, NOW()),
    error_message = COALESCE(sr.error_message, 'Run was marked succeeded while step was still running')
FROM job_runs jr
JOIN runs r ON r.id = jr.run_id
WHERE sr.job_run_id = jr.id
  AND r.status = 'succeeded'
  AND sr.status = 'running';

UPDATE step_runs sr
SET status = 'skipped',
    finished_at = COALESCE(sr.finished_at, r.finished_at, NOW()),
    error_message = COALESCE(sr.error_message, 'Run completed; step was not executed')
FROM job_runs jr
JOIN runs r ON r.id = jr.run_id
WHERE sr.job_run_id = jr.id
  AND r.status = 'succeeded'
  AND sr.status IN ('pending', 'queued');

-- ---------------------------------------------------------------------------
-- job_runs
-- ---------------------------------------------------------------------------

UPDATE job_runs jr
SET status = 'cancelled',
    finished_at = COALESCE(jr.finished_at, r.finished_at, NOW()),
    error_message = COALESCE(jr.error_message, 'Run cancelled')
FROM runs r
WHERE jr.run_id = r.id
  AND r.status = 'cancelled'
  AND jr.status IN ('pending', 'queued', 'running');

UPDATE job_runs jr
SET status = 'timed_out',
    finished_at = COALESCE(jr.finished_at, r.finished_at, NOW()),
    error_message = COALESCE(jr.error_message, 'Run timed out')
FROM runs r
WHERE jr.run_id = r.id
  AND r.status = 'timed_out'
  AND jr.status IN ('pending', 'queued', 'running');

UPDATE job_runs jr
SET status = 'failed',
    exit_code = COALESCE(jr.exit_code, 1),
    finished_at = COALESCE(jr.finished_at, r.finished_at, NOW()),
    error_message = COALESCE(jr.error_message, 'Run ended while job was running')
FROM runs r
WHERE jr.run_id = r.id
  AND r.status = 'failed'
  AND jr.status = 'running';

UPDATE job_runs jr
SET status = 'cancelled',
    finished_at = COALESCE(jr.finished_at, r.finished_at, NOW()),
    error_message = COALESCE(jr.error_message, 'Run failed before this job executed')
FROM runs r
WHERE jr.run_id = r.id
  AND r.status = 'failed'
  AND jr.status IN ('pending', 'queued');

UPDATE job_runs jr
SET status = 'failed',
    exit_code = COALESCE(jr.exit_code, 1),
    finished_at = COALESCE(jr.finished_at, r.finished_at, NOW()),
    error_message = COALESCE(jr.error_message, 'Run was marked succeeded while job was still running')
FROM runs r
WHERE jr.run_id = r.id
  AND r.status = 'succeeded'
  AND jr.status = 'running';

UPDATE job_runs jr
SET status = 'skipped',
    finished_at = COALESCE(jr.finished_at, r.finished_at, NOW()),
    error_message = COALESCE(jr.error_message, 'Run completed; job was not executed')
FROM runs r
WHERE jr.run_id = r.id
  AND r.status = 'succeeded'
  AND jr.status IN ('pending', 'queued');
