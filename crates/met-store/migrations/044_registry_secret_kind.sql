-- Add 'registry' to the builtin_secrets kind check constraint (ADR-015, Phase 1.1).
-- Authenticated OCI image pulls use registry secrets for ephemeral Docker config.

ALTER TABLE builtin_secrets
    DROP CONSTRAINT IF EXISTS builtin_secrets_kind_check;

ALTER TABLE builtin_secrets
    ADD CONSTRAINT builtin_secrets_kind_check
    CHECK (kind IN ('kv', 'ssh_private_key', 'github_app', 'api_key', 'x509_bundle', 'registry'));
