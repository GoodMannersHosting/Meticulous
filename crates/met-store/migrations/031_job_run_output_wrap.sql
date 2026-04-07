-- Per-job X25519 secret for `met-output secret` wrapping (public key is sent to agents via dispatch).
ALTER TABLE job_runs
    ADD COLUMN IF NOT EXISTS output_wrap_x25519_secret BYTEA;

COMMENT ON COLUMN job_runs.output_wrap_x25519_secret IS
    'X25519 static secret (32 bytes) for workflow secret outputs; never log. NULL before engine materializes keys.';
