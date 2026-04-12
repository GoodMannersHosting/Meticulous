-- Remote / external secret provider kinds (reference stored in metadata.secret_ref; ciphertext is a placeholder).
ALTER TABLE builtin_secrets DROP CONSTRAINT IF EXISTS builtin_secrets_kind_check;
ALTER TABLE builtin_secrets ADD CONSTRAINT builtin_secrets_kind_check CHECK (
    kind IN (
        'kv',
        'ssh_private_key',
        'github_app',
        'api_key',
        'x509_bundle',
        'registry',
        'aws_sm',
        'vault',
        'gcp_sm',
        'azure_kv',
        'kubernetes'
    )
);
