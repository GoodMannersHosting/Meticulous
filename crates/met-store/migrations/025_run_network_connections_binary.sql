-- Associate observed network flows with the executable (best-effort via /proc correlation).
ALTER TABLE run_network_connections
    ADD COLUMN IF NOT EXISTS binary_path TEXT,
    ADD COLUMN IF NOT EXISTS binary_sha256 TEXT;
