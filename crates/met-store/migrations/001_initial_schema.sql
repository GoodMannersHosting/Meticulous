-- Meticulous CI/CD Initial Schema
-- This migration creates all core tables, types, indexes, and triggers.

-- ============================================================================
-- Custom Enum Types
-- ============================================================================

CREATE TYPE run_status AS ENUM (
    'pending',
    'queued',
    'running',
    'succeeded',
    'failed',
    'cancelled',
    'timed_out',
    'skipped'
);

CREATE TYPE agent_status AS ENUM (
    'online',
    'offline',
    'busy',
    'draining',
    'decommissioned'
);

CREATE TYPE trigger_kind AS ENUM (
    'webhook',
    'manual',
    'tag_push',
    'schedule'
);

CREATE TYPE secret_scope AS ENUM (
    'global',
    'project'
);

CREATE TYPE variable_scope AS ENUM (
    'global',
    'project'
);

CREATE TYPE workflow_scope AS ENUM (
    'global',
    'project'
);

CREATE TYPE owner_type AS ENUM (
    'user',
    'group'
);

CREATE TYPE step_kind AS ENUM (
    'command',
    'workflow_ref',
    'plugin'
);

CREATE TYPE group_role AS ENUM (
    'member',
    'maintainer',
    'owner'
);

-- ============================================================================
-- Helper Function for updated_at Triggers
-- ============================================================================

CREATE OR REPLACE FUNCTION set_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- Organizations (Tenant Boundary)
-- ============================================================================

CREATE TABLE organizations (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ
);

CREATE INDEX idx_organizations_slug ON organizations(slug) WHERE deleted_at IS NULL;

CREATE TRIGGER organizations_updated_at
    BEFORE UPDATE ON organizations
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ============================================================================
-- Users
-- ============================================================================

CREATE TABLE users (
    id UUID PRIMARY KEY,
    org_id UUID NOT NULL REFERENCES organizations(id),
    username TEXT NOT NULL,
    email TEXT NOT NULL,
    display_name TEXT,
    password_hash TEXT,
    is_active BOOLEAN NOT NULL DEFAULT true,
    is_admin BOOLEAN NOT NULL DEFAULT false,
    external_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ,
    UNIQUE(org_id, username),
    UNIQUE(org_id, email)
);

CREATE INDEX idx_users_org ON users(org_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_users_external_id ON users(external_id) WHERE external_id IS NOT NULL;

CREATE TRIGGER users_updated_at
    BEFORE UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ============================================================================
-- Groups
-- ============================================================================

CREATE TABLE groups (
    id UUID PRIMARY KEY,
    org_id UUID NOT NULL REFERENCES organizations(id),
    name TEXT NOT NULL,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(org_id, name)
);

CREATE INDEX idx_groups_org ON groups(org_id);

CREATE TRIGGER groups_updated_at
    BEFORE UPDATE ON groups
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ============================================================================
-- Group Memberships
-- ============================================================================

CREATE TABLE group_memberships (
    group_id UUID NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role group_role NOT NULL DEFAULT 'member',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (group_id, user_id)
);

CREATE INDEX idx_group_memberships_user ON group_memberships(user_id);

-- ============================================================================
-- Projects
-- ============================================================================

CREATE TABLE projects (
    id UUID PRIMARY KEY,
    org_id UUID NOT NULL REFERENCES organizations(id),
    name TEXT NOT NULL,
    slug TEXT NOT NULL,
    description TEXT,
    owner_type owner_type NOT NULL,
    owner_id TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ,
    UNIQUE(org_id, slug)
);

CREATE INDEX idx_projects_org ON projects(org_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_projects_owner ON projects(owner_type, owner_id) WHERE deleted_at IS NULL;

CREATE TRIGGER projects_updated_at
    BEFORE UPDATE ON projects
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ============================================================================
-- Pipelines
-- ============================================================================

CREATE TABLE pipelines (
    id UUID PRIMARY KEY,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    slug TEXT NOT NULL,
    description TEXT,
    definition JSONB NOT NULL DEFAULT '{}',
    definition_path TEXT,
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(project_id, slug)
);

CREATE INDEX idx_pipelines_project ON pipelines(project_id);

CREATE TRIGGER pipelines_updated_at
    BEFORE UPDATE ON pipelines
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ============================================================================
-- Jobs (DAG Nodes)
-- ============================================================================

CREATE TABLE jobs (
    id UUID PRIMARY KEY,
    pipeline_id UUID NOT NULL REFERENCES pipelines(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    depends_on TEXT[] NOT NULL DEFAULT '{}',
    agent_tags TEXT[] NOT NULL DEFAULT '{}',
    timeout_secs INTEGER,
    retry_count INTEGER NOT NULL DEFAULT 0,
    condition TEXT,
    config JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(pipeline_id, name)
);

CREATE INDEX idx_jobs_pipeline ON jobs(pipeline_id);

-- ============================================================================
-- Steps
-- ============================================================================

CREATE TABLE steps (
    id UUID PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    kind step_kind NOT NULL DEFAULT 'command',
    command TEXT,
    working_dir TEXT,
    shell TEXT,
    workflow_ref TEXT,
    plugin TEXT,
    environment JSONB NOT NULL DEFAULT '{}',
    sequence INTEGER NOT NULL,
    continue_on_error BOOLEAN NOT NULL DEFAULT false,
    timeout_secs INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_steps_job ON steps(job_id);
CREATE INDEX idx_steps_job_sequence ON steps(job_id, sequence);

-- ============================================================================
-- Triggers
-- ============================================================================

CREATE TABLE triggers (
    id UUID PRIMARY KEY,
    pipeline_id UUID NOT NULL REFERENCES pipelines(id) ON DELETE CASCADE,
    kind trigger_kind NOT NULL,
    config JSONB NOT NULL DEFAULT '{}',
    enabled BOOLEAN NOT NULL DEFAULT true,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_triggers_pipeline ON triggers(pipeline_id);
CREATE INDEX idx_triggers_kind ON triggers(kind) WHERE enabled = true;

CREATE TRIGGER triggers_updated_at
    BEFORE UPDATE ON triggers
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ============================================================================
-- Runs (Pipeline Execution Records)
-- ============================================================================

CREATE TABLE runs (
    id UUID PRIMARY KEY,
    pipeline_id UUID NOT NULL REFERENCES pipelines(id) ON DELETE CASCADE,
    trigger_id UUID REFERENCES triggers(id) ON DELETE SET NULL,
    status run_status NOT NULL DEFAULT 'pending',
    run_number BIGINT NOT NULL,
    commit_sha TEXT,
    branch TEXT,
    triggered_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ
);

CREATE INDEX idx_runs_pipeline_status ON runs(pipeline_id, status);
CREATE INDEX idx_runs_created ON runs(created_at DESC);
CREATE INDEX idx_runs_pipeline_number ON runs(pipeline_id, run_number DESC);

-- ============================================================================
-- Job Runs
-- ============================================================================

CREATE TABLE job_runs (
    id UUID PRIMARY KEY,
    run_id UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    job_id UUID NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    agent_id UUID,
    status run_status NOT NULL DEFAULT 'pending',
    attempt INTEGER NOT NULL DEFAULT 0,
    log_path TEXT,
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_job_runs_run ON job_runs(run_id);
CREATE INDEX idx_job_runs_job ON job_runs(job_id);
CREATE INDEX idx_job_runs_agent ON job_runs(agent_id) WHERE agent_id IS NOT NULL;
CREATE INDEX idx_job_runs_status ON job_runs(status) WHERE status IN ('pending', 'queued', 'running');

-- ============================================================================
-- Step Runs
-- ============================================================================

CREATE TABLE step_runs (
    id UUID PRIMARY KEY,
    job_run_id UUID NOT NULL REFERENCES job_runs(id) ON DELETE CASCADE,
    step_id UUID NOT NULL REFERENCES steps(id) ON DELETE CASCADE,
    status run_status NOT NULL DEFAULT 'pending',
    exit_code INTEGER,
    log_path TEXT,
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_step_runs_job_run ON step_runs(job_run_id);

-- ============================================================================
-- Agents
-- ============================================================================

CREATE TABLE agents (
    id UUID PRIMARY KEY,
    org_id UUID NOT NULL REFERENCES organizations(id),
    name TEXT NOT NULL,
    status agent_status NOT NULL DEFAULT 'offline',
    pool TEXT,
    tags TEXT[] NOT NULL DEFAULT '{}',
    capabilities JSONB NOT NULL DEFAULT '{}',
    os TEXT NOT NULL,
    arch TEXT NOT NULL,
    version TEXT NOT NULL,
    ip_address TEXT,
    max_jobs INTEGER NOT NULL DEFAULT 1,
    running_jobs INTEGER NOT NULL DEFAULT 0,
    last_heartbeat_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_agents_org_status ON agents(org_id, status);
CREATE INDEX idx_agents_pool ON agents(pool) WHERE pool IS NOT NULL;
CREATE INDEX idx_agents_tags ON agents USING gin(tags);
CREATE INDEX idx_agents_available ON agents(org_id, running_jobs) WHERE status = 'online';

-- ============================================================================
-- Agent Tokens
-- ============================================================================

CREATE TABLE agent_tokens (
    id UUID PRIMARY KEY,
    agent_id UUID REFERENCES agents(id) ON DELETE CASCADE,
    org_id UUID NOT NULL REFERENCES organizations(id),
    token_hash TEXT NOT NULL UNIQUE,
    name TEXT,
    scope JSONB NOT NULL DEFAULT '{}',
    expires_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_agent_tokens_agent ON agent_tokens(agent_id) WHERE agent_id IS NOT NULL;
CREATE INDEX idx_agent_tokens_hash ON agent_tokens(token_hash) WHERE revoked_at IS NULL;

-- ============================================================================
-- Secrets (External References Only)
-- ============================================================================

CREATE TABLE secrets (
    id UUID PRIMARY KEY,
    org_id UUID NOT NULL REFERENCES organizations(id),
    project_id UUID REFERENCES projects(id) ON DELETE CASCADE,
    scope secret_scope NOT NULL DEFAULT 'global',
    name TEXT NOT NULL,
    provider TEXT NOT NULL,
    provider_ref TEXT NOT NULL,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(org_id, project_id, name)
);

CREATE INDEX idx_secrets_org_scope ON secrets(org_id, scope);
CREATE INDEX idx_secrets_project ON secrets(project_id) WHERE project_id IS NOT NULL;

CREATE TRIGGER secrets_updated_at
    BEFORE UPDATE ON secrets
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ============================================================================
-- Variables
-- ============================================================================

CREATE TABLE variables (
    id UUID PRIMARY KEY,
    org_id UUID NOT NULL REFERENCES organizations(id),
    project_id UUID REFERENCES projects(id) ON DELETE CASCADE,
    scope variable_scope NOT NULL DEFAULT 'global',
    name TEXT NOT NULL,
    value TEXT NOT NULL,
    is_sensitive BOOLEAN NOT NULL DEFAULT false,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(org_id, project_id, name)
);

CREATE INDEX idx_variables_org_scope ON variables(org_id, scope);
CREATE INDEX idx_variables_project ON variables(project_id) WHERE project_id IS NOT NULL;

CREATE TRIGGER variables_updated_at
    BEFORE UPDATE ON variables
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ============================================================================
-- Reusable Workflows
-- ============================================================================

CREATE TABLE reusable_workflows (
    id UUID PRIMARY KEY,
    org_id UUID NOT NULL REFERENCES organizations(id),
    project_id UUID REFERENCES projects(id) ON DELETE CASCADE,
    scope workflow_scope NOT NULL DEFAULT 'global',
    name TEXT NOT NULL,
    version TEXT NOT NULL,
    definition JSONB NOT NULL,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(org_id, project_id, name, version)
);

CREATE INDEX idx_workflows_org_scope ON reusable_workflows(org_id, scope);
CREATE INDEX idx_workflows_project ON reusable_workflows(project_id) WHERE project_id IS NOT NULL;

CREATE TRIGGER reusable_workflows_updated_at
    BEFORE UPDATE ON reusable_workflows
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ============================================================================
-- Artifacts
-- ============================================================================

CREATE TABLE artifacts (
    id UUID PRIMARY KEY,
    run_id UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    job_run_id UUID NOT NULL REFERENCES job_runs(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    content_type TEXT,
    size_bytes BIGINT NOT NULL,
    storage_path TEXT NOT NULL,
    sha256 TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ
);

CREATE INDEX idx_artifacts_run ON artifacts(run_id);
CREATE INDEX idx_artifacts_job_run ON artifacts(job_run_id);
CREATE INDEX idx_artifacts_expires ON artifacts(expires_at) WHERE expires_at IS NOT NULL;

-- ============================================================================
-- Agent Pools
-- ============================================================================

CREATE TABLE agent_pools (
    id UUID PRIMARY KEY,
    org_id UUID NOT NULL REFERENCES organizations(id),
    name TEXT NOT NULL,
    description TEXT,
    auto_scale BOOLEAN NOT NULL DEFAULT false,
    min_agents INTEGER NOT NULL DEFAULT 0,
    max_agents INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(org_id, name)
);

CREATE INDEX idx_agent_pools_org ON agent_pools(org_id);
