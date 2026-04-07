-- How generic project webhooks authenticate inbound POSTs (SCM rows keep defaults; SCM handlers unchanged).

ALTER TABLE webhook_registrations
    ADD COLUMN IF NOT EXISTS generic_inbound_auth TEXT NOT NULL DEFAULT 'hmac'
        CHECK (generic_inbound_auth IN ('none', 'hmac', 'query'));

ALTER TABLE webhook_registrations
    ADD COLUMN IF NOT EXISTS generic_query_param_name TEXT NULL;
