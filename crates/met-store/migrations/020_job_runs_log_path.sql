-- Align `job_runs` with `met_core::models::JobRun` (sqlx FromRow expects `log_path`).
ALTER TABLE job_runs ADD COLUMN IF NOT EXISTS log_path TEXT;
