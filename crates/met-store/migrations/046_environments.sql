-- Pipeline environments: named deployment targets (ADR-016, Phase 2.1).

CREATE TABLE IF NOT EXISTS environments (
    id                     UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id                 UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    project_id             UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name                   TEXT NOT NULL CHECK (name ~ '^[a-z0-9][a-z0-9-]{0,62}$'),
    display_name           TEXT NOT NULL,
    description            TEXT,
    require_approval       BOOLEAN NOT NULL DEFAULT false,
    required_approvers     INT NOT NULL DEFAULT 1,
    approval_timeout_hours INT NOT NULL DEFAULT 72,
    allowed_branches       TEXT[],
    auto_deploy_branch     TEXT,
    variables              JSONB NOT NULL DEFAULT '{}',
    tier                   TEXT NOT NULL DEFAULT 'development'
                           CHECK (tier IN ('development','staging','production','custom')),
    created_at             TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at             TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (project_id, name)
);

CREATE INDEX IF NOT EXISTS idx_environments_project ON environments(project_id);

CREATE TABLE IF NOT EXISTS environment_approvals (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id          UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    environment_id  UUID NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    approved_by     UUID REFERENCES users(id),
    decision        TEXT NOT NULL CHECK (decision IN ('approved', 'rejected')),
    comment         TEXT,
    decided_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (run_id, environment_id, approved_by)
);
