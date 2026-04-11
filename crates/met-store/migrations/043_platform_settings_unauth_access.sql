-- Global toggle for unauthenticated access to public resources (ADR-021).
-- Disabled by default; only super_admin can enable.

INSERT INTO platform_settings (key, value, updated_at)
VALUES ('allow_unauthenticated_access', 'false'::jsonb, NOW())
ON CONFLICT (key) DO NOTHING;
