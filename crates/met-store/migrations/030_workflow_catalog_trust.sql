-- Workflow catalog: provenance, approval/trust, soft-delete, org policy for untrusted workflows.

CREATE TYPE workflow_source AS ENUM ('git', 'api', 'project_sync');
CREATE TYPE workflow_submission_status AS ENUM ('pending', 'approved', 'rejected');
CREATE TYPE workflow_trust_state AS ENUM ('trusted', 'untrusted');

ALTER TABLE organizations
    ADD COLUMN IF NOT EXISTS allow_untrusted_workflows BOOLEAN NOT NULL DEFAULT TRUE;

ALTER TABLE reusable_workflows
    ADD COLUMN IF NOT EXISTS source workflow_source NOT NULL DEFAULT 'api',
    ADD COLUMN IF NOT EXISTS scm_repository TEXT,
    ADD COLUMN IF NOT EXISTS scm_ref TEXT,
    ADD COLUMN IF NOT EXISTS scm_path TEXT,
    ADD COLUMN IF NOT EXISTS scm_revision TEXT,
    ADD COLUMN IF NOT EXISTS submission_status workflow_submission_status NOT NULL DEFAULT 'approved',
    ADD COLUMN IF NOT EXISTS trust_state workflow_trust_state NOT NULL DEFAULT 'trusted',
    ADD COLUMN IF NOT EXISTS submitted_by UUID REFERENCES users(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS reviewed_by UUID REFERENCES users(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS reviewed_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS catalog_metadata JSONB NOT NULL DEFAULT '{}'::jsonb;

-- Backfill scope-appropriate sources (global default is already 'api').
UPDATE reusable_workflows SET source = 'project_sync' WHERE scope = 'project';

-- Remove duplicate global catalog rows before partial unique index (keep newest by created_at, then id).
DELETE FROM reusable_workflows a
    USING reusable_workflows b
WHERE a.scope = 'global'
  AND a.project_id IS NULL
  AND b.scope = 'global'
  AND b.project_id IS NULL
  AND a.org_id = b.org_id
  AND a.name = b.name
  AND a.version = b.version
  AND (a.created_at < b.created_at OR (a.created_at = b.created_at AND a.id < b.id));

CREATE UNIQUE INDEX IF NOT EXISTS idx_reusable_workflows_global_org_name_version
    ON reusable_workflows (org_id, name, version)
    WHERE scope = 'global' AND project_id IS NULL;

CREATE INDEX IF NOT EXISTS idx_reusable_workflows_catalog_global_name
    ON reusable_workflows (org_id, name)
    WHERE scope = 'global' AND project_id IS NULL AND deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_reusable_workflows_submission_status
    ON reusable_workflows (org_id, submission_status)
    WHERE scope = 'global' AND project_id IS NULL AND deleted_at IS NULL;
