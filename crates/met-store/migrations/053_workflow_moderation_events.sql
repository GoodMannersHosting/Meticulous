-- Workflow moderation events: records every approve/reject/trust/untrust/delete action
-- with an optional markdown note. Required for SecurityEngineer actors.

CREATE TABLE IF NOT EXISTS workflow_moderation_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workflow_id UUID NOT NULL REFERENCES reusable_workflows(id) ON DELETE CASCADE,
    org_id UUID NOT NULL,
    action TEXT NOT NULL,          -- approve | reject | trust | untrust | delete
    actor_user_id UUID NOT NULL,
    note TEXT,                     -- required for security_engineer actors, optional for superadmin
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_workflow_moderation_events_workflow_id
    ON workflow_moderation_events (workflow_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_workflow_moderation_events_org_id
    ON workflow_moderation_events (org_id, created_at DESC);
