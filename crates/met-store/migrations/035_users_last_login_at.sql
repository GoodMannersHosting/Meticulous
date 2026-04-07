-- Track last successful interactive login (password or OAuth) per user.
ALTER TABLE users
    ADD COLUMN IF NOT EXISTS last_login_at TIMESTAMPTZ NULL;
