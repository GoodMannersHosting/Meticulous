-- External secret provider configurations (ADR-020, Phase 1.2).
-- Stores encrypted connection details for AWS SM, Vault, GCP SM, Azure KV,
-- Kubernetes, Bitwarden, 1Password, Akeyless, and Conjur.

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'secret_provider_type') THEN
        CREATE TYPE secret_provider_type AS ENUM (
            'aws_sm', 'vault', 'gcp_sm', 'azure_kv',
            'kubernetes', 'bitwarden', 'onepassword',
            'akeyless', 'conjur'
        );
    END IF;
END $$;

CREATE TABLE IF NOT EXISTS secret_provider_configs (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id            UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    project_id        UUID REFERENCES projects(id) ON DELETE CASCADE,
    name              TEXT NOT NULL CHECK (name ~ '^[a-z0-9][a-z0-9_-]{0,62}$'),
    provider_type     secret_provider_type NOT NULL,
    config_encrypted  BYTEA NOT NULL,
    resolution_mode   TEXT NOT NULL DEFAULT 'remote'
                      CHECK (resolution_mode IN ('local', 'remote', 'auto')),
    enabled           BOOLEAN NOT NULL DEFAULT true,
    last_tested_at    TIMESTAMPTZ,
    last_test_ok      BOOLEAN,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_secret_provider_configs_unique_name
    ON secret_provider_configs (
        org_id,
        COALESCE(project_id, '00000000-0000-0000-0000-000000000000'::uuid),
        name
    );

CREATE INDEX IF NOT EXISTS idx_secret_provider_configs_org ON secret_provider_configs(org_id);
CREATE INDEX IF NOT EXISTS idx_secret_provider_configs_project ON secret_provider_configs(project_id) WHERE project_id IS NOT NULL;
