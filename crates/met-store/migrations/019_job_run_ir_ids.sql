-- Pipeline execution uses JobId / StepId from parsed IR (YAML, workflows). Rows in the
-- legacy `jobs` and `steps` catalog tables are not created for these runs. Keeping FKs
-- caused: insert on job_runs violates foreign key constraint "job_runs_job_id_fkey".
ALTER TABLE job_runs DROP CONSTRAINT IF EXISTS job_runs_job_id_fkey;
ALTER TABLE step_runs DROP CONSTRAINT IF EXISTS step_runs_step_id_fkey;
