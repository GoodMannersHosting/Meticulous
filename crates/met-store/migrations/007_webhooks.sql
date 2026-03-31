CREATE TABLE IF NOT EXISTS webhook_registrations (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id      UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    provider        VARCHAR(50) NOT NULL,
    external_id     VARCHAR(255),
    secret_hash     VARCHAR(255) NOT NULL,
    events          TEXT[] NOT NULL DEFAULT '{}',
    active          BOOLEAN NOT NULL DEFAULT true,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_webhook_registrations_project ON webhook_registrations(project_id);
