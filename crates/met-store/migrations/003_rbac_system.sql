-- RBAC System and Admin Portal Foundation
-- This migration adds:
-- 1. Project lifecycle (archive, scheduled deletion)
-- 2. Permission roles and user role assignments
-- 3. API tokens for programmatic access
-- 4. Auth providers (OIDC, GitHub) and group mappings
-- 5. Platform settings

-- ============================================================================
-- Project Lifecycle Extensions
-- ============================================================================

ALTER TABLE projects 
    ADD COLUMN IF NOT EXISTS archived_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS scheduled_deletion_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_projects_archived 
    ON projects(org_id, archived_at) 
    WHERE archived_at IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_projects_scheduled_deletion 
    ON projects(scheduled_deletion_at) 
    WHERE scheduled_deletion_at IS NOT NULL;

-- ============================================================================
-- Platform Settings
-- ============================================================================

CREATE TABLE IF NOT EXISTS platform_settings (
    key TEXT PRIMARY KEY,
    value JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_by UUID REFERENCES users(id) ON DELETE SET NULL
);

CREATE TRIGGER platform_settings_updated_at
    BEFORE UPDATE ON platform_settings
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

INSERT INTO platform_settings (key, value) VALUES 
    ('project_deletion_retention_days', '7'),
    ('password_auth_enabled', 'true')
ON CONFLICT (key) DO NOTHING;

-- ============================================================================
-- Permission Roles
-- ============================================================================

CREATE TYPE permission_role AS ENUM ('admin', 'auditor', 'security_lead', 'user');

CREATE TABLE IF NOT EXISTS user_roles (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role permission_role NOT NULL,
    granted_by UUID REFERENCES users(id) ON DELETE SET NULL,
    granted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, role)
);

CREATE INDEX IF NOT EXISTS idx_user_roles_user ON user_roles(user_id);
CREATE INDEX IF NOT EXISTS idx_user_roles_role ON user_roles(role);

-- ============================================================================
-- API Tokens
-- ============================================================================

CREATE TABLE IF NOT EXISTS api_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    prefix TEXT NOT NULL,
    scopes TEXT[] NOT NULL DEFAULT '{}',
    project_ids UUID[] DEFAULT NULL,
    expires_at TIMESTAMPTZ,
    last_used_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_api_tokens_user ON api_tokens(user_id) WHERE revoked_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_api_tokens_hash ON api_tokens(token_hash) WHERE revoked_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_api_tokens_prefix ON api_tokens(prefix);

-- ============================================================================
-- Auth Providers (OIDC, GitHub)
-- ============================================================================

CREATE TABLE IF NOT EXISTS auth_providers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    provider_type TEXT NOT NULL,
    name TEXT NOT NULL,
    client_id TEXT NOT NULL,
    client_secret_ref TEXT NOT NULL,
    issuer_url TEXT,
    enabled BOOLEAN NOT NULL DEFAULT false,
    config JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(org_id, name)
);

CREATE INDEX IF NOT EXISTS idx_auth_providers_org ON auth_providers(org_id);
CREATE INDEX IF NOT EXISTS idx_auth_providers_type ON auth_providers(org_id, provider_type);

CREATE TRIGGER auth_providers_updated_at
    BEFORE UPDATE ON auth_providers
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- Ensure only one provider per type can be enabled at a time per org
CREATE UNIQUE INDEX IF NOT EXISTS idx_auth_providers_one_enabled_per_type 
    ON auth_providers(org_id, provider_type) 
    WHERE enabled = true;

-- ============================================================================
-- OIDC Group Mappings
-- ============================================================================

CREATE TABLE IF NOT EXISTS oidc_group_mappings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_id UUID NOT NULL REFERENCES auth_providers(id) ON DELETE CASCADE,
    oidc_group_claim TEXT NOT NULL,
    meticulous_group_id UUID NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    role group_role NOT NULL DEFAULT 'member',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(provider_id, oidc_group_claim, meticulous_group_id)
);

CREATE INDEX IF NOT EXISTS idx_oidc_group_mappings_provider ON oidc_group_mappings(provider_id);
CREATE INDEX IF NOT EXISTS idx_oidc_group_mappings_group ON oidc_group_mappings(meticulous_group_id);

-- ============================================================================
-- Cascade Constraints for Project Deletion
-- Ensure all project-related data is properly cascaded on deletion
-- ============================================================================

-- These ALTER statements ensure ON DELETE CASCADE is set properly
-- The IF EXISTS and error handling makes this idempotent

DO $$
BEGIN
    -- Pipelines
    IF EXISTS (SELECT 1 FROM information_schema.table_constraints 
               WHERE constraint_name = 'pipelines_project_id_fkey' 
               AND table_name = 'pipelines') THEN
        ALTER TABLE pipelines DROP CONSTRAINT pipelines_project_id_fkey;
    END IF;
    ALTER TABLE pipelines ADD CONSTRAINT pipelines_project_id_fkey 
        FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE;

    -- Secrets
    IF EXISTS (SELECT 1 FROM information_schema.table_constraints 
               WHERE constraint_name = 'secrets_project_id_fkey' 
               AND table_name = 'secrets') THEN
        ALTER TABLE secrets DROP CONSTRAINT secrets_project_id_fkey;
    END IF;
    ALTER TABLE secrets ADD CONSTRAINT secrets_project_id_fkey 
        FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE;

    -- Variables
    IF EXISTS (SELECT 1 FROM information_schema.table_constraints 
               WHERE constraint_name = 'variables_project_id_fkey' 
               AND table_name = 'variables') THEN
        ALTER TABLE variables DROP CONSTRAINT variables_project_id_fkey;
    END IF;
    ALTER TABLE variables ADD CONSTRAINT variables_project_id_fkey 
        FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE;

    -- Reusable workflows
    IF EXISTS (SELECT 1 FROM information_schema.table_constraints 
               WHERE constraint_name = 'reusable_workflows_project_id_fkey' 
               AND table_name = 'reusable_workflows') THEN
        ALTER TABLE reusable_workflows DROP CONSTRAINT reusable_workflows_project_id_fkey;
    END IF;
    ALTER TABLE reusable_workflows ADD CONSTRAINT reusable_workflows_project_id_fkey 
        FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE;
END $$;
