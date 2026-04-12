//! Platform / database health metrics for admin dashboards.

use met_core::ids::OrganizationId;
use sqlx::PgPool;

use crate::error::Result;

/// One relation ranked by `pg_total_relation_size`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RelationSizeRow {
    pub schema: String,
    pub name: String,
    pub total_bytes: i64,
}

/// Artifact byte totals recorded in Postgres for an organization.
#[derive(Debug, Clone, Default)]
pub struct OrgArtifactStorageTotals {
    pub total_bytes: i64,
    pub artifact_count: i64,
}

/// Load database size and largest user tables (physical size includes indexes, TOAST).
pub async fn database_disk_overview(pool: &PgPool) -> Result<(i64, Vec<RelationSizeRow>)> {
    let row = sqlx::query_as::<_, (i64,)>(
        "SELECT pg_database_size(current_database())::bigint AS database_bytes",
    )
    .fetch_one(pool)
    .await?;
    let database_bytes = row.0;

    let top = sqlx::query_as::<_, RelationSizeRow>(
        r#"
        SELECT
            n.nspname AS schema,
            c.relname AS name,
            pg_total_relation_size(c.oid)::bigint AS total_bytes
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE c.relkind IN ('r', 'm')
          AND n.nspname NOT IN ('pg_catalog', 'information_schema')
        ORDER BY total_bytes DESC
        LIMIT 15
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok((database_bytes, top))
}

/// Sum of `artifacts.size_bytes` for rows belonging to `org_id` (via projects → pipelines → runs).
pub async fn org_artifact_storage_totals(
    pool: &PgPool,
    org_id: OrganizationId,
) -> Result<OrgArtifactStorageTotals> {
    let org_u = org_id.as_uuid();
    let row = sqlx::query_as::<_, (Option<i64>, i64)>(
        r#"
        SELECT
            COALESCE(SUM(a.size_bytes), 0)::bigint AS total_bytes,
            COUNT(*)::bigint AS artifact_count
        FROM artifacts a
        INNER JOIN runs r ON r.id = a.run_id
        INNER JOIN pipelines p ON p.id = r.pipeline_id
        INNER JOIN projects pr ON pr.id = p.project_id
        WHERE pr.org_id = $1 AND pr.deleted_at IS NULL
        "#,
    )
    .bind(org_u)
    .fetch_one(pool)
    .await?;

    Ok(OrgArtifactStorageTotals {
        total_bytes: row.0.unwrap_or(0),
        artifact_count: row.1,
    })
}
