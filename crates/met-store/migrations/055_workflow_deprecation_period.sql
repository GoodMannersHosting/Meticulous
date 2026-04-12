-- Date-gated deprecation: a workflow version becomes "warned" before the date
-- and hard-blocked (fails pipeline runs) on or after it.

ALTER TABLE reusable_workflows
    ADD COLUMN IF NOT EXISTS deprecated_after TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS deprecation_note TEXT;

CREATE INDEX idx_reusable_workflows_deprecated_after
    ON reusable_workflows (org_id, name, deprecated_after)
    WHERE deprecated_after IS NOT NULL AND deleted_at IS NULL;
