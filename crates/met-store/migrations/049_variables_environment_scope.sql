-- Environment-scoped variables (Pipeline Environments UX plan).
ALTER TABLE variables ADD COLUMN IF NOT EXISTS environment_id UUID REFERENCES environments(id);
