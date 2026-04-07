-- ADR-005 deduplication + ADR-013 project webhook → pipeline routing

CREATE TABLE IF NOT EXISTS webhook_deliveries (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider         TEXT NOT NULL CHECK (provider IN ('github', 'gitlab', 'bitbucket', 'generic')),
    delivery_id      TEXT NOT NULL,
    registration_id  UUID NOT NULL REFERENCES webhook_registrations(id) ON DELETE CASCADE,
    received_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    run_ids          UUID[] NOT NULL DEFAULT '{}',
    UNIQUE (provider, delivery_id)
);

CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_received_at ON webhook_deliveries (received_at);
CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_registration ON webhook_deliveries (registration_id);

CREATE TABLE IF NOT EXISTS webhook_registration_targets (
    id                        UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    webhook_registration_id   UUID NOT NULL REFERENCES webhook_registrations(id) ON DELETE CASCADE,
    pipeline_id               UUID NOT NULL REFERENCES pipelines(id) ON DELETE CASCADE,
    enabled                   BOOLEAN NOT NULL DEFAULT true,
    filter_config             JSONB NOT NULL DEFAULT '{}',
    created_at                TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at                TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (webhook_registration_id, pipeline_id)
);

CREATE INDEX IF NOT EXISTS idx_webhook_registration_targets_registration
    ON webhook_registration_targets (webhook_registration_id);

CREATE TRIGGER webhook_registration_targets_updated_at
    BEFORE UPDATE ON webhook_registration_targets
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
