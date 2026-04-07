-- Per-step linkage for executed binaries; syscall audit stream; telemetry audit events.

ALTER TABLE run_binary_executions
    ADD COLUMN IF NOT EXISTS step_run_id UUID REFERENCES step_runs(id) ON DELETE SET NULL;

ALTER TABLE run_binary_executions
    ADD COLUMN IF NOT EXISTS step_sequence INTEGER;

CREATE TABLE IF NOT EXISTS run_syscall_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    job_run_id UUID REFERENCES job_runs(id) ON DELETE SET NULL,
    step_run_id UUID REFERENCES step_runs(id) ON DELETE SET NULL,
    agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    syscall_nr INTEGER NOT NULL,
    syscall_name TEXT NOT NULL,
    outcome TEXT NOT NULL,
    return_code BIGINT,
    pid INTEGER,
    tid INTEGER,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_run_syscall_events_run ON run_syscall_events(run_id);
CREATE INDEX IF NOT EXISTS idx_run_syscall_events_job ON run_syscall_events(job_run_id)
    WHERE job_run_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS run_runtime_script_artifacts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    job_run_id UUID NOT NULL REFERENCES job_runs(id) ON DELETE CASCADE,
    step_run_id UUID REFERENCES step_runs(id) ON DELETE SET NULL,
    agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    sha256_hex TEXT NOT NULL,
    byte_length BIGINT NOT NULL,
    truncated BOOLEAN NOT NULL DEFAULT false,
    object_key TEXT,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_run_runtime_scripts_run ON run_runtime_script_artifacts(run_id);
