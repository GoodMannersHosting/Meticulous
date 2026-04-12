-- Reconcile legacy oidc_signing_keys (005_security: PEM + purpose) with ADR-017 schema (048).
-- If the old shape is present, rename it so CREATE TABLE can install the workload-identity table.
-- Version 052: was briefly a duplicate 049 alongside `049_variables_environment_scope.sql` (invalid).

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'oidc_signing_keys'
          AND column_name = 'purpose'
    ) THEN
        ALTER TABLE oidc_signing_keys RENAME TO oidc_signing_keys_legacy_pre_adr017;
    END IF;
END $$;

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
