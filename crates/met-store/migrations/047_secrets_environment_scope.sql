-- Environment-scoped secrets (ADR-016, Phase 2.1).
-- Allows the same secret name to exist at both project and environment scope.

ALTER TABLE builtin_secrets ADD COLUMN IF NOT EXISTS environment_id UUID REFERENCES environments(id);

-- Rebuild unique index to include environment_id.
-- The previous index (from 016) is on (org_id, project_id, pipeline_id, path, version).
-- We replace it with one that includes environment_id for environment-scoped secrets.
DROP INDEX IF EXISTS idx_builtin_secrets_unique;
CREATE UNIQUE INDEX idx_builtin_secrets_unique
    ON builtin_secrets (
        org_id,
        COALESCE(project_id, '00000000-0000-0000-0000-000000000000'::uuid),
        COALESCE(pipeline_id, '00000000-0000-0000-0000-000000000000'::uuid),
        COALESCE(environment_id, '00000000-0000-0000-0000-000000000000'::uuid),
        path,
        version
    )
    WHERE deleted_at IS NULL;
