-- Org-wide secrets may be restricted so they do not appear in project/pipeline
-- `stored:` resolution (platform-only creds, e.g. catalog GitHub App).

ALTER TABLE builtin_secrets
    ADD COLUMN IF NOT EXISTS propagate_to_projects boolean NOT NULL DEFAULT true;

COMMENT ON COLUMN builtin_secrets.propagate_to_projects IS
    'For org-wide rows (project_id NULL): when false, secret is not listed or resolved for pipelines/jobs; still usable for catalog SCM import. Always true for project-scoped rows.';
