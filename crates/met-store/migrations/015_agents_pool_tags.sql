-- Pool tags from enrollment (which job dispatch pools this agent serves).
ALTER TABLE agents
    ADD COLUMN IF NOT EXISTS pool_tags TEXT[] NOT NULL DEFAULT ARRAY['_default']::text[];

UPDATE agents
SET pool_tags = ARRAY[pool]
WHERE pool IS NOT NULL;

COMMENT ON COLUMN agents.pool_tags IS
    'Join-token pool tags; dispatch targets agents with pool_tag = ANY(pool_tags).';
