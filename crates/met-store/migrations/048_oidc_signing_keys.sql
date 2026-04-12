-- OIDC workload identity provider signing keys (ADR-017, Phase 2.2).

CREATE TABLE IF NOT EXISTS oidc_signing_keys (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    kid             TEXT NOT NULL UNIQUE,
    private_key_enc BYTEA NOT NULL,
    public_key_jwk  JSONB NOT NULL,
    algorithm       TEXT NOT NULL DEFAULT 'ES256' CHECK (algorithm = 'ES256'),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at      TIMESTAMPTZ NOT NULL,
    revoked_at      TIMESTAMPTZ,
    CONSTRAINT valid_lifetime CHECK (expires_at > created_at)
);

CREATE TABLE IF NOT EXISTS oidc_token_audit (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_run_id  UUID NOT NULL,
    agent_id    UUID NOT NULL,
    audience    TEXT NOT NULL,
    kid         TEXT NOT NULL,
    jti         UUID NOT NULL,
    issued_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at  TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_oidc_token_audit_job ON oidc_token_audit(job_run_id);
CREATE INDEX IF NOT EXISTS idx_oidc_token_audit_jti ON oidc_token_audit(jti);
