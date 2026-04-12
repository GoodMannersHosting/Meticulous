-- Data retention configuration.
--
-- Adds two global platform_settings keys:
--   heartbeat_retention_hours  – how many hours of agent_heartbeats to keep (0 = disabled)
--   run_retention_days         – default days to retain pipeline runs across all projects (0 = disabled)
--
-- Adds a per-project override column so individual projects can have a tighter
-- (or looser) window than the platform default.  NULL means "inherit global".

INSERT INTO platform_settings (key, value, updated_at)
VALUES
    ('heartbeat_retention_hours', '48',  now()),
    ('run_retention_days',        '0',   now())
ON CONFLICT (key) DO NOTHING;

ALTER TABLE projects
    ADD COLUMN IF NOT EXISTS run_retention_days INT
        CHECK (run_retention_days IS NULL OR run_retention_days >= 0);

COMMENT ON COLUMN projects.run_retention_days IS
    'Per-project run retention override in days.  NULL = use the global platform_settings value.  0 = disable retention for this project.';
