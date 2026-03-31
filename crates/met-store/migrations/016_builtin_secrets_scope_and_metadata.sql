-- Extend builtin_secrets for pipeline scope, kind, metadata, description.
-- Fix uniqueness to include pipeline_id (nullable = org/project-only row).

ALTER TABLE builtin_secrets
    ADD COLUMN IF NOT EXISTS pipeline_id UUID REFERENCES pipelines(id) ON DELETE CASCADE,
    ADD COLUMN IF NOT EXISTS kind TEXT NOT NULL DEFAULT 'kv'
        CHECK (kind IN ('kv', 'ssh_private_key', 'github_app', 'api_key', 'x509_bundle')),
    ADD COLUMN IF NOT EXISTS metadata JSONB NOT NULL DEFAULT '{}',
    ADD COLUMN IF NOT EXISTS description TEXT;

DROP INDEX IF EXISTS idx_builtin_secrets_unique;

CREATE UNIQUE INDEX idx_builtin_secrets_unique
    ON builtin_secrets (
        org_id,
        COALESCE(project_id, '00000000-0000-0000-0000-000000000000'::uuid),
        COALESCE(pipeline_id, '00000000-0000-0000-0000-000000000000'::uuid),
        path,
        version
    );

CREATE INDEX IF NOT EXISTS idx_builtin_secrets_pipeline
    ON builtin_secrets (pipeline_id, path)
    WHERE pipeline_id IS NOT NULL AND deleted_at IS NULL;
