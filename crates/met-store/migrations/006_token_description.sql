-- Add optional description field to API tokens
ALTER TABLE api_tokens ADD COLUMN IF NOT EXISTS description TEXT;
