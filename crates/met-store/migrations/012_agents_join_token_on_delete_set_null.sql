-- Allow deleting join_tokens rows while keeping agents (clear enrollment link).
ALTER TABLE agents DROP CONSTRAINT IF EXISTS agents_join_token_id_fkey;

ALTER TABLE agents
    ADD CONSTRAINT agents_join_token_id_fkey
    FOREIGN KEY (join_token_id) REFERENCES join_tokens(id) ON DELETE SET NULL;
