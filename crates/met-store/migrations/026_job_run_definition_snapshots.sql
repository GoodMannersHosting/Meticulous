-- Content-addressed JSON for pipeline / reusable workflow definitions (shared across job_runs).
CREATE TABLE definition_snapshots (
    content_sha256 BYTEA NOT NULL PRIMARY KEY CHECK (octet_length(content_sha256) = 32),
    body JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE definition_snapshots IS 'SHA-256–deduplicated definition JSON; referenced from job_runs.';

ALTER TABLE job_runs
    ADD COLUMN IF NOT EXISTS pipeline_definition_sha256 BYTEA REFERENCES definition_snapshots (content_sha256),
    ADD COLUMN IF NOT EXISTS workflow_definition_sha256 BYTEA REFERENCES definition_snapshots (content_sha256),
    ADD COLUMN IF NOT EXISTS source_workflow JSONB;

COMMENT ON COLUMN job_runs.pipeline_definition_sha256 IS 'SHA-256 of canonical pipeline definition JSON at job_run creation.';
COMMENT ON COLUMN job_runs.workflow_definition_sha256 IS 'SHA-256 of reusable workflow definition JSON when this job was expanded from one; null for inline/root jobs.';
COMMENT ON COLUMN job_runs.source_workflow IS 'Resolved reusable workflow ref (scope, name, version); null when not from a reusable workflow.';

CREATE INDEX IF NOT EXISTS idx_job_runs_pipeline_definition_sha256
    ON job_runs (pipeline_definition_sha256)
    WHERE pipeline_definition_sha256 IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_job_runs_workflow_definition_sha256
    ON job_runs (workflow_definition_sha256)
    WHERE workflow_definition_sha256 IS NOT NULL;
