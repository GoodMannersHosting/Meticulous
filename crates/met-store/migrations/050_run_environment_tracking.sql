-- Track environment and workflow invocation on runs/job_runs for the matrix view.
ALTER TABLE runs ADD COLUMN IF NOT EXISTS environment_id UUID REFERENCES environments(id);
ALTER TABLE job_runs ADD COLUMN IF NOT EXISTS workflow_invocation_id TEXT;

CREATE INDEX IF NOT EXISTS idx_runs_environment ON runs(environment_id) WHERE environment_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_job_runs_invocation ON job_runs(workflow_invocation_id) WHERE workflow_invocation_id IS NOT NULL;
