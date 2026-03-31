-- Agent System Schema Extensions
-- This migration adds tables for join tokens, agent heartbeats, and job assignments.

-- ============================================================================
-- Additional Enum Types
-- ============================================================================

CREATE TYPE join_token_scope AS ENUM (
    'platform',
    'tenant',
    'project',
    'pipeline'
);

CREATE TYPE job_assignment_status AS ENUM (
    'accepted',
    'running',
    'succeeded',
    'failed',
    'cancelled',
    'timed_out'
);

CREATE TYPE environment_type AS ENUM (
    'physical',
    'virtual',
    'container'
);

-- ============================================================================
-- Join Tokens (Agent Enrollment)
-- ============================================================================

CREATE TABLE join_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    token_hash TEXT NOT NULL UNIQUE,
    scope join_token_scope NOT NULL DEFAULT 'tenant',
    scope_id UUID,
    max_uses INTEGER,
    current_uses INTEGER NOT NULL DEFAULT 0,
    labels TEXT[] NOT NULL DEFAULT '{}',
    pool_tags TEXT[] NOT NULL DEFAULT '{}',
    expires_at TIMESTAMPTZ,
    revoked BOOLEAN NOT NULL DEFAULT false,
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT join_tokens_scope_check CHECK (
        (scope = 'platform' AND scope_id IS NULL) OR
        (scope != 'platform' AND scope_id IS NOT NULL)
    )
);

CREATE INDEX idx_join_tokens_hash ON join_tokens(token_hash) WHERE NOT revoked;
CREATE INDEX idx_join_tokens_scope ON join_tokens(scope, scope_id);

CREATE TRIGGER join_tokens_updated_at
    BEFORE UPDATE ON join_tokens
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ============================================================================
-- Enhanced Agents Table (add security bundle fields)
-- ============================================================================

ALTER TABLE agents 
    ADD COLUMN IF NOT EXISTS environment_type environment_type DEFAULT 'virtual',
    ADD COLUMN IF NOT EXISTS kernel_version TEXT,
    ADD COLUMN IF NOT EXISTS public_ips TEXT[] NOT NULL DEFAULT '{}',
    ADD COLUMN IF NOT EXISTS private_ips TEXT[] NOT NULL DEFAULT '{}',
    ADD COLUMN IF NOT EXISTS ntp_synchronized BOOLEAN DEFAULT true,
    ADD COLUMN IF NOT EXISTS container_runtime TEXT,
    ADD COLUMN IF NOT EXISTS container_runtime_version TEXT,
    ADD COLUMN IF NOT EXISTS x509_public_key BYTEA,
    ADD COLUMN IF NOT EXISTS join_token_id UUID REFERENCES join_tokens(id),
    ADD COLUMN IF NOT EXISTS jwt_expires_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS jwt_renewable BOOLEAN NOT NULL DEFAULT true,
    ADD COLUMN IF NOT EXISTS deregistered_at TIMESTAMPTZ;

CREATE INDEX idx_agents_join_token ON agents(join_token_id) WHERE join_token_id IS NOT NULL;

-- ============================================================================
-- Agent Heartbeats (Diagnostic History)
-- ============================================================================

CREATE TABLE agent_heartbeats (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    status agent_status NOT NULL,
    cpu_percent REAL,
    memory_percent REAL,
    disk_percent REAL,
    current_job_id UUID,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_agent_heartbeats_agent_time ON agent_heartbeats(agent_id, recorded_at DESC);

-- Partition hint: In production, consider partitioning this table by recorded_at
-- and setting up automatic partition management with short retention (24-48 hours).

-- ============================================================================
-- Job Assignments (Agent-Job Mapping)
-- ============================================================================

CREATE TABLE job_assignments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_run_id UUID NOT NULL REFERENCES job_runs(id) ON DELETE CASCADE,
    agent_id UUID NOT NULL REFERENCES agents(id),
    status job_assignment_status NOT NULL DEFAULT 'accepted',
    accepted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    exit_code INTEGER,
    failure_reason TEXT,
    attempt INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX idx_job_assignments_job_run ON job_assignments(job_run_id);
CREATE INDEX idx_job_assignments_agent ON job_assignments(agent_id) WHERE status IN ('accepted', 'running');
CREATE INDEX idx_job_assignments_status ON job_assignments(status) WHERE status IN ('accepted', 'running');

-- ============================================================================
-- Add revoked status to agent_status enum
-- ============================================================================

ALTER TYPE agent_status ADD VALUE IF NOT EXISTS 'revoked';
ALTER TYPE agent_status ADD VALUE IF NOT EXISTS 'dead';
