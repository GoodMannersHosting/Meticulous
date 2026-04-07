-- Generic (project-scoped) inbound webhooks: JSON payload → variable mapping (per registration).

ALTER TABLE webhook_registrations
    ADD COLUMN IF NOT EXISTS payload_mapping JSONB NOT NULL DEFAULT '{}';
