-- Workflow auto-sync scheduling: org-level default and per-workflow overrides.

ALTER TABLE organizations
    ADD COLUMN IF NOT EXISTS default_workflow_sync_interval_minutes INT;

CREATE TABLE workflow_sync_schedules (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id UUID NOT NULL,
    workflow_name TEXT NOT NULL,   -- matches reusable_workflows.name (applies to all versions)
    enabled BOOL NOT NULL DEFAULT true,
    interval_minutes INT NOT NULL, -- 0 = disabled
    last_synced_at TIMESTAMPTZ,
    next_sync_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (org_id, workflow_name)
);

CREATE INDEX idx_workflow_sync_schedules_due
    ON workflow_sync_schedules (next_sync_at)
    WHERE enabled = true AND interval_minutes > 0;
