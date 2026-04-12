//! Admin Platform Health aggregate (storage, NATS, engine status).

use axum::{
    Json, Router,
    extract::{Query, State},
    routing::get,
};
use met_objstore::{ObjectStore, ObjectStoreUsageEstimate, estimate_prefix_size};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use utoipa::ToSchema;

use crate::{
    error::{ApiError, ApiResult},
    extractors::Auth,
    routes::admin::require_admin,
    state::AppState,
    VERSION,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/ops/platform-health", get(platform_health))
}

#[derive(Debug, Deserialize)]
pub struct PlatformHealthQuery {
    /// Expensive: paginate object listings to estimate org-prefix usage (capped).
    #[serde(default)]
    pub deep_object_store: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PlatformHealthResponse {
    pub api_version: String,
    pub engine_initialized: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine_init_error: Option<String>,
    pub postgres: PostgresHealthSection,
    pub org_artifacts: OrgArtifactsSection,
    pub object_storage: ObjectStorageHealthSection,
    pub nats_jetstream: NatsJetStreamHealthSection,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PostgresHealthSection {
    pub database_bytes: i64,
    pub top_relations: Vec<RelationSizeResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RelationSizeResponse {
    pub schema: String,
    pub name: String,
    pub total_bytes: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OrgArtifactsSection {
    pub total_bytes: i64,
    pub artifact_count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ObjectStorageHealthSection {
    pub endpoint_display: String,
    pub bucket: String,
    pub path_style: bool,
    pub client_initialized: bool,
    pub reachable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reachability_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deep_scan: Option<ObjectStorageDeepScanSection>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ObjectStorageDeepScanSection {
    pub prefix: String,
    pub bytes_summed: u64,
    pub objects_scanned: u64,
    pub list_pages: u32,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct NatsJetStreamHealthSection {
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
    pub streams: Vec<JetStreamStreamRow>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct JetStreamStreamRow {
    pub name: String,
    pub messages: u64,
    pub bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn endpoint_display(endpoint: &str) -> String {
    if endpoint.is_empty() {
        return "default (AWS)".to_string();
    }
    url::Url::parse(endpoint)
        .ok()
        .and_then(|u| u.host_str().map(std::string::ToString::to_string))
        .unwrap_or_else(|| endpoint.to_string())
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/ops/platform-health",
    params(
        ("deep_object_store" = Option<bool>, Query, description = "Run capped object listing for org-prefix usage estimate"),
    ),
    responses(
        (status = 200, description = "Platform health snapshot", body = PlatformHealthResponse),
        (status = 403, description = "Not admin"),
    ),
    tag = "admin",
)]
#[instrument(skip(state))]
pub async fn platform_health(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Query(q): Query<PlatformHealthQuery>,
) -> ApiResult<Json<PlatformHealthResponse>> {
    require_admin(&admin)?;

    let (database_bytes, top) =
        met_store::repos::database_disk_overview(state.db()).await.map_err(|e| {
            ApiError::internal(format!("postgres disk overview failed: {e}"))
        })?;

    let top_relations: Vec<RelationSizeResponse> = top
        .into_iter()
        .map(|r| RelationSizeResponse {
            schema: r.schema,
            name: r.name,
            total_bytes: r.total_bytes,
        })
        .collect();

    let art = met_store::repos::org_artifact_storage_totals(state.db(), admin.org_id)
        .await
        .map_err(|e| ApiError::internal(format!("artifact totals failed: {e}")))?;

    let (reachable, reachability_error) = if let Some(store) = state.object_store.as_ref() {
        match store.head_bucket().await {
            Ok(()) => (true, None),
            Err(e) => (false, Some(e.to_string())),
        }
    } else {
        (
            false,
            Some("object store client not initialized".to_string()),
        )
    };

    let deep_scan = if q.deep_object_store {
        let prefix = format!("orgs/{}/", admin.org_id.as_uuid());
        if let Some(store) = state.object_store.as_ref() {
            let (max_objects, max_pages) = ObjectStoreUsageEstimate::sanitize_caps(10_000, 50);
            match estimate_prefix_size(
                store.as_ref() as &(dyn ObjectStore + Send + Sync),
                &prefix,
                max_objects,
                max_pages,
                500,
            )
            .await
            {
                Ok(est) => Some(ObjectStorageDeepScanSection {
                    prefix,
                    bytes_summed: est.bytes_summed,
                    objects_scanned: est.objects_scanned,
                    list_pages: est.list_pages,
                    truncated: est.truncated,
                    error: None,
                }),
                Err(e) => Some(ObjectStorageDeepScanSection {
                    prefix,
                    bytes_summed: 0,
                    objects_scanned: 0,
                    list_pages: 0,
                    truncated: false,
                    error: Some(e.to_string()),
                }),
            }
        } else {
            Some(ObjectStorageDeepScanSection {
                prefix,
                bytes_summed: 0,
                objects_scanned: 0,
                list_pages: 0,
                truncated: false,
                error: Some("object store client not initialized".to_string()),
            })
        }
    } else {
        None
    };

    let nats_jetstream = if let Some(nats) = state.nats_ops.as_ref() {
        let snaps = nats.jetstream_streams_summary().await;
        let streams: Vec<JetStreamStreamRow> = snaps
            .into_iter()
            .map(|s| JetStreamStreamRow {
                name: s.name,
                messages: s.messages,
                bytes: s.bytes,
                error: s.error,
            })
            .collect();
        NatsJetStreamHealthSection {
            available: true,
            unavailable_reason: None,
            streams,
        }
    } else {
        NatsJetStreamHealthSection {
            available: false,
            unavailable_reason: Some("NATS is not connected".to_string()),
            streams: vec![],
        }
    };

    Ok(Json(PlatformHealthResponse {
        api_version: VERSION.to_string(),
        engine_initialized: state.engine.is_some(),
        engine_init_error: state.engine_init_error.clone(),
        postgres: PostgresHealthSection {
            database_bytes,
            top_relations,
        },
        org_artifacts: OrgArtifactsSection {
            total_bytes: art.total_bytes,
            artifact_count: art.artifact_count,
        },
        object_storage: ObjectStorageHealthSection {
            endpoint_display: endpoint_display(&state.object_storage.endpoint),
            bucket: state.object_storage.bucket.clone(),
            path_style: state.object_storage.path_style,
            client_initialized: state.object_store.is_some(),
            reachable,
            reachability_error,
            deep_scan,
        },
        nats_jetstream,
    }))
}
