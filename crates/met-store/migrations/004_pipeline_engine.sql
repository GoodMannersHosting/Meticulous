-- Pipeline Engine Tables Migration
-- This migration adds comprehensive pipeline execution tracking tables.

-- ============================================================================
-- Pipeline Runs (Enhanced for full execution tracking)
-- ============================================================================

-- Add additional columns to runs table for enhanced tracking
ALTER TABLE runs ADD COLUMN IF NOT EXISTS org_id UUID REFERENCES organizations(id);
ALTER TABLE runs ADD COLUMN IF NOT EXISTS trace_id UUID;
ALTER TABLE runs ADD COLUMN IF NOT EXISTS trigger_data JSONB;
ALTER TABLE runs ADD COLUMN IF NOT EXISTS error_message TEXT;

-- Backfill org_id from pipeline -> project -> org
UPDATE runs r
SET org_id = (
    SELECT p.org_id 
    FROM pipelines pl 
    JOIN projects p ON pl.project_id = p.id 
    WHERE pl.id = r.pipeline_id
)
WHERE r.org_id IS NULL;

-- Create index for trace correlation
CREATE INDEX IF NOT EXISTS idx_runs_trace ON runs(trace_id) WHERE trace_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_runs_org ON runs(org_id) WHERE org_id IS NOT NULL;

-- ============================================================================
-- Job Runs (Enhanced)
-- ============================================================================

-- Add additional columns to job_runs for full tracking
ALTER TABLE job_runs ADD COLUMN IF NOT EXISTS job_name TEXT;
ALTER TABLE job_runs ADD COLUMN IF NOT EXISTS exit_code INTEGER;
ALTER TABLE job_runs ADD COLUMN IF NOT EXISTS error_message TEXT;
ALTER TABLE job_runs ADD COLUMN IF NOT EXISTS cache_hit BOOLEAN DEFAULT FALSE;
ALTER TABLE job_runs ADD COLUMN IF NOT EXISTS cache_key TEXT;
ALTER TABLE job_runs ADD COLUMN IF NOT EXISTS outputs JSONB DEFAULT '{}';
ALTER TABLE job_runs ADD COLUMN IF NOT EXISTS metadata JSONB DEFAULT '{}';

-- Create index for cache tracking
CREATE INDEX IF NOT EXISTS idx_job_runs_cache ON job_runs(cache_key) WHERE cache_key IS NOT NULL;

-- ============================================================================
-- Step Runs (Enhanced)  
-- ============================================================================

-- Add additional columns to step_runs
ALTER TABLE step_runs ADD COLUMN IF NOT EXISTS step_name TEXT;
ALTER TABLE step_runs ADD COLUMN IF NOT EXISTS error_message TEXT;
ALTER TABLE step_runs ADD COLUMN IF NOT EXISTS outputs JSONB DEFAULT '{}';

-- ============================================================================
-- Pipeline Run Logs
-- ============================================================================

CREATE TABLE IF NOT EXISTS run_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    job_run_id UUID REFERENCES job_runs(id) ON DELETE CASCADE,
    step_run_id UUID REFERENCES step_runs(id) ON DELETE CASCADE,
    log_level TEXT NOT NULL DEFAULT 'info',
    message TEXT NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    metadata JSONB DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_run_logs_run ON run_logs(run_id, timestamp);
CREATE INDEX IF NOT EXISTS idx_run_logs_job ON run_logs(job_run_id, timestamp) WHERE job_run_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_run_logs_step ON run_logs(step_run_id, timestamp) WHERE step_run_id IS NOT NULL;

-- ============================================================================
-- Cache Entries (Dedicated table for cache management)
-- ============================================================================

CREATE TABLE IF NOT EXISTS cache_entries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    cache_key TEXT NOT NULL,
    storage_path TEXT NOT NULL,
    size_bytes BIGINT NOT NULL DEFAULT 0,
    compression TEXT DEFAULT 'zstd',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_hit_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    hit_count INTEGER NOT NULL DEFAULT 0,
    expires_at TIMESTAMPTZ,
    metadata JSONB DEFAULT '{}',
    UNIQUE(project_id, cache_key)
);

CREATE INDEX IF NOT EXISTS idx_cache_entries_project ON cache_entries(project_id);
CREATE INDEX IF NOT EXISTS idx_cache_entries_lru ON cache_entries(project_id, last_hit_at);
CREATE INDEX IF NOT EXISTS idx_cache_entries_expires ON cache_entries(expires_at) WHERE expires_at IS NOT NULL;

-- ============================================================================
-- Artifacts (Enhanced)
-- ============================================================================

-- Add additional columns to artifacts table
ALTER TABLE artifacts ADD COLUMN IF NOT EXISTS pinned BOOLEAN DEFAULT FALSE;
ALTER TABLE artifacts ADD COLUMN IF NOT EXISTS retention_days INTEGER;
ALTER TABLE artifacts ADD COLUMN IF NOT EXISTS metadata JSONB DEFAULT '{}';

-- ============================================================================
-- Reusable Workflows (Enhanced for semver support)
-- ============================================================================

ALTER TABLE reusable_workflows ADD COLUMN IF NOT EXISTS deprecated BOOLEAN DEFAULT FALSE;
ALTER TABLE reusable_workflows ADD COLUMN IF NOT EXISTS min_version TEXT;
ALTER TABLE reusable_workflows ADD COLUMN IF NOT EXISTS max_version TEXT;
ALTER TABLE reusable_workflows ADD COLUMN IF NOT EXISTS tags TEXT[] DEFAULT '{}';

CREATE INDEX IF NOT EXISTS idx_workflows_tags ON reusable_workflows USING gin(tags);

-- ============================================================================
-- Run Events (For event sourcing and audit)
-- ============================================================================

CREATE TABLE IF NOT EXISTS run_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,
    event_data JSONB NOT NULL DEFAULT '{}',
    actor TEXT,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_run_events_run ON run_events(run_id, timestamp);
CREATE INDEX IF NOT EXISTS idx_run_events_type ON run_events(event_type, timestamp);

-- ============================================================================
-- Job Queue (For tracking dispatched jobs)
-- ============================================================================

CREATE TABLE IF NOT EXISTS job_queue (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_run_id UUID NOT NULL REFERENCES job_runs(id) ON DELETE CASCADE,
    org_id UUID NOT NULL REFERENCES organizations(id),
    pool_selector JSONB NOT NULL DEFAULT '{}',
    priority INTEGER NOT NULL DEFAULT 100,
    dispatched_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    claimed_at TIMESTAMPTZ,
    claimed_by UUID REFERENCES agents(id),
    timeout_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    UNIQUE(job_run_id)
);

CREATE INDEX IF NOT EXISTS idx_job_queue_org ON job_queue(org_id, priority DESC) WHERE claimed_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_job_queue_agent ON job_queue(claimed_by) WHERE claimed_by IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_job_queue_timeout ON job_queue(timeout_at) WHERE timeout_at IS NOT NULL AND claimed_at IS NOT NULL;

-- ============================================================================
-- Artifact Dependencies (For tracking artifact flow between jobs)
-- ============================================================================

CREATE TABLE IF NOT EXISTS artifact_dependencies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    source_job_run_id UUID NOT NULL REFERENCES job_runs(id) ON DELETE CASCADE,
    target_job_run_id UUID NOT NULL REFERENCES job_runs(id) ON DELETE CASCADE,
    artifact_id UUID NOT NULL REFERENCES artifacts(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(run_id, source_job_run_id, target_job_run_id, artifact_id)
);

CREATE INDEX IF NOT EXISTS idx_artifact_deps_run ON artifact_dependencies(run_id);
CREATE INDEX IF NOT EXISTS idx_artifact_deps_target ON artifact_dependencies(target_job_run_id);

-- ============================================================================
-- Functions for run statistics
-- ============================================================================

CREATE OR REPLACE FUNCTION update_cache_hit_at()
RETURNS TRIGGER AS $$
BEGIN
    UPDATE cache_entries
    SET last_hit_at = NOW(), hit_count = hit_count + 1
    WHERE id = NEW.id;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- Views for common queries
-- ============================================================================

CREATE OR REPLACE VIEW run_summary AS
SELECT 
    r.id,
    r.pipeline_id,
    r.org_id,
    r.status,
    r.run_number,
    r.triggered_by,
    r.created_at,
    r.started_at,
    r.finished_at,
    EXTRACT(EPOCH FROM (r.finished_at - r.started_at)) * 1000 AS duration_ms,
    (SELECT COUNT(*) FROM job_runs jr WHERE jr.run_id = r.id) AS total_jobs,
    (SELECT COUNT(*) FROM job_runs jr WHERE jr.run_id = r.id AND jr.status = 'succeeded') AS succeeded_jobs,
    (SELECT COUNT(*) FROM job_runs jr WHERE jr.run_id = r.id AND jr.status = 'failed') AS failed_jobs,
    (SELECT COUNT(*) FROM job_runs jr WHERE jr.run_id = r.id AND jr.status = 'skipped') AS skipped_jobs
FROM runs r;

CREATE OR REPLACE VIEW job_run_summary AS
SELECT 
    jr.id,
    jr.run_id,
    jr.job_id,
    jr.job_name,
    jr.agent_id,
    jr.status,
    jr.attempt,
    jr.cache_hit,
    jr.started_at,
    jr.finished_at,
    EXTRACT(EPOCH FROM (jr.finished_at - jr.started_at)) * 1000 AS duration_ms,
    (SELECT COUNT(*) FROM step_runs sr WHERE sr.job_run_id = jr.id) AS total_steps,
    (SELECT COUNT(*) FROM step_runs sr WHERE sr.job_run_id = jr.id AND sr.status = 'succeeded') AS succeeded_steps,
    (SELECT COUNT(*) FROM step_runs sr WHERE sr.job_run_id = jr.id AND sr.status = 'failed') AS failed_steps
FROM job_runs jr;
