-- Link retry runs to the pipeline run they were restarted from (UI: "Run Pipeline" stays root; Retry sets parent).

ALTER TABLE runs
    ADD COLUMN IF NOT EXISTS parent_run_id UUID REFERENCES runs(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_runs_parent_run_id
    ON runs (parent_run_id)
    WHERE parent_run_id IS NOT NULL;

COMMENT ON COLUMN runs.parent_run_id IS 'Non-null when this run was created via API Retry from another run; null for new runs from Run Pipeline, webhooks, etc.';
