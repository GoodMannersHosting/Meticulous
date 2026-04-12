-- Add soft-delete support to pipelines, consistent with projects/organizations.
ALTER TABLE pipelines
    ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_pipelines_active
    ON pipelines (project_id, deleted_at)
    WHERE deleted_at IS NULL;
