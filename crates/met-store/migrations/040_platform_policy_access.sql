-- Org policy (token TTL caps, rate-limit tunables), API token lifecycle, service accounts,
-- project ACL, pipeline archive, app enable flag.

CREATE TABLE IF NOT EXISTS org_policy (
    org_id UUID PRIMARY KEY REFERENCES organizations(id) ON DELETE CASCADE,
    max_api_token_ttl_days INT NOT NULL DEFAULT 365,
    user_rl_primary_period_secs INT NOT NULL DEFAULT 3600,
    user_rl_primary_max INT NOT NULL DEFAULT 15000,
    user_rl_secondary_period_secs INT NOT NULL DEFAULT 10,
    user_rl_secondary_max INT NOT NULL DEFAULT 60,
    app_rl_primary_period_secs INT NOT NULL DEFAULT 3600,
    app_rl_primary_max INT NOT NULL DEFAULT 15000,
    app_rl_secondary_period_secs INT NOT NULL DEFAULT 10,
    app_rl_secondary_max INT NOT NULL DEFAULT 60,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO org_policy (org_id)
SELECT id FROM organizations
ON CONFLICT (org_id) DO NOTHING;

ALTER TABLE api_tokens ADD COLUMN IF NOT EXISTS deactivated_at TIMESTAMPTZ;

ALTER TABLE users ADD COLUMN IF NOT EXISTS service_account BOOLEAN NOT NULL DEFAULT false;

ALTER TABLE meticulous_apps ADD COLUMN IF NOT EXISTS enabled BOOLEAN NOT NULL DEFAULT true;

ALTER TABLE pipelines ADD COLUMN IF NOT EXISTS archived_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_pipelines_archived
    ON pipelines(project_id)
    WHERE archived_at IS NOT NULL;

CREATE TYPE project_principal_type AS ENUM ('user', 'group');
CREATE TYPE project_role AS ENUM ('admin', 'developer', 'readonly');

CREATE TABLE IF NOT EXISTS project_members (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    principal_type project_principal_type NOT NULL,
    principal_id UUID NOT NULL,
    role project_role NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (project_id, principal_type, principal_id)
);

CREATE INDEX IF NOT EXISTS idx_project_members_project ON project_members(project_id);
CREATE INDEX IF NOT EXISTS idx_project_members_principal ON project_members(principal_type, principal_id);

ALTER TYPE permission_role ADD VALUE 'security_auditor';
