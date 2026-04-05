-- Pipeline-scoped variables: override project-level values per pipeline.

ALTER TABLE variables
    ADD COLUMN IF NOT EXISTS pipeline_id UUID REFERENCES pipelines(id) ON DELETE CASCADE;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'variables_org_id_project_id_name_key'
    ) THEN
        ALTER TABLE variables DROP CONSTRAINT variables_org_id_project_id_name_key;
    END IF;
END $$;

CREATE UNIQUE INDEX IF NOT EXISTS idx_variables_scope_name
    ON variables (
        org_id,
        COALESCE(project_id, '00000000-0000-0000-0000-000000000000'::uuid),
        COALESCE(pipeline_id, '00000000-0000-0000-0000-000000000000'::uuid),
        name
    );

CREATE INDEX IF NOT EXISTS idx_variables_pipeline
    ON variables (pipeline_id)
    WHERE pipeline_id IS NOT NULL;
