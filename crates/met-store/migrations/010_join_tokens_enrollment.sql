-- Join tokens: required description, one-time use (max_uses = 1), consumption linkage

-- Backfill description before NOT NULL
UPDATE join_tokens SET description = '(migrated)' WHERE description IS NULL;

ALTER TABLE join_tokens
    ALTER COLUMN description SET NOT NULL;

-- Normalize max_uses to single-use for existing rows; clamp impossible states
UPDATE join_tokens SET max_uses = 1 WHERE max_uses IS NULL OR max_uses <> 1;
UPDATE join_tokens SET current_uses = 1 WHERE current_uses > 1;

ALTER TABLE join_tokens
    ALTER COLUMN max_uses SET NOT NULL;

ALTER TABLE join_tokens
    ADD COLUMN IF NOT EXISTS consumed_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS consumed_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_join_tokens_consumed_by ON join_tokens(consumed_by_agent_id)
    WHERE consumed_by_agent_id IS NOT NULL;

ALTER TABLE join_tokens
    ADD CONSTRAINT join_tokens_max_uses_one CHECK (max_uses = 1);

COMMENT ON COLUMN join_tokens.consumed_by_agent_id IS 'Agent that first enrolled with this token (set with agent insert).';
