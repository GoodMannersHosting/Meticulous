-- Require password change for bootstrap / default-credential accounts before full API access.
ALTER TABLE users
    ADD COLUMN IF NOT EXISTS password_must_change BOOLEAN NOT NULL DEFAULT false;

COMMENT ON COLUMN users.password_must_change IS
    'When true, the user must change password (POST /auth/change-password) before accessing other authenticated routes.';
