-- Optional per-pipeline restriction for API tokens (intersects with project_ids).

ALTER TABLE api_tokens ADD COLUMN IF NOT EXISTS pipeline_ids UUID[] DEFAULT NULL;

COMMENT ON COLUMN api_tokens.pipeline_ids IS 'When set, token may only access these pipelines (must belong to allowed projects).';
