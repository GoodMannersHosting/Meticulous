-- Git-backed pipeline metadata (GitHub App + repo pointer).
ALTER TABLE pipelines ADD COLUMN scm_provider TEXT;
ALTER TABLE pipelines ADD COLUMN scm_repository TEXT;
ALTER TABLE pipelines ADD COLUMN scm_ref TEXT;
ALTER TABLE pipelines ADD COLUMN scm_path TEXT;
ALTER TABLE pipelines ADD COLUMN scm_credentials_secret_path TEXT;
ALTER TABLE pipelines ADD COLUMN scm_revision TEXT;
