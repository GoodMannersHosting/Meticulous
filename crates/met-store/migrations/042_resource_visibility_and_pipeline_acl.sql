-- Three-tier resource visibility and pipeline-level ACL.
-- Phase 0 of the master platform plan (ADR-021).

-- Visibility enum shared by projects and pipelines.
CREATE TYPE resource_visibility AS ENUM ('public', 'authenticated', 'private');

ALTER TABLE projects
    ADD COLUMN IF NOT EXISTS visibility resource_visibility NOT NULL DEFAULT 'authenticated';

-- Pipelines gain explicit ownership (backfilled from parent project) and visibility.
ALTER TABLE pipelines
    ADD COLUMN IF NOT EXISTS owner_type owner_type NOT NULL DEFAULT 'user',
    ADD COLUMN IF NOT EXISTS owner_id   TEXT       NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS visibility  resource_visibility NOT NULL DEFAULT 'authenticated';

-- Pipeline-level membership (separate from project_members).
CREATE TYPE pipeline_principal_type AS ENUM ('user', 'group');
CREATE TYPE pipeline_role AS ENUM ('admin', 'developer', 'readonly');

CREATE TABLE IF NOT EXISTS pipeline_members (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    pipeline_id    UUID NOT NULL REFERENCES pipelines(id) ON DELETE CASCADE,
    principal_type pipeline_principal_type NOT NULL,
    principal_id   UUID NOT NULL,
    role           pipeline_role NOT NULL,
    inherited      BOOLEAN NOT NULL DEFAULT false,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (pipeline_id, principal_type, principal_id)
);

CREATE INDEX IF NOT EXISTS idx_pipeline_members_pipeline
    ON pipeline_members(pipeline_id);
CREATE INDEX IF NOT EXISTS idx_pipeline_members_principal
    ON pipeline_members(principal_type, principal_id);

-- Super-admin: unrestricted access (break-glass).  Existing 'admin' becomes metadata-only.
-- ALTER TYPE ... ADD VALUE is not transactional; safe to run in migration runner.
ALTER TYPE permission_role ADD VALUE IF NOT EXISTS 'super_admin';

-- Backfill pipeline ownership from parent project.
UPDATE pipelines p
SET owner_type = pr.owner_type, owner_id = pr.owner_id
FROM projects pr
WHERE p.project_id = pr.id AND p.owner_id = '';
