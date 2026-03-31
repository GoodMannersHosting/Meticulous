-- Security Infrastructure
-- Adds tables for: secret provider configuration, built-in encrypted secrets,
-- per-job PKI certificates, OIDC signing keys, append-only audit log,
-- syscall/binary execution tracking, network metadata, and known binaries.

-- ============================================================================
-- Secret Provider Configurations
-- ============================================================================

CREATE TABLE IF NOT EXISTS secret_provider_configs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    project_id UUID REFERENCES projects(id) ON DELETE CASCADE,
    provider_type TEXT NOT NULL,
    name TEXT NOT NULL,
    config JSONB NOT NULL DEFAULT '{}',
    is_default BOOLEAN NOT NULL DEFAULT false,
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX idx_secret_provider_configs_unique
    ON secret_provider_configs(org_id, COALESCE(project_id, '00000000-0000-0000-0000-000000000000'::uuid), name);

CREATE INDEX idx_secret_provider_configs_org ON secret_provider_configs(org_id);
CREATE INDEX idx_secret_provider_configs_project ON secret_provider_configs(project_id)
    WHERE project_id IS NOT NULL;
CREATE INDEX idx_secret_provider_configs_default ON secret_provider_configs(org_id, is_default)
    WHERE is_default = true AND enabled = true;

CREATE TRIGGER secret_provider_configs_updated_at
    BEFORE UPDATE ON secret_provider_configs
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ============================================================================
-- Built-in Encrypted Secrets
-- ============================================================================

CREATE TABLE IF NOT EXISTS builtin_secrets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    project_id UUID REFERENCES projects(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    encrypted_value BYTEA NOT NULL,
    nonce BYTEA NOT NULL,
    key_id TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    content_type TEXT DEFAULT 'text/plain',
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);

CREATE UNIQUE INDEX idx_builtin_secrets_unique
    ON builtin_secrets(org_id, COALESCE(project_id, '00000000-0000-0000-0000-000000000000'::uuid), path, version);

CREATE INDEX idx_builtin_secrets_lookup ON builtin_secrets(org_id, path)
    WHERE deleted_at IS NULL;
CREATE INDEX idx_builtin_secrets_project ON builtin_secrets(project_id, path)
    WHERE project_id IS NOT NULL AND deleted_at IS NULL;
CREATE INDEX idx_builtin_secrets_key_id ON builtin_secrets(key_id);

CREATE TRIGGER builtin_secrets_updated_at
    BEFORE UPDATE ON builtin_secrets
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ============================================================================
-- Job Certificates (per-job ephemeral PKI audit trail)
-- ============================================================================

CREATE TABLE IF NOT EXISTS job_certificates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_run_id UUID NOT NULL REFERENCES job_runs(id) ON DELETE CASCADE,
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    serial_number TEXT NOT NULL UNIQUE,
    subject_cn TEXT NOT NULL,
    issuer_cn TEXT NOT NULL,
    public_key_fingerprint TEXT NOT NULL,
    not_before TIMESTAMPTZ NOT NULL,
    not_after TIMESTAMPTZ NOT NULL,
    consumed BOOLEAN NOT NULL DEFAULT false,
    consumed_at TIMESTAMPTZ,
    revoked BOOLEAN NOT NULL DEFAULT false,
    revoked_at TIMESTAMPTZ,
    revocation_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_job_certificates_job ON job_certificates(job_run_id);
CREATE INDEX idx_job_certificates_agent ON job_certificates(agent_id);
CREATE INDEX idx_job_certificates_serial ON job_certificates(serial_number);
CREATE INDEX idx_job_certificates_active ON job_certificates(not_after)
    WHERE NOT consumed AND NOT revoked;

-- ============================================================================
-- OIDC Signing Keys (rotatable keys for API auth and pipeline OIDC)
-- ============================================================================

CREATE TYPE oidc_key_purpose AS ENUM ('api_auth', 'pipeline_identity');

CREATE TABLE IF NOT EXISTS oidc_signing_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    kid TEXT NOT NULL UNIQUE,
    purpose oidc_key_purpose NOT NULL,
    algorithm TEXT NOT NULL DEFAULT 'RS256',
    private_key_pem TEXT NOT NULL,
    public_key_pem TEXT NOT NULL,
    active_from TIMESTAMPTZ NOT NULL DEFAULT now(),
    active_until TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_oidc_signing_keys_active ON oidc_signing_keys(purpose, active_from, active_until);
CREATE INDEX idx_oidc_signing_keys_kid ON oidc_signing_keys(kid);

-- ============================================================================
-- Audit Log (append-only with trigger preventing UPDATE/DELETE)
-- ============================================================================

CREATE TABLE IF NOT EXISTS audit_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    timestamp TIMESTAMPTZ NOT NULL DEFAULT now(),
    action TEXT NOT NULL,
    actor_type TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    actor_name TEXT,
    resource_type TEXT,
    resource_id TEXT,
    resource_name TEXT,
    org_id UUID REFERENCES organizations(id) ON DELETE SET NULL,
    project_id UUID REFERENCES projects(id) ON DELETE SET NULL,
    outcome TEXT NOT NULL DEFAULT 'unknown',
    client_ip INET,
    user_agent TEXT,
    request_id TEXT,
    metadata JSONB NOT NULL DEFAULT '{}',
    error_message TEXT
);

CREATE INDEX idx_audit_log_timestamp ON audit_log(timestamp DESC);
CREATE INDEX idx_audit_log_actor ON audit_log(actor_type, actor_id);
CREATE INDEX idx_audit_log_action ON audit_log(action, timestamp DESC);
CREATE INDEX idx_audit_log_resource ON audit_log(resource_type, resource_id)
    WHERE resource_type IS NOT NULL;
CREATE INDEX idx_audit_log_org ON audit_log(org_id, timestamp DESC)
    WHERE org_id IS NOT NULL;
CREATE INDEX idx_audit_log_project ON audit_log(project_id, timestamp DESC)
    WHERE project_id IS NOT NULL;
CREATE INDEX idx_audit_log_request ON audit_log(request_id)
    WHERE request_id IS NOT NULL;

-- Prevent UPDATE and DELETE on audit_log (append-only)
CREATE OR REPLACE FUNCTION audit_log_immutable()
RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'audit_log is append-only: % operations are forbidden', TG_OP;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER audit_log_no_update
    BEFORE UPDATE ON audit_log
    FOR EACH ROW EXECUTE FUNCTION audit_log_immutable();

CREATE TRIGGER audit_log_no_delete
    BEFORE DELETE ON audit_log
    FOR EACH ROW EXECUTE FUNCTION audit_log_immutable();

-- ============================================================================
-- Run Binary Executions (syscall audit: every exec during a run)
-- ============================================================================

CREATE TABLE IF NOT EXISTS run_binary_executions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    job_run_id UUID REFERENCES job_runs(id) ON DELETE CASCADE,
    agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    binary_path TEXT NOT NULL,
    binary_sha256 TEXT NOT NULL,
    argv TEXT[],
    pid INTEGER,
    ppid INTEGER,
    executed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_run_binary_executions_run ON run_binary_executions(run_id);
CREATE INDEX idx_run_binary_executions_job ON run_binary_executions(job_run_id)
    WHERE job_run_id IS NOT NULL;
CREATE INDEX idx_run_binary_executions_sha ON run_binary_executions(binary_sha256);
CREATE INDEX idx_run_binary_executions_agent ON run_binary_executions(agent_id)
    WHERE agent_id IS NOT NULL;

-- ============================================================================
-- Run Network Connections (per-run network metadata)
-- ============================================================================

CREATE TABLE IF NOT EXISTS run_network_connections (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    job_run_id UUID REFERENCES job_runs(id) ON DELETE CASCADE,
    agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    src_ip INET NOT NULL,
    src_port INTEGER NOT NULL,
    dst_ip INET NOT NULL,
    dst_port INTEGER NOT NULL,
    protocol TEXT NOT NULL DEFAULT 'tcp',
    direction TEXT NOT NULL DEFAULT 'outbound',
    pid INTEGER,
    bytes_sent BIGINT DEFAULT 0,
    bytes_received BIGINT DEFAULT 0,
    connected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    disconnected_at TIMESTAMPTZ
);

CREATE INDEX idx_run_network_connections_run ON run_network_connections(run_id);
CREATE INDEX idx_run_network_connections_dst ON run_network_connections(dst_ip, dst_port);
CREATE INDEX idx_run_network_connections_agent ON run_network_connections(agent_id)
    WHERE agent_id IS NOT NULL;

-- ============================================================================
-- Known Binaries (tool/binary inventory with flagging)
-- ============================================================================

CREATE TABLE IF NOT EXISTS known_binaries (
    sha256 TEXT PRIMARY KEY,
    binary_name TEXT NOT NULL,
    binary_path TEXT,
    first_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    run_count BIGINT NOT NULL DEFAULT 1,
    flagged BOOLEAN NOT NULL DEFAULT false,
    flagged_at TIMESTAMPTZ,
    flagged_by UUID REFERENCES users(id) ON DELETE SET NULL,
    flag_reason TEXT,
    block_execution BOOLEAN NOT NULL DEFAULT false,
    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX idx_known_binaries_flagged ON known_binaries(flagged)
    WHERE flagged = true;
CREATE INDEX idx_known_binaries_name ON known_binaries(binary_name);
CREATE INDEX idx_known_binaries_last_seen ON known_binaries(last_seen_at DESC);

-- ============================================================================
-- Add security-related columns to existing agents table
-- ============================================================================

ALTER TABLE agents
    ADD COLUMN IF NOT EXISTS binary_sha256 TEXT,
    ADD COLUMN IF NOT EXISTS renewal_approved BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS last_security_bundle JSONB;

-- ============================================================================
-- Extend join_tokens with description and org scoping
-- ============================================================================

ALTER TABLE join_tokens
    ADD COLUMN IF NOT EXISTS description TEXT,
    ADD COLUMN IF NOT EXISTS org_id UUID REFERENCES organizations(id) ON DELETE CASCADE;

CREATE INDEX IF NOT EXISTS idx_join_tokens_org ON join_tokens(org_id)
    WHERE org_id IS NOT NULL;
