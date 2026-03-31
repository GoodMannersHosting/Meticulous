-- Log cache (PostgreSQL) + archive metadata (SeaweedFS is source of truth after job completes)

CREATE TABLE IF NOT EXISTS log_cache (
    job_run_id UUID NOT NULL,
    sequence BIGINT NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,
    stream TEXT NOT NULL CHECK (stream IN ('stdout', 'stderr', 'system')),
    content TEXT NOT NULL,
    run_id UUID NOT NULL,
    step_run_id UUID,
    cached_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- NULL = in-flight job logs (kept until archived). Non-null = lazy-loaded cache with TTL.
    expires_at TIMESTAMPTZ,
    PRIMARY KEY (job_run_id, sequence)
);

CREATE INDEX IF NOT EXISTS idx_log_cache_expires
    ON log_cache(expires_at)
    WHERE expires_at IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_log_cache_run
    ON log_cache(run_id);

CREATE INDEX IF NOT EXISTS idx_log_cache_job_run_ts
    ON log_cache(job_run_id, timestamp);

CREATE TABLE IF NOT EXISTS log_archives (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_run_id UUID NOT NULL UNIQUE,
    run_id UUID NOT NULL,
    project_id UUID NOT NULL,
    storage_key TEXT NOT NULL,
    line_count BIGINT NOT NULL,
    size_bytes BIGINT NOT NULL,
    compressed BOOLEAN NOT NULL DEFAULT TRUE,
    archived_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    sha256_checksum TEXT
);

CREATE INDEX IF NOT EXISTS idx_log_archives_run ON log_archives(run_id);
CREATE INDEX IF NOT EXISTS idx_log_archives_project ON log_archives(project_id);

COMMENT ON TABLE log_cache IS 'Hot cache for logs; SeaweedFS holds authoritative copy after job completes.';
COMMENT ON COLUMN log_cache.expires_at IS 'NULL while job is active. Set on lazy reload for 24h eviction.';
