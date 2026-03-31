//! Artifact routes.

use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use met_core::ids::{ArtifactId, RunId};
use serde::Serialize;
use tracing::instrument;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    error::{ApiError, ApiResult},
    extractors::Auth,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/runs/{run_id}/artifacts", get(list_run_artifacts))
        .route("/artifacts/{id}", get(get_artifact))
        .route("/runs/{run_id}/sbom", get(get_run_sbom))
        .route("/runs/{run_id}/attestation", get(get_run_attestation))
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ArtifactRow {
    pub id: Uuid,
    pub run_id: Uuid,
    pub job_run_id: Option<Uuid>,
    pub name: String,
    pub path: String,
    pub size_bytes: i64,
    pub content_type: Option<String>,
    pub sha256: Option<String>,
    pub storage_backend: String,
    pub storage_key: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ArtifactResponse {
    #[schema(value_type = String)]
    pub id: ArtifactId,
    #[schema(value_type = String)]
    pub run_id: RunId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_run_id: Option<String>,
    pub name: String,
    pub path: String,
    pub size_bytes: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    pub storage_backend: String,
    pub download_url: String,
    pub created_at: DateTime<Utc>,
}

impl From<ArtifactRow> for ArtifactResponse {
    fn from(r: ArtifactRow) -> Self {
        let download_url = format!("/api/v1/artifacts/{}/download", r.id);
        Self {
            id: ArtifactId::from_uuid(r.id),
            run_id: RunId::from_uuid(r.run_id),
            job_run_id: r.job_run_id.map(|id| id.to_string()),
            name: r.name,
            path: r.path,
            size_bytes: r.size_bytes,
            content_type: r.content_type,
            sha256: r.sha256,
            storage_backend: r.storage_backend,
            download_url,
            created_at: r.created_at,
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/runs/{run_id}/artifacts",
    params(("run_id" = String, Path, description = "Run ID")),
    responses(
        (status = 200, description = "List of artifacts", body = Vec<ArtifactResponse>),
        (status = 404, description = "Run not found"),
    ),
    tag = "artifacts",
)]
#[instrument(skip(state))]
async fn list_run_artifacts(
    State(state): State<AppState>,
    Auth(_user): Auth,
    Path(run_id): Path<RunId>,
) -> ApiResult<Json<Vec<ArtifactResponse>>> {
    let rows = sqlx::query_as::<_, ArtifactRow>(
        r#"
        SELECT id, run_id, job_run_id, name, path, size_bytes, content_type, sha256, storage_backend, storage_key, created_at
        FROM artifacts
        WHERE run_id = $1
        ORDER BY created_at ASC
        "#,
    )
    .bind(run_id.as_uuid())
    .fetch_all(state.db())
    .await
    .map_err(met_store::StoreError::from)?;

    Ok(Json(rows.into_iter().map(ArtifactResponse::from).collect()))
}

#[utoipa::path(
    get,
    path = "/api/v1/artifacts/{id}",
    params(("id" = String, Path, description = "Artifact ID")),
    responses(
        (status = 200, description = "Artifact details", body = ArtifactResponse),
        (status = 404, description = "Artifact not found"),
    ),
    tag = "artifacts",
)]
#[instrument(skip(state))]
async fn get_artifact(
    State(state): State<AppState>,
    Auth(_user): Auth,
    Path(id): Path<ArtifactId>,
) -> ApiResult<Json<ArtifactResponse>> {
    let row = sqlx::query_as::<_, ArtifactRow>(
        r#"
        SELECT id, run_id, job_run_id, name, path, size_bytes, content_type, sha256, storage_backend, storage_key, created_at
        FROM artifacts
        WHERE id = $1
        "#,
    )
    .bind(id.as_uuid())
    .fetch_optional(state.db())
    .await
    .map_err(met_store::StoreError::from)?
    .ok_or_else(|| ApiError::not_found("artifact not found"))?;

    Ok(Json(ArtifactResponse::from(row)))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SbomResponse {
    #[schema(value_type = String)]
    pub run_id: RunId,
    pub format: String,
    pub status: String,
    pub sbom: Option<serde_json::Value>,
}

#[utoipa::path(
    get,
    path = "/api/v1/runs/{run_id}/sbom",
    params(("run_id" = String, Path, description = "Run ID")),
    responses(
        (status = 200, description = "SBOM for run", body = SbomResponse),
    ),
    tag = "artifacts",
)]
#[instrument(skip(_state))]
async fn get_run_sbom(
    State(_state): State<AppState>,
    Auth(_user): Auth,
    Path(run_id): Path<RunId>,
) -> ApiResult<Json<SbomResponse>> {
    Ok(Json(SbomResponse {
        run_id,
        format: "spdx".to_string(),
        status: "pending".to_string(),
        sbom: None,
    }))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AttestationResponse {
    #[schema(value_type = String)]
    pub run_id: RunId,
    pub format: String,
    pub status: String,
    pub attestation: Option<serde_json::Value>,
}

#[utoipa::path(
    get,
    path = "/api/v1/runs/{run_id}/attestation",
    params(("run_id" = String, Path, description = "Run ID")),
    responses(
        (status = 200, description = "Attestation for run", body = AttestationResponse),
    ),
    tag = "artifacts",
)]
#[instrument(skip(_state))]
async fn get_run_attestation(
    State(_state): State<AppState>,
    Auth(_user): Auth,
    Path(run_id): Path<RunId>,
) -> ApiResult<Json<AttestationResponse>> {
    Ok(Json(AttestationResponse {
        run_id,
        format: "in-toto".to_string(),
        status: "pending".to_string(),
        attestation: None,
    }))
}
