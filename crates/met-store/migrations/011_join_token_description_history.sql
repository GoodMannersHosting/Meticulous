-- Append-only description history for join tokens (audit trail).

CREATE TABLE join_token_description_history (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    join_token_id UUID NOT NULL REFERENCES join_tokens(id) ON DELETE CASCADE,
    description TEXT NOT NULL,
    changed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    changed_by UUID REFERENCES users(id) ON DELETE SET NULL
);

CREATE INDEX idx_join_token_desc_hist_token ON join_token_description_history(join_token_id, changed_at DESC);

-- Backfill: one row per existing token (initial description at creation time).
INSERT INTO join_token_description_history (join_token_id, description, changed_at, changed_by)
SELECT id, description, created_at, created_by FROM join_tokens;
