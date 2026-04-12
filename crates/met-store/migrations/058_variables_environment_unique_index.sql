-- Include environment_id in variables uniqueness so the same name can exist per environment
-- (project- or pipeline-scoped), matching stored-secrets scoping.

DROP INDEX IF EXISTS idx_variables_scope_name;

CREATE UNIQUE INDEX idx_variables_scope_name
    ON variables (
        org_id,
        COALESCE(project_id, '00000000-0000-0000-0000-000000000000'::uuid),
        COALESCE(pipeline_id, '00000000-0000-0000-0000-000000000000'::uuid),
        COALESCE(environment_id, '00000000-0000-0000-0000-000000000000'::uuid),
        name
    );
