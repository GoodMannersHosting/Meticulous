CREATE TABLE IF NOT EXISTS debug_sessions (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID NOT NULL REFERENCES users(id),
    project_id      UUID REFERENCES projects(id),
    pipeline_id     UUID REFERENCES pipelines(id),
    run_id          UUID REFERENCES runs(id),
    token_hash      VARCHAR(255) NOT NULL,
    proxy_url       VARCHAR(512) NOT NULL DEFAULT '',
    expires_at      TIMESTAMPTZ NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    closed_at       TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_debug_sessions_user ON debug_sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_debug_sessions_project ON debug_sessions(project_id) WHERE project_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS debug_session_secrets (
    session_id      UUID NOT NULL REFERENCES debug_sessions(id) ON DELETE CASCADE,
    secret_name     VARCHAR(255) NOT NULL,
    consumed        BOOLEAN NOT NULL DEFAULT false,
    consumed_at     TIMESTAMPTZ,
    PRIMARY KEY (session_id, secret_name)
);

CREATE TABLE IF NOT EXISTS rate_limit_counters (
    key             VARCHAR(255) NOT NULL,
    window_start    TIMESTAMPTZ NOT NULL,
    count           INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (key, window_start)
);
