-- Point-in-time agent audit payload when a job run is claimed (forensics / compromise investigation).
ALTER TABLE job_runs
    ADD COLUMN IF NOT EXISTS agent_snapshot JSONB,
    ADD COLUMN IF NOT EXISTS agent_snapshot_captured_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_job_runs_agent_snapshot_agent
    ON job_runs ((agent_snapshot ->> 'id'))
    WHERE agent_snapshot IS NOT NULL;
