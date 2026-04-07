-- Optional human-readable label and creator attribution for project webhooks and pipeline triggers.

ALTER TABLE webhook_registrations
    ADD COLUMN IF NOT EXISTS description TEXT,
    ADD COLUMN IF NOT EXISTS created_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL;

ALTER TABLE triggers
    ADD COLUMN IF NOT EXISTS created_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL;
