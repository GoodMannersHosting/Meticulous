-- Per-run merged workflow invocation outputs (public map + secret envelope metadata).
-- Idempotent merge: PostgreSQL jsonb || on upsert (last key wins for overlapping keys).
CREATE TABLE pipeline_run_workflow_outputs (
    run_id UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    workflow_invocation_id TEXT NOT NULL,
    producer_job_run_id UUID NOT NULL REFERENCES job_runs(id) ON DELETE CASCADE,
    public_outputs JSONB NOT NULL DEFAULT '{}'::jsonb,
    secret_envelopes JSONB NOT NULL DEFAULT '{}'::jsonb,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (run_id, workflow_invocation_id)
);

CREATE INDEX idx_pipeline_run_workflow_outputs_run
    ON pipeline_run_workflow_outputs(run_id);

COMMENT ON TABLE pipeline_run_workflow_outputs IS
    'Merged met-output public map + sealed secret envelopes per pipeline workflow invocation id; see design/workflow-invocation-outputs.md';
