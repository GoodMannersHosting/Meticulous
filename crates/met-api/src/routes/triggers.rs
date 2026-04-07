//! Authenticated CRUD for pipeline triggers (`triggers` table).

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{delete, get, patch, post},
};
use met_core::ids::{PipelineId, TriggerId};
use met_core::models::{CreateTrigger, Trigger, TriggerKind, UpdateTrigger, WebhookConfig};
use met_store::repos::{PipelineRepo, ProjectRepo, TriggerRepo};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use tracing::instrument;
use utoipa::ToSchema;

use crate::{
    error::{ApiError, ApiResult},
    extractors::Auth,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/pipelines/{pipeline_id}/triggers",
            get(list_triggers).post(create_trigger),
        )
        .route(
            "/triggers/{trigger_id}",
            patch(update_trigger).delete(delete_trigger),
        )
}

fn redact_trigger_config(config: &JsonValue) -> JsonValue {
    let mut c = config.clone();
    if let Some(obj) = c.as_object_mut() {
        obj.remove("secret");
    }
    c
}

fn secret_configured(config: &JsonValue) -> bool {
    config
        .get("secret")
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.is_empty())
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TriggerPublicResponse {
    #[schema(value_type = String)]
    pub id: TriggerId,
    #[schema(value_type = String)]
    pub pipeline_id: PipelineId,
    #[schema(value_type = String)]
    pub kind: TriggerKind,
    /// Webhook configuration without `secret`; use `secret_configured` to see if HMAC is required.
    #[schema(value_type = Object)]
    pub config: JsonValue,
    pub secret_configured: bool,
    pub enabled: bool,
    pub description: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Present only on create when `generate_webhook_secret` was true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_secret: Option<String>,
}

impl TriggerPublicResponse {
    fn from_trigger(t: &Trigger, generated_secret: Option<String>) -> Self {
        Self {
            id: t.id,
            pipeline_id: t.pipeline_id,
            kind: t.kind,
            config: redact_trigger_config(&t.config),
            secret_configured: secret_configured(&t.config),
            enabled: t.enabled,
            description: t.description.clone(),
            created_at: t.created_at,
            updated_at: t.updated_at,
            generated_secret,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateTriggerRequest {
    #[schema(value_type = String)]
    pub kind: TriggerKind,
    #[schema(value_type = Object)]
    pub config: JsonValue,
    pub description: Option<String>,
    /// When true and kind is `webhook`, a random secret is generated and returned once in the response.
    #[serde(default)]
    pub generate_webhook_secret: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateTriggerRequest {
    pub enabled: Option<bool>,
    pub description: Option<String>,
    /// Merged into existing JSON config (objects merge recursively).
    #[schema(value_type = Object)]
    pub config_patch: Option<JsonValue>,
}

#[utoipa::path(
    get,
    path = "/api/v1/pipelines/{pipeline_id}/triggers",
    params(("pipeline_id" = String, Path, description = "Pipeline ID")),
    responses(
        (status = 200, description = "Triggers for pipeline", body = Vec<TriggerPublicResponse>),
        (status = 403, description = "Forbidden"),
    ),
    tag = "triggers",
)]
#[instrument(skip(state))]
async fn list_triggers(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(pipeline_id): Path<PipelineId>,
) -> ApiResult<Json<Vec<TriggerPublicResponse>>> {
    let pipeline_repo = PipelineRepo::new(state.db());
    let pipeline = pipeline_repo.get(pipeline_id).await?;
    if !user.can_access_project(pipeline.project_id) {
        return Err(ApiError::forbidden("no access to this project"));
    }
    let project = ProjectRepo::new(state.db())
        .get(pipeline.project_id)
        .await?;
    let rows = TriggerRepo::new(state.db())
        .list_for_pipeline(project.org_id, pipeline_id)
        .await?;
    let out = rows
        .iter()
        .map(|t| TriggerPublicResponse::from_trigger(t, None))
        .collect();
    Ok(Json(out))
}

#[utoipa::path(
    post,
    path = "/api/v1/pipelines/{pipeline_id}/triggers",
    params(("pipeline_id" = String, Path, description = "Pipeline ID")),
    request_body = CreateTriggerRequest,
    responses(
        (status = 200, description = "Trigger created", body = TriggerPublicResponse),
        (status = 400, description = "Bad request"),
    ),
    tag = "triggers",
)]
#[instrument(skip(state, body))]
async fn create_trigger(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(pipeline_id): Path<PipelineId>,
    Json(body): Json<CreateTriggerRequest>,
) -> ApiResult<Json<TriggerPublicResponse>> {
    if body.kind != TriggerKind::Webhook {
        return Err(ApiError::bad_request(
            "only webhook triggers can be created via this API for now",
        ));
    }

    let pipeline_repo = PipelineRepo::new(state.db());
    let pipeline = pipeline_repo.get(pipeline_id).await?;
    if !user.can_access_project(pipeline.project_id) {
        return Err(ApiError::forbidden("no access to this project"));
    }
    let project = ProjectRepo::new(state.db())
        .get(pipeline.project_id)
        .await?;

    let mut config_val = body.config.clone();
    let mut generated = None;
    if body.generate_webhook_secret {
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        let secret = hex::encode(bytes);
        generated = Some(secret.clone());
        if let Some(obj) = config_val.as_object_mut() {
            obj.insert("secret".into(), json!(secret));
        } else {
            config_val = json!({ "secret": secret });
        }
    }

    // Drop sync keys from API-created configs so repo sync does not treat them as managed.
    if let Some(obj) = config_val.as_object_mut() {
        obj.remove("sync_key");
        obj.remove("managed_by");
    }

    let input = CreateTrigger {
        kind: body.kind,
        config: config_val,
        description: body.description,
    };

    let trigger = TriggerRepo::new(state.db())
        .insert(project.org_id, pipeline_id, &input, true)
        .await?;

    Ok(Json(TriggerPublicResponse::from_trigger(
        &trigger,
        generated,
    )))
}

#[utoipa::path(
    patch,
    path = "/api/v1/triggers/{trigger_id}",
    params(("trigger_id" = String, Path, description = "Trigger ID")),
    request_body = UpdateTriggerRequest,
    responses(
        (status = 200, description = "Trigger updated", body = TriggerPublicResponse),
        (status = 404, description = "Not found"),
    ),
    tag = "triggers",
)]
#[instrument(skip(state, body))]
async fn update_trigger(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(trigger_id): Path<TriggerId>,
    Json(body): Json<UpdateTriggerRequest>,
) -> ApiResult<Json<TriggerPublicResponse>> {
    let repo = TriggerRepo::new(state.db());
    let prior = repo.get_for_org(user.org_id, trigger_id).await?;
    let pipeline = PipelineRepo::new(state.db()).get(prior.pipeline_id).await?;
    if !user.can_access_project(pipeline.project_id) {
        return Err(ApiError::forbidden("no access to this project"));
    }

    if let Some(ref p) = body.config_patch {
        if p.get("sync_key").is_some() || p.get("managed_by").is_some() {
            return Err(ApiError::bad_request(
                "sync_key and managed_by cannot be set via API; they are managed by Git sync",
            ));
        }
        let managed: WebhookConfig = serde_json::from_value(prior.config.clone()).unwrap_or_default();
        if managed.managed_by.as_deref() == Some("repo")
            && (p.get("branches").is_some()
                || p.get("paths").is_some()
                || p.get("paths_ignore").is_some()
                || p.get("events").is_some())
        {
            return Err(ApiError::bad_request(
                "declarative webhook filters on repo-managed triggers are owned by pipeline YAML; edit the repo or delete this trigger",
            ));
        }
    }

    let patch = UpdateTrigger {
        enabled: body.enabled,
        description: body.description,
        config_patch: body.config_patch,
    };
    let updated = repo.update(user.org_id, trigger_id, &patch).await?;
    Ok(Json(TriggerPublicResponse::from_trigger(&updated, None)))
}

#[utoipa::path(
    delete,
    path = "/api/v1/triggers/{trigger_id}",
    params(("trigger_id" = String, Path, description = "Trigger ID")),
    responses(
        (status = 200, description = "Trigger deleted"),
        (status = 404, description = "Not found"),
    ),
    tag = "triggers",
)]
#[instrument(skip(state))]
async fn delete_trigger(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(trigger_id): Path<TriggerId>,
) -> ApiResult<()> {
    let repo = TriggerRepo::new(state.db());
    let prior = repo.get_for_org(user.org_id, trigger_id).await?;
    let pipeline = PipelineRepo::new(state.db()).get(prior.pipeline_id).await?;
    if !user.can_access_project(pipeline.project_id) {
        return Err(ApiError::forbidden("no access to this project"));
    }
    let managed: WebhookConfig = serde_json::from_value(prior.config.clone()).unwrap_or_default();
    if managed.managed_by.as_deref() == Some("repo") {
        return Err(ApiError::bad_request(
            "repo-managed triggers are deleted when removed from pipeline YAML or when the pipeline syncs",
        ));
    }
    repo.delete(user.org_id, trigger_id).await?;
    Ok(())
}
