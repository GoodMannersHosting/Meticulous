-- Meticulous Apps: GitHub-Apps-style integrations with asymmetric keys and per-project installations.

CREATE TABLE meticulous_apps (
    id UUID PRIMARY KEY,
    application_id TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    description TEXT,
    created_by UUID NOT NULL REFERENCES users (id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_meticulous_apps_created_by ON meticulous_apps (created_by);

CREATE TABLE meticulous_app_keys (
    id UUID PRIMARY KEY,
    app_id UUID NOT NULL REFERENCES meticulous_apps (id) ON DELETE CASCADE,
    key_id TEXT NOT NULL,
    public_key_pem TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    revoked_at TIMESTAMPTZ,
    CONSTRAINT meticulous_app_keys_app_key UNIQUE (app_id, key_id)
);

CREATE INDEX idx_meticulous_app_keys_lookup ON meticulous_app_keys (app_id) WHERE revoked_at IS NULL;

CREATE TABLE meticulous_app_installations (
    id UUID PRIMARY KEY,
    app_id UUID NOT NULL REFERENCES meticulous_apps (id) ON DELETE CASCADE,
    project_id UUID NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    permissions TEXT[] NOT NULL DEFAULT '{}',
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX idx_meticulous_app_installations_active
    ON meticulous_app_installations (app_id, project_id)
    WHERE revoked_at IS NULL;

CREATE INDEX idx_meticulous_app_installations_project ON meticulous_app_installations (project_id) WHERE revoked_at IS NULL;

COMMENT ON TABLE meticulous_apps IS 'Registered Meticulous App integration (machine identity).';
COMMENT ON COLUMN meticulous_apps.application_id IS 'Stable public application identifier (e.g. mapp_<uuid>).';
COMMENT ON TABLE meticulous_app_keys IS 'Public keys for validating app-issued JWTs; private key shown once at creation.';
COMMENT ON TABLE meticulous_app_installations IS 'Binds an app to a project with permission allowlist.';
