//! Webhook ingestion routes for SCM events.

use axum::{
    Json, Router,
    body::Bytes,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, Uri},
    routing::{delete, get, patch, post},
};
use met_core::ids::{OrganizationId, PipelineId, ProjectId, TriggerId, UserId};
use met_core::models::{TriggerKind, WEBHOOK_MAX_BODY_BYTES, WebhookConfig};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use tracing::{debug, info, instrument};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    error::{ApiError, ApiResult},
    extractors::{Auth, CurrentUser},
    pipeline_execution,
    state::AppState,
};
use met_store::repos::{
    CreateWebhookTarget, PipelineRepo, ProjectRepo, UpdateWebhookTarget, WebhookDeliveryClaim,
    WebhookRegistrationContext, WebhookRegistrationSummary, WebhookRegistrationTarget, WebhookRepo,
    get_trigger_for_webhook_dispatch,
};

/// Prefer proxy headers (when present); otherwise use the direct TCP peer address.
fn webhook_client_ip(headers: &HeaderMap, connect: &SocketAddr) -> String {
    const XFF: &str = "x-forwarded-for";
    const XREAL: &str = "x-real-ip";
    if let Some(raw) = headers.get(XFF).and_then(|v| v.to_str().ok()) {
        if let Some(first) = raw
            .split(',')
            .next()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return first.to_string();
        }
    }
    if let Some(raw) = headers.get(XREAL).and_then(|v| v.to_str().ok()) {
        let ip = raw.trim();
        if !ip.is_empty() {
            return ip.to_string();
        }
    }
    connect.ip().to_string()
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/webhooks/{org_id}/{trigger_id}", post(handle_webhook))
        .route(
            "/webhooks/github/{org_id}/{trigger_id}",
            post(handle_github_webhook),
        )
        .route(
            "/webhooks/gitlab/{org_id}/{trigger_id}",
            post(handle_gitlab_webhook),
        )
        .route(
            "/webhooks/bitbucket/{org_id}/{trigger_id}",
            post(handle_bitbucket_webhook),
        )
        .route("/projects/{project_id}/scm/setup", post(setup_scm_webhook))
        .route(
            "/projects/{project_id}/webhooks",
            get(list_project_webhooks),
        )
        .route(
            "/projects/{project_id}/webhooks/{registration_id}/rotate-inbound-secret",
            post(rotate_project_webhook_inbound_secret),
        )
        .route(
            "/projects/{project_id}/webhooks/{registration_id}/clear-inbound-secret",
            post(clear_project_webhook_inbound_secret),
        )
        .route(
            "/projects/{project_id}/webhooks/{registration_id}",
            patch(patch_project_webhook),
        )
        .route(
            "/projects/{project_id}/webhooks/{registration_id}/targets",
            get(list_webhook_targets).post(create_webhook_target),
        )
        .route(
            "/projects/{project_id}/webhooks/{registration_id}/targets/{target_id}",
            patch(update_webhook_target).delete(delete_webhook_target),
        )
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WebhookResponse {
    pub accepted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default)]
    pub run_ids: Vec<String>,
    #[serde(default)]
    pub duplicate: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub targets_matched: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_errors: Vec<String>,
    pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WebhookTargetResponse {
    #[schema(value_type = String)]
    pub id: Uuid,
    #[schema(value_type = String)]
    pub pipeline_id: PipelineId,
    pub enabled: bool,
    pub filter_config: serde_json::Value,
}

impl From<WebhookRegistrationTarget> for WebhookTargetResponse {
    fn from(t: WebhookRegistrationTarget) -> Self {
        Self {
            id: t.id,
            pipeline_id: t.pipeline_id,
            enabled: t.enabled,
            filter_config: t.filter_config,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateWebhookTargetRequest {
    #[schema(value_type = String)]
    pub pipeline_id: PipelineId,
    #[serde(default = "default_target_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub filter_config: serde_json::Value,
}

fn default_target_enabled() -> bool {
    true
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateWebhookTargetRequest {
    pub enabled: Option<bool>,
    pub filter_config: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PatchProjectWebhookRequest {
    /// Omit to leave unchanged. Empty string clears the description.
    #[serde(default)]
    pub description: Option<String>,
    /// When set, replaces pipeline targets with this exact list (pipelines must belong to the project).
    #[serde(default)]
    #[schema(value_type = Option<Vec<String>>)]
    pub target_pipeline_ids: Option<Vec<PipelineId>>,
    /// For `provider: generic` only. When enabling HMAC or query auth from `none`, a new signing secret is generated and returned once.
    #[serde(default)]
    pub generic_inbound_auth: Option<String>,
    /// For generic `query` auth: parameter name (letters, digits, `-`, `_`; must start with a letter). Ignored when effective auth is not `query`.
    #[serde(default)]
    pub generic_query_param_name: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RotateInboundSecretResponse {
    /// Value to configure in your caller (HMAC key material / query value), same as at create time. Shown once.
    pub signing_secret: String,
}

/// Matching [`setup_scm_webhook`]: store and reveal SHA-256 hex of a random UUID string.
fn generate_inbound_secret_hash() -> String {
    let secret = uuid::Uuid::new_v4().to_string();
    format!("{:x}", Sha256::digest(secret.as_bytes()))
}

async fn require_project_in_user_org(
    pool: &sqlx::PgPool,
    user: &CurrentUser,
    project_id: ProjectId,
) -> ApiResult<()> {
    let project = ProjectRepo::new(pool).get(project_id).await?;
    if project.org_id != user.org_id {
        return Err(ApiError::not_found("Project not found"));
    }
    if !user.can_access_project(project_id) {
        return Err(ApiError::forbidden(
            "You do not have access to this project",
        ));
    }
    Ok(())
}

async fn load_inbound_registration(
    state: &AppState,
    path_org_id: OrganizationId,
    registration_id: TriggerId,
    expected_provider: &str,
) -> ApiResult<WebhookRegistrationContext> {
    let repo = WebhookRepo::new(state.db());
    let Some(ctx) = repo.get_registration_context(registration_id).await? else {
        return Err(ApiError::not_found("Webhook not found"));
    };
    if ctx.org_id != path_org_id {
        return Err(ApiError::not_found("Webhook not found"));
    }
    if ctx.provider.to_lowercase() != expected_provider {
        return Err(ApiError::not_found("Webhook not found"));
    }
    Ok(ctx)
}

fn scm_webhook_response_duplicate(run_ids: Vec<Uuid>) -> WebhookResponse {
    WebhookResponse {
        accepted: true,
        run_id: run_ids.first().map(|u| u.to_string()),
        run_ids: run_ids.iter().map(Uuid::to_string).collect(),
        duplicate: true,
        targets_matched: Some(0),
        target_errors: vec![],
        message: "Duplicate delivery; returning existing run ids".to_string(),
    }
}

fn scm_webhook_response_dispatched(
    run_ids: Vec<Uuid>,
    targets_matched: usize,
    target_errors: Vec<String>,
    fallback_message: &str,
) -> WebhookResponse {
    let msg = if run_ids.is_empty() && !target_errors.is_empty() {
        target_errors.join("; ")
    } else if run_ids.is_empty() {
        fallback_message.to_string()
    } else {
        format!("Enqueued {} run(s)", run_ids.len())
    };
    WebhookResponse {
        accepted: true,
        run_id: run_ids.first().map(|u| u.to_string()),
        run_ids: run_ids.iter().map(Uuid::to_string).collect(),
        duplicate: false,
        targets_matched: Some(targets_matched),
        target_errors,
        message: msg,
    }
}

fn registration_event_allowed(ctx: &WebhookRegistrationContext, event_type: &str) -> bool {
    if ctx.provider.eq_ignore_ascii_case("generic") {
        ctx.events.is_empty() || ctx.events.iter().any(|e| e == event_type)
    } else {
        ctx.events.iter().any(|e| e == event_type)
    }
}

/// ADR-013: after signature verify — dedupe, fan out to target pipelines via [`pipeline_execution`].
async fn dispatch_registered_webhook_fanout(
    state: &AppState,
    ctx: &WebhookRegistrationContext,
    provider: &str,
    delivery_id: &str,
    event_type: &str,
    branch: Option<&str>,
    commit_sha: Option<&str>,
    trigger_data: serde_json::Value,
    log_label: &str,
    webhook_remote_addr: Option<String>,
    vars_base: HashMap<String, String>,
) -> ApiResult<WebhookResponse> {
    let registration_tid = TriggerId::from_uuid(ctx.registration_id);
    let hook_repo = WebhookRepo::new(state.db());
    let claim = hook_repo
        .claim_webhook_delivery(provider, delivery_id, registration_tid)
        .await?;

    if let WebhookDeliveryClaim::Duplicate { run_ids } = claim {
        return Ok(scm_webhook_response_duplicate(run_ids));
    }

    let event_ok = registration_event_allowed(ctx, event_type);
    let mut target_errors: Vec<String> = Vec::new();
    let mut run_ids: Vec<Uuid> = Vec::new();
    let mut matched: usize = 0;

    let triggered_by = format!("{provider}:webhook:{delivery_id}");

    if !event_ok {
        target_errors.push("event type ignored by registration filters".to_string());
        hook_repo
            .set_delivery_run_ids(provider, delivery_id, &[])
            .await?;
        return Ok(scm_webhook_response_dispatched(
            run_ids,
            matched,
            target_errors,
            log_label,
        ));
    }

    let targets = hook_repo.list_targets(registration_tid).await?;
    let had_targets = !targets.is_empty();
    if targets.is_empty() {
        target_errors.push("no pipeline targets configured for this webhook".to_string());
        hook_repo
            .set_delivery_run_ids(provider, delivery_id, &[])
            .await?;
        return Ok(scm_webhook_response_dispatched(
            run_ids,
            matched,
            target_errors,
            &format!("{} event accepted", log_label),
        ));
    }

    let pipeline_repo = PipelineRepo::new(state.db());
    for t in targets {
        if !t.enabled {
            continue;
        }
        if !WebhookRepo::target_event_allows(&t.filter_config, event_type) {
            continue;
        }
        if let Some(b) = branch {
            if !WebhookRepo::target_branch_allows(&t.filter_config, b) {
                continue;
            }
        } else if WebhookRepo::target_requires_branch(&t.filter_config) {
            continue;
        }

        let pipeline = match pipeline_repo.get(t.pipeline_id).await {
            Ok(p) => p,
            Err(_) => {
                target_errors.push(format!("pipeline {} not found", t.pipeline_id));
                continue;
            }
        };
        if pipeline.project_id != ctx.project_id {
            target_errors.push(format!(
                "pipeline {} is not in the webhook project",
                t.pipeline_id
            ));
            continue;
        }
        if !pipeline.enabled {
            target_errors.push(format!("pipeline {} is disabled", t.pipeline_id));
            continue;
        }

        matched += 1;
        match pipeline_execution::dispatch_pipeline_run(
            state,
            &pipeline,
            ctx.org_id,
            commit_sha,
            branch,
            None,
            &triggered_by,
            "Webhook",
            Some(vars_base.clone()),
            webhook_remote_addr.clone(),
        )
        .await
        {
            Ok(run) => run_ids.push(run.id.as_uuid()),
            Err(e) => {
                target_errors.push(format!("pipeline {}: {e}", t.pipeline_id));
            }
        }
    }

    if event_ok && had_targets && run_ids.is_empty() && target_errors.is_empty() {
        target_errors.push("no targets matched branch or event filters".to_string());
    }

    hook_repo
        .set_delivery_run_ids(provider, delivery_id, &run_ids)
        .await?;

    info!(
        registration_id = %ctx.registration_id,
        provider = %provider,
        event = %event_type,
        ?branch,
        trigger_data = ?trigger_data,
        "webhook dispatch completed for {}",
        log_label
    );

    Ok(scm_webhook_response_dispatched(
        run_ids,
        matched,
        target_errors,
        &format!("{} event accepted", log_label),
    ))
}

async fn dispatch_scm_registration_webhook(
    state: &AppState,
    ctx: &WebhookRegistrationContext,
    provider: &str,
    delivery_id: &str,
    event_type: &str,
    branch: Option<&str>,
    commit_sha: Option<&str>,
    trigger_data: serde_json::Value,
    log_label: &str,
    webhook_remote_addr: Option<String>,
) -> ApiResult<WebhookResponse> {
    let mut vars_base: HashMap<String, String> = HashMap::new();
    if let Some(b) = branch {
        if !b.is_empty() {
            vars_base.insert("webhook_branch".into(), b.to_string());
        }
    }
    if let Some(c) = commit_sha {
        if !c.is_empty() {
            vars_base.insert("webhook_commit".into(), c.to_string());
        }
    }
    vars_base.insert("webhook_event".into(), event_type.to_string());

    dispatch_registered_webhook_fanout(
        state,
        ctx,
        provider,
        delivery_id,
        event_type,
        branch,
        commit_sha,
        trigger_data,
        log_label,
        webhook_remote_addr,
        vars_base,
    )
    .await
}

fn github_delivery_id(headers: &HeaderMap, body: &[u8]) -> String {
    if let Some(v) = headers
        .get(GITHUB_DELIVERY_HEADER)
        .and_then(|v| v.to_str().ok())
    {
        return v.to_string();
    }
    format!("github-synthetic:{:x}", Sha256::digest(body))
}

fn gitlab_synthetic_delivery_id(secret: &str, body: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(secret.as_bytes());
    h.update(body);
    format!("gitlab:{:x}", h.finalize())
}

fn bitbucket_delivery_id(body: &[u8]) -> String {
    format!("bitbucket:{:x}", Sha256::digest(body))
}

#[derive(Debug, Deserialize)]
pub struct GenericWebhookPayload {
    pub event: Option<String>,
    pub branch: Option<String>,
    pub commit: Option<String>,
    pub ref_name: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/webhooks/{org_id}/{trigger_id}",
    params(
        ("org_id" = String, Path, description = "Organization ID"),
        ("trigger_id" = String, Path, description = "Trigger ID"),
    ),
    responses(
        (status = 200, description = "Webhook accepted", body = WebhookResponse),
        (status = 400, description = "Bad request"),
        (status = 403, description = "Invalid or missing HMAC signature"),
        (status = 404, description = "Unknown trigger or organization mismatch"),
    ),
    tag = "webhooks",
)]
#[instrument(skip(state, headers, body, uri))]
async fn handle_webhook(
    State(state): State<AppState>,
    Path((org_id, trigger_id)): Path<(OrganizationId, TriggerId)>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<WebhookResponse>> {
    debug!(
        org_id = %org_id,
        trigger_id = %trigger_id,
        body_len = body.len(),
        "received generic webhook"
    );

    if body.len() > WEBHOOK_MAX_BODY_BYTES {
        return Err(ApiError::bad_request(format!(
            "webhook body exceeds maximum size ({} bytes)",
            WEBHOOK_MAX_BODY_BYTES
        )));
    }

    let client_ip = webhook_client_ip(&headers, &addr);

    let pipeline_trigger =
        match get_trigger_for_webhook_dispatch(state.db(), org_id, trigger_id).await {
            Ok(t) => Some(t),
            Err(e) if e.is_not_found() => None,
            Err(e) => return Err(e.into()),
        };

    if let Some(trigger) = pipeline_trigger {
        if trigger.kind != TriggerKind::Webhook {
            return Err(ApiError::not_found("trigger"));
        }
        if !trigger.enabled {
            return Err(ApiError::bad_request("trigger is disabled"));
        }

        let cfg: WebhookConfig = serde_json::from_value(trigger.config.clone())
            .map_err(|_| ApiError::bad_request("trigger has invalid webhook configuration JSON"))?;

        verify_pipeline_trigger_inbound_auth(&cfg, &uri, &headers, &body)?;

        let mut vars = cfg
            .map_payload_to_variables(&body)
            .map_err(|e| ApiError::bad_request(e.to_string()))?;

        if let Ok(payload) = serde_json::from_slice::<GenericWebhookPayload>(&body) {
            if let Some(b) = payload.branch.filter(|s| !s.is_empty()) {
                vars.entry("webhook_branch".into()).or_insert(b);
            }
            if let Some(c) = payload.commit.filter(|s| !s.is_empty()) {
                vars.entry("webhook_commit".into()).or_insert(c);
            }
            if let Some(r) = payload.ref_name.filter(|s| !s.is_empty()) {
                vars.entry("webhook_ref".into()).or_insert(r);
            }
        }

        let pipeline_repo = PipelineRepo::new(state.db());
        let pipeline = pipeline_repo.get(trigger.pipeline_id).await?;

        let run = pipeline_execution::dispatch_pipeline_run(
            &state,
            &pipeline,
            org_id,
            None,
            None,
            Some(trigger_id),
            "Webhook",
            "Webhook",
            Some(vars),
            Some(client_ip.clone()),
        )
        .await?;

        info!(run_id = %run.id, "webhook started pipeline run");

        return Ok(Json(WebhookResponse {
            accepted: true,
            run_id: Some(run.id.to_string()),
            run_ids: vec![run.id.to_string()],
            duplicate: false,
            targets_matched: Some(1),
            target_errors: vec![],
            message: "Webhook accepted; pipeline run created".to_string(),
        }));
    }

    // Project-level `generic` registration: same URL shape, fans out via `webhook_registration_targets`.
    let hook_repo = WebhookRepo::new(state.db());
    let Some(ctx) = hook_repo.get_registration_context(trigger_id).await? else {
        return Err(ApiError::not_found("trigger"));
    };
    if ctx.org_id != org_id {
        return Err(ApiError::not_found("trigger"));
    }
    if !ctx.provider.eq_ignore_ascii_case("generic") {
        return Err(ApiError::not_found("trigger"));
    }

    verify_generic_inbound_auth(&ctx, &uri, &headers, &body)?;

    let cfg: WebhookConfig =
        serde_json::from_value(ctx.payload_mapping.clone()).unwrap_or_default();

    let mut vars = cfg
        .map_payload_to_variables(&body)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;

    let mut event_type = String::from("webhook");
    let mut branch: Option<String> = None;
    let mut commit_sha: Option<String> = None;

    if let Ok(payload) = serde_json::from_slice::<GenericWebhookPayload>(&body) {
        if let Some(ev) = payload.event.filter(|s| !s.is_empty()) {
            event_type = ev;
        }
        if let Some(b) = payload.branch.filter(|s| !s.is_empty()) {
            branch = Some(b.clone());
            vars.entry("webhook_branch".into()).or_insert(b);
        }
        if let Some(c) = payload.commit.filter(|s| !s.is_empty()) {
            commit_sha = Some(c.clone());
            vars.entry("webhook_commit".into()).or_insert(c);
        }
        if let Some(r) = payload.ref_name.filter(|s| !s.is_empty()) {
            vars.entry("webhook_ref".into()).or_insert(r);
        }
    }

    vars.entry("webhook_event".into())
        .or_insert(event_type.clone());

    let delivery_id = github_delivery_id(&headers, &body);
    let trigger_data = serde_json::json!({
        "provider": "generic",
        "registration_id": ctx.registration_id,
        "delivery_id": delivery_id,
    });

    let resp = dispatch_registered_webhook_fanout(
        &state,
        &ctx,
        "generic",
        &delivery_id,
        &event_type,
        branch.as_deref(),
        commit_sha.as_deref(),
        trigger_data,
        "generic project webhook",
        Some(client_ip),
        vars,
    )
    .await?;

    Ok(Json(resp))
}

#[derive(Debug, Deserialize)]
pub struct GitHubWebhookPayload {
    #[serde(rename = "ref")]
    pub ref_name: Option<String>,
    pub after: Option<String>,
    pub before: Option<String>,
    pub repository: Option<GitHubRepository>,
    pub pusher: Option<GitHubPusher>,
    pub head_commit: Option<GitHubCommit>,
    pub action: Option<String>,
    pub pull_request: Option<GitHubPullRequest>,
}

#[derive(Debug, Deserialize)]
pub struct GitHubRepository {
    pub id: i64,
    pub name: String,
    pub full_name: String,
    pub clone_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GitHubPusher {
    pub name: String,
    pub email: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GitHubCommit {
    pub id: String,
    pub message: String,
    pub author: Option<GitHubAuthor>,
}

#[derive(Debug, Deserialize)]
pub struct GitHubAuthor {
    pub name: String,
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct GitHubPullRequest {
    pub number: i64,
    pub title: String,
    pub head: GitHubPRRef,
    pub base: GitHubPRRef,
}

#[derive(Debug, Deserialize)]
pub struct GitHubPRRef {
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub sha: String,
}

const GITHUB_SIGNATURE_HEADER: &str = "x-hub-signature-256";
const GITHUB_EVENT_HEADER: &str = "x-github-event";
const GITHUB_DELIVERY_HEADER: &str = "x-github-delivery";

#[utoipa::path(
    post,
    path = "/api/v1/webhooks/github/{org_id}/{trigger_id}",
    params(
        ("org_id" = String, Path, description = "Organization ID"),
        ("trigger_id" = String, Path, description = "Trigger ID"),
    ),
    responses(
        (status = 200, description = "GitHub webhook accepted", body = WebhookResponse),
        (status = 400, description = "Bad request"),
        (status = 403, description = "Invalid signature"),
        (status = 404, description = "Not found"),
    ),
    tag = "webhooks",
)]
#[instrument(skip(state, headers, body))]
async fn handle_github_webhook(
    State(state): State<AppState>,
    Path((org_id, trigger_id)): Path<(OrganizationId, TriggerId)>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<WebhookResponse>> {
    if body.len() > WEBHOOK_MAX_BODY_BYTES {
        return Err(ApiError::bad_request(format!(
            "webhook body exceeds maximum size ({} bytes)",
            WEBHOOK_MAX_BODY_BYTES
        )));
    }

    let ctx = load_inbound_registration(&state, org_id, trigger_id, "github").await?;

    let event = headers
        .get(GITHUB_EVENT_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    let delivery_id = github_delivery_id(&headers, &body);

    debug!(
        org_id = %org_id,
        trigger_id = %trigger_id,
        event = %event,
        delivery_id = %delivery_id,
        "received GitHub webhook"
    );

    let secret = ctx.secret_verifier.as_bytes();
    if !secret.is_empty() {
        let signature = headers
            .get(GITHUB_SIGNATURE_HEADER)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| ApiError::bad_request("Missing X-Hub-Signature-256 header"))?;

        if !verify_github_signature(secret, &body, signature) {
            return Err(ApiError::forbidden("Invalid webhook signature"));
        }
    }

    let payload: GitHubWebhookPayload = serde_json::from_slice(&body)
        .map_err(|e| ApiError::bad_request(format!("Invalid GitHub payload: {e}")))?;

    let (branch, commit_sha) = match event {
        "push" => {
            let branch = payload
                .ref_name
                .as_ref()
                .and_then(|r| r.strip_prefix("refs/heads/"))
                .map(String::from);
            let commit_sha = payload.after.clone();
            (branch, commit_sha)
        }
        "pull_request" => {
            let pr = payload.pull_request.as_ref();
            let branch = pr.map(|p| p.head.ref_name.clone());
            let commit_sha = pr.map(|p| p.head.sha.clone());
            (branch, commit_sha)
        }
        _ => (None, None),
    };

    let trigger_data = serde_json::json!({
        "provider": "github",
        "event": event,
        "repository": payload.repository.as_ref().map(|r| &r.full_name),
        "delivery_id": delivery_id,
    });

    let client_ip = webhook_client_ip(&headers, &addr);
    let resp = dispatch_scm_registration_webhook(
        &state,
        &ctx,
        "github",
        &delivery_id,
        event,
        branch.as_deref(),
        commit_sha.as_deref(),
        trigger_data,
        event,
        Some(client_ip),
    )
    .await?;

    Ok(Json(resp))
}

#[derive(Debug, Deserialize)]
pub struct GitLabWebhookPayload {
    pub object_kind: Option<String>,
    #[serde(rename = "ref")]
    pub ref_name: Option<String>,
    pub checkout_sha: Option<String>,
    pub before: Option<String>,
    pub after: Option<String>,
    pub project: Option<GitLabProject>,
    pub user_name: Option<String>,
    pub user_email: Option<String>,
    pub object_attributes: Option<GitLabMergeRequest>,
}

#[derive(Debug, Deserialize)]
pub struct GitLabProject {
    pub id: i64,
    pub name: String,
    pub path_with_namespace: String,
    pub git_http_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GitLabMergeRequest {
    pub iid: i64,
    pub title: String,
    pub source_branch: String,
    pub target_branch: String,
    pub state: String,
    pub last_commit: Option<GitLabCommit>,
}

#[derive(Debug, Deserialize)]
pub struct GitLabCommit {
    pub id: String,
    pub message: String,
}

const GITLAB_EVENT_HEADER: &str = "x-gitlab-event";
const GITLAB_TOKEN_HEADER: &str = "x-gitlab-token";

#[utoipa::path(
    post,
    path = "/api/v1/webhooks/gitlab/{org_id}/{trigger_id}",
    params(
        ("org_id" = String, Path, description = "Organization ID"),
        ("trigger_id" = String, Path, description = "Trigger ID"),
    ),
    responses(
        (status = 200, description = "GitLab webhook accepted", body = WebhookResponse),
        (status = 400, description = "Bad request"),
        (status = 403, description = "Invalid token"),
        (status = 404, description = "Not found"),
    ),
    tag = "webhooks",
)]
#[instrument(skip(state, headers, body))]
async fn handle_gitlab_webhook(
    State(state): State<AppState>,
    Path((org_id, trigger_id)): Path<(OrganizationId, TriggerId)>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<WebhookResponse>> {
    if body.len() > WEBHOOK_MAX_BODY_BYTES {
        return Err(ApiError::bad_request(format!(
            "webhook body exceeds maximum size ({} bytes)",
            WEBHOOK_MAX_BODY_BYTES
        )));
    }

    let ctx = load_inbound_registration(&state, org_id, trigger_id, "gitlab").await?;

    let _event_header = headers
        .get(GITLAB_EVENT_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    debug!(
        org_id = %org_id,
        trigger_id = %trigger_id,
        "received GitLab webhook"
    );

    let secret = ctx.secret_verifier.clone();
    if !secret.is_empty() {
        let token = headers
            .get(GITLAB_TOKEN_HEADER)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| ApiError::bad_request("Missing X-Gitlab-Token header"))?;
        if !constant_time_eq(token.as_bytes(), secret.as_bytes()) {
            return Err(ApiError::forbidden("Invalid GitLab webhook token"));
        }
    }

    let delivery_id = gitlab_synthetic_delivery_id(&secret, &body);

    let payload: GitLabWebhookPayload = serde_json::from_slice(&body)
        .map_err(|e| ApiError::bad_request(format!("Invalid GitLab payload: {e}")))?;

    let object_kind = payload.object_kind.as_deref().unwrap_or("unknown");

    let (branch, commit_sha) = match object_kind {
        "push" => {
            let branch = payload
                .ref_name
                .as_ref()
                .and_then(|r| r.strip_prefix("refs/heads/"))
                .map(String::from);
            let commit_sha = payload.after.clone();
            (branch, commit_sha)
        }
        "merge_request" => {
            let mr = payload.object_attributes.as_ref();
            let branch = mr.map(|m| m.source_branch.clone());
            let commit_sha = mr.and_then(|m| m.last_commit.as_ref().map(|c| c.id.clone()));
            (branch, commit_sha)
        }
        _ => (None, None),
    };

    let event_for_filters = match object_kind {
        "merge_request" => "pull_request",
        other => other,
    };

    let trigger_data = serde_json::json!({
        "provider": "gitlab",
        "object_kind": object_kind,
        "project": payload.project.as_ref().map(|p| &p.path_with_namespace),
        "delivery_id": delivery_id,
    });

    let client_ip = webhook_client_ip(&headers, &addr);
    let resp = dispatch_scm_registration_webhook(
        &state,
        &ctx,
        "gitlab",
        &delivery_id,
        event_for_filters,
        branch.as_deref(),
        commit_sha.as_deref(),
        trigger_data,
        object_kind,
        Some(client_ip),
    )
    .await?;

    Ok(Json(resp))
}

pub fn verify_github_signature(secret: &[u8], body: &[u8], signature: &str) -> bool {
    let expected_prefix = "sha256=";
    if !signature.starts_with(expected_prefix) {
        return false;
    }

    let signature_hex = &signature[expected_prefix.len()..];

    use hmac::{Hmac, Mac};
    type HmacSha256 = Hmac<Sha256>;

    let mut mac = match HmacSha256::new_from_slice(secret) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(body);

    let expected = mac.finalize().into_bytes();
    let expected_hex: String = expected.iter().map(|b| format!("{:02x}", b)).collect();

    constant_time_eq(signature_hex.as_bytes(), expected_hex.as_bytes())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b.iter()).fold(0, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn query_param_first_raw(query: Option<&str>, key: &str) -> Option<String> {
    let q = query?;
    url::form_urlencoded::parse(q.as_bytes())
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.into_owned())
}

fn verify_pipeline_trigger_inbound_auth(
    cfg: &WebhookConfig,
    uri: &Uri,
    headers: &HeaderMap,
    body: &[u8],
) -> ApiResult<()> {
    match cfg.resolved_inbound_auth().as_str() {
        "none" => Ok(()),
        "hmac" => {
            let Some(secret) = cfg.secret.as_deref().filter(|s| !s.is_empty()) else {
                return Err(ApiError::bad_request(
                    "webhook trigger misconfigured: hmac inbound_auth without secret",
                ));
            };
            let signature = headers
                .get(GITHUB_SIGNATURE_HEADER)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| ApiError::bad_request("Missing X-Hub-Signature-256 header"))?;

            if !verify_github_signature(secret.as_bytes(), body, signature) {
                return Err(ApiError::forbidden("Invalid webhook signature"));
            }
            Ok(())
        }
        "query" => {
            let Some(param) = cfg
                .inbound_query_param
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            else {
                return Err(ApiError::internal(
                    "webhook trigger misconfigured (query auth without inbound_query_param)",
                ));
            };
            let Some(secret) = cfg.secret.as_deref().filter(|s| !s.is_empty()) else {
                return Err(ApiError::internal(
                    "webhook trigger misconfigured (query auth without secret)",
                ));
            };
            let got = query_param_first_raw(uri.query(), param).ok_or_else(|| {
                ApiError::forbidden("Missing webhook authentication query parameter")
            })?;
            if !constant_time_eq(got.as_bytes(), secret.as_bytes()) {
                return Err(ApiError::forbidden("Invalid webhook query authentication"));
            }
            Ok(())
        }
        _ => Err(ApiError::internal("unsupported pipeline inbound_auth")),
    }
}

/// Inbound auth for `provider = generic` only (`none` | `hmac` | `query`).
fn verify_generic_inbound_auth(
    ctx: &WebhookRegistrationContext,
    uri: &Uri,
    headers: &HeaderMap,
    body: &[u8],
) -> ApiResult<()> {
    let mode = ctx.generic_inbound_auth.to_lowercase();
    match mode.as_str() {
        "none" => Ok(()),
        "hmac" => {
            let secret = ctx.secret_verifier.as_bytes();
            if secret.is_empty() {
                return Ok(());
            }
            let signature = headers
                .get(GITHUB_SIGNATURE_HEADER)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| ApiError::bad_request("Missing X-Hub-Signature-256 header"))?;

            if !verify_github_signature(secret, body, signature) {
                return Err(ApiError::forbidden("Invalid webhook signature"));
            }
            Ok(())
        }
        "query" => {
            let Some(param) = ctx
                .generic_query_param_name
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            else {
                return Err(ApiError::internal(
                    "webhook registration is misconfigured (query auth without parameter name)",
                ));
            };
            if ctx.secret_verifier.is_empty() {
                return Err(ApiError::internal(
                    "webhook registration is misconfigured (query auth without secret)",
                ));
            }
            let got = query_param_first_raw(uri.query(), param).ok_or_else(|| {
                ApiError::forbidden("Missing webhook authentication query parameter")
            })?;
            if !constant_time_eq(got.as_bytes(), ctx.secret_verifier.as_bytes()) {
                return Err(ApiError::forbidden("Invalid webhook query authentication"));
            }
            Ok(())
        }
        _ => Err(ApiError::internal("unsupported generic_inbound_auth")),
    }
}

fn normalize_generic_inbound_auth(raw: Option<String>) -> ApiResult<String> {
    let v = raw
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "hmac".to_string())
        .to_lowercase();
    match v.as_str() {
        "none" | "hmac" | "query" => Ok(v),
        _ => Err(ApiError::bad_request(
            "generic_inbound_auth must be none, hmac, or query",
        )),
    }
}

/// Allow-list query parameter names (first character alphabetic).
fn generic_query_param_name_valid(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[derive(Debug, Deserialize)]
pub struct BitbucketWebhookPayload {
    pub push: Option<BitbucketPush>,
    pub pullrequest: Option<BitbucketPullRequest>,
    pub repository: Option<BitbucketRepository>,
    pub actor: Option<BitbucketActor>,
}

#[derive(Debug, Deserialize)]
pub struct BitbucketPush {
    pub changes: Vec<BitbucketChange>,
}

#[derive(Debug, Deserialize)]
pub struct BitbucketChange {
    pub new: Option<BitbucketRef>,
    pub old: Option<BitbucketRef>,
}

#[derive(Debug, Deserialize)]
pub struct BitbucketRef {
    pub name: String,
    pub target: Option<BitbucketTarget>,
}

#[derive(Debug, Deserialize)]
pub struct BitbucketTarget {
    pub hash: String,
}

#[derive(Debug, Deserialize)]
pub struct BitbucketPullRequest {
    pub id: i64,
    pub title: String,
    pub source: BitbucketPRRef,
    pub destination: BitbucketPRRef,
}

#[derive(Debug, Deserialize)]
pub struct BitbucketPRRef {
    pub branch: BitbucketBranch,
}

#[derive(Debug, Deserialize)]
pub struct BitbucketBranch {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct BitbucketRepository {
    pub uuid: String,
    pub full_name: String,
}

#[derive(Debug, Deserialize)]
pub struct BitbucketActor {
    pub display_name: String,
}

const BITBUCKET_EVENT_HEADER: &str = "x-event-key";

#[utoipa::path(
    post,
    path = "/api/v1/webhooks/bitbucket/{org_id}/{trigger_id}",
    params(
        ("org_id" = String, Path, description = "Organization ID"),
        ("trigger_id" = String, Path, description = "Trigger ID"),
    ),
    responses(
        (status = 200, description = "Bitbucket webhook accepted", body = WebhookResponse),
        (status = 400, description = "Bad request"),
        (status = 404, description = "Not found"),
    ),
    tag = "webhooks",
)]
#[instrument(skip(state, headers, body))]
async fn handle_bitbucket_webhook(
    State(state): State<AppState>,
    Path((org_id, trigger_id)): Path<(OrganizationId, TriggerId)>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<WebhookResponse>> {
    if body.len() > WEBHOOK_MAX_BODY_BYTES {
        return Err(ApiError::bad_request(format!(
            "webhook body exceeds maximum size ({} bytes)",
            WEBHOOK_MAX_BODY_BYTES
        )));
    }

    let ctx = load_inbound_registration(&state, org_id, trigger_id, "bitbucket").await?;

    let event = headers
        .get(BITBUCKET_EVENT_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    debug!(
        org_id = %org_id,
        trigger_id = %trigger_id,
        event = %event,
        "received Bitbucket webhook"
    );

    let _secret = ctx.secret_verifier.clone();
    let delivery_id = bitbucket_delivery_id(&body);

    let payload: BitbucketWebhookPayload = serde_json::from_slice(&body)
        .map_err(|e| ApiError::bad_request(format!("Invalid Bitbucket payload: {e}")))?;

    let (branch, commit_sha, event_for_filters) = if let Some(ref push) = payload.push {
        let change = push.changes.first();
        let branch = change.and_then(|c| c.new.as_ref()).map(|r| r.name.clone());
        let commit_sha = change
            .and_then(|c| c.new.as_ref())
            .and_then(|r| r.target.as_ref())
            .map(|t| t.hash.clone());
        (branch, commit_sha, "push")
    } else if let Some(ref pr) = payload.pullrequest {
        (Some(pr.source.branch.name.clone()), None, "pull_request")
    } else {
        (None, None, event)
    };

    let trigger_data = serde_json::json!({
        "provider": "bitbucket",
        "event": event,
        "repository": payload.repository.as_ref().map(|r| &r.full_name),
        "delivery_id": delivery_id,
    });

    let client_ip = webhook_client_ip(&headers, &addr);
    let resp = dispatch_scm_registration_webhook(
        &state,
        &ctx,
        "bitbucket",
        &delivery_id,
        event_for_filters,
        branch.as_deref(),
        commit_sha.as_deref(),
        trigger_data,
        event,
        Some(client_ip),
    )
    .await?;

    Ok(Json(resp))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SetupScmWebhookTargetInput {
    #[schema(value_type = String)]
    pub pipeline_id: PipelineId,
    #[serde(default)]
    pub filter_config: serde_json::Value,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SetupScmWebhookRequest {
    pub provider: String,
    /// Stored for compatibility; not written to the database today.
    #[serde(default)]
    pub repository_url: Option<String>,
    pub events: Option<Vec<String>>,
    #[serde(default)]
    pub targets: Vec<SetupScmWebhookTargetInput>,
    /// For `provider: generic`: optional [`WebhookConfig`] JSON (no `secret`); controls JSON→variable mapping.
    #[serde(default)]
    pub payload_mapping: Option<serde_json::Value>,
    /// For `provider: generic`: `none` (no verification), `hmac` (default, `X-Hub-Signature-256`), or `query` (secret in URL).
    #[serde(default)]
    pub generic_inbound_auth: Option<String>,
    /// For `generic_inbound_auth: query`: query parameter name (e.g. `token`). Value must equal the signing secret.
    #[serde(default)]
    pub generic_query_param_name: Option<String>,
    /// Optional label shown in the project webhooks list.
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SetupScmWebhookResponse {
    pub webhook_id: String,
    pub webhook_url: String,
    pub provider: String,
    pub events: Vec<String>,
    /// For `generic` with `hmac` or `query` auth: shared secret (hex). Shown once. Omitted for `generic_inbound_auth: none`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signing_secret: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ProjectWebhookRegistrationResponse {
    #[schema(value_type = String)]
    pub id: Uuid,
    pub provider: String,
    pub events: Vec<String>,
    pub active: bool,
    #[schema(value_type = Object)]
    pub payload_mapping: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Relative inbound URL (prepend public API origin), includes org id and provider when needed.
    pub inbound_path: String,
    /// For `generic`: `none`, `hmac`, or `query`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generic_inbound_auth: Option<String>,
    /// When auth is `query`: parameter name callers must append to the URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generic_query_param_name: Option<String>,
    /// Whether inbound signing material is stored (`secret_hash` non-empty).
    pub inbound_secret_configured: bool,
    /// Present only when a new verifier was generated (e.g. enabling auth from `none`). Same semantics as create.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signing_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = String, nullable = true)]
    pub created_by_user_id: Option<UserId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by_username: Option<String>,
}

impl ProjectWebhookRegistrationResponse {
    fn from_summary(s: WebhookRegistrationSummary, org_id: OrganizationId) -> Self {
        let prov = s.provider.to_lowercase();
        let inbound_path = if prov == "generic" {
            format!("/api/v1/webhooks/{org_id}/{}", s.id)
        } else {
            format!("/api/v1/webhooks/{prov}/{org_id}/{}", s.id)
        };
        let (gia, gqpn) = if prov == "generic" {
            (
                Some(s.generic_inbound_auth.clone()),
                s.generic_query_param_name.clone(),
            )
        } else {
            (None, None)
        };
        Self {
            id: s.id,
            provider: s.provider,
            events: s.events,
            active: s.active,
            payload_mapping: s.payload_mapping,
            created_at: s.created_at,
            inbound_path,
            generic_inbound_auth: gia,
            generic_query_param_name: gqpn,
            inbound_secret_configured: s.secret_configured,
            signing_secret: None,
            description: s.description,
            created_by_user_id: s.created_by_user_id,
            created_by_username: s.created_by_username,
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/projects/{project_id}/scm/setup",
    params(("project_id" = String, Path, description = "Project ID")),
    request_body = SetupScmWebhookRequest,
    responses(
        (status = 200, description = "SCM webhook configured", body = SetupScmWebhookResponse),
        (status = 400, description = "Bad request"),
    ),
    tag = "webhooks",
)]
#[instrument(skip(state))]
async fn setup_scm_webhook(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(project_id): Path<ProjectId>,
    Json(req): Json<SetupScmWebhookRequest>,
) -> ApiResult<Json<SetupScmWebhookResponse>> {
    require_project_in_user_org(state.db(), &user, project_id).await?;

    let provider = req.provider.to_lowercase();
    let (events, payload_mapping): (Vec<String>, serde_json::Value) = if provider == "generic" {
        (
            req.events.unwrap_or_default(),
            req.payload_mapping.unwrap_or_else(|| serde_json::json!({})),
        )
    } else if matches!(provider.as_str(), "github" | "gitlab" | "bitbucket") {
        (
            req.events
                .unwrap_or_else(|| vec!["push".to_string(), "pull_request".to_string()]),
            serde_json::json!({}),
        )
    } else {
        return Err(ApiError::bad_request(format!(
            "Unsupported provider: {}. Supported: github, gitlab, bitbucket, generic",
            req.provider
        )));
    };

    let (generic_auth, query_param_name): (String, Option<String>) = if provider == "generic" {
        let auth = normalize_generic_inbound_auth(req.generic_inbound_auth.clone())?;
        let qn = req
            .generic_query_param_name
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        if auth == "query" {
            let Some(ref n) = qn else {
                return Err(ApiError::bad_request(
                    "generic_query_param_name is required when generic_inbound_auth is query",
                ));
            };
            if !generic_query_param_name_valid(n) {
                return Err(ApiError::bad_request(
                    "generic_query_param_name must start with a letter and use only letters, digits, hyphen, or underscore",
                ));
            }
        } else if qn.is_some() {
            return Err(ApiError::bad_request(
                "generic_query_param_name is only allowed when generic_inbound_auth is query",
            ));
        }
        (auth, qn)
    } else {
        ("hmac".to_string(), None)
    };

    let (secret_hash, signing_secret): (String, Option<String>) =
        if provider == "generic" && generic_auth == "none" {
            (String::new(), None)
        } else {
            let secret = uuid::Uuid::new_v4().to_string();
            let h = format!("{:x}", Sha256::digest(secret.as_bytes()));
            let reveal = if provider == "generic" {
                (generic_auth != "none").then_some(h.clone())
            } else {
                None
            };
            (h, reveal)
        };

    let description: Option<String> = req.description.and_then(|s| {
        let t = s.trim().to_string();
        (!t.is_empty()).then_some(t)
    });

    let webhook_id: (uuid::Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO webhook_registrations (project_id, provider, secret_hash, events, payload_mapping, generic_inbound_auth, generic_query_param_name, description, created_by_user_id)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING id
        "#,
    )
    .bind(project_id.as_uuid())
    .bind(&provider)
    .bind(&secret_hash)
    .bind(&events)
    .bind(&payload_mapping)
    .bind(&generic_auth)
    .bind(&query_param_name)
    .bind(&description)
    .bind(user.user_id.as_uuid())
    .fetch_one(state.db())
    .await
    .map_err(met_store::StoreError::from)?;

    let registration_id = TriggerId::from_uuid(webhook_id.0);
    let pipeline_repo = PipelineRepo::new(state.db());
    let hook_repo = WebhookRepo::new(state.db());

    for t in &req.targets {
        let pipeline = pipeline_repo.get(t.pipeline_id).await?;
        if pipeline.project_id != project_id {
            return Err(ApiError::unprocessable(format!(
                "pipeline {} does not belong to this project",
                t.pipeline_id
            )));
        }
        hook_repo
            .insert_target(
                registration_id,
                &CreateWebhookTarget {
                    pipeline_id: t.pipeline_id,
                    enabled: true,
                    filter_config: t.filter_config.clone(),
                },
            )
            .await?;
    }

    let webhook_url = if provider == "generic" {
        format!(
            "/api/v1/webhooks/{org}/{trigger}",
            org = user.org_id,
            trigger = registration_id.as_uuid(),
        )
    } else {
        format!(
            "/api/v1/webhooks/{provider}/{org}/{trigger}",
            provider = provider,
            org = user.org_id,
            trigger = registration_id.as_uuid(),
        )
    };

    Ok(Json(SetupScmWebhookResponse {
        webhook_id: registration_id.as_uuid().to_string(),
        webhook_url,
        provider,
        events,
        signing_secret,
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/projects/{project_id}/webhooks",
    params(("project_id" = String, Path, description = "Project ID")),
    responses(
        (status = 200, description = "Webhook registrations", body = Vec<ProjectWebhookRegistrationResponse>),
    ),
    tag = "webhooks",
)]
#[instrument(skip(state))]
async fn list_project_webhooks(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(project_id): Path<ProjectId>,
) -> ApiResult<Json<Vec<ProjectWebhookRegistrationResponse>>> {
    require_project_in_user_org(state.db(), &user, project_id).await?;
    let rows = WebhookRepo::new(state.db())
        .list_registrations_for_project(project_id)
        .await?;
    let out = rows
        .into_iter()
        .map(|s| ProjectWebhookRegistrationResponse::from_summary(s, user.org_id))
        .collect();
    Ok(Json(out))
}

#[utoipa::path(
    patch,
    path = "/api/v1/projects/{project_id}/webhooks/{registration_id}",
    params(
        ("project_id" = String, Path, description = "Project ID"),
        ("registration_id" = String, Path, description = "Webhook registration ID"),
    ),
    request_body = PatchProjectWebhookRequest,
    responses(
        (status = 200, description = "Updated registration", body = ProjectWebhookRegistrationResponse),
        (status = 400, description = "Bad request"),
    ),
    tag = "webhooks",
)]
#[instrument(skip(state, body))]
async fn patch_project_webhook(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path((project_id, registration_id)): Path<(ProjectId, TriggerId)>,
    Json(body): Json<PatchProjectWebhookRequest>,
) -> ApiResult<Json<ProjectWebhookRegistrationResponse>> {
    require_project_in_user_org(state.db(), &user, project_id).await?;
    let repo = WebhookRepo::new(state.db());
    repo.assert_registration_in_project(project_id, registration_id)
        .await?;

    if body.description.is_none()
        && body.target_pipeline_ids.is_none()
        && body.generic_inbound_auth.is_none()
        && body.generic_query_param_name.is_none()
    {
        return Err(ApiError::bad_request(
            "provide description, target_pipeline_ids, generic_inbound_auth, and/or generic_query_param_name to update",
        ));
    }

    let mut signing_secret_out: Option<String> = None;

    if body.generic_inbound_auth.is_some() || body.generic_query_param_name.is_some() {
        let summary = repo
            .get_registration_summary_for_project(project_id, registration_id)
            .await?;
        if !summary.provider.eq_ignore_ascii_case("generic") {
            return Err(ApiError::bad_request(
                "generic_inbound_auth and generic_query_param_name apply only to generic webhooks",
            ));
        }

        let new_auth = if let Some(ref raw) = body.generic_inbound_auth {
            normalize_generic_inbound_auth(Some(raw.clone()))?
        } else {
            summary.generic_inbound_auth.to_lowercase()
        };

        if body.generic_query_param_name.is_some() && new_auth != "query" {
            return Err(ApiError::bad_request(
                "generic_query_param_name is only valid when generic_inbound_auth is query (set auth to query in the same request)",
            ));
        }

        let qn_db: Option<String> = if new_auth == "query" {
            if let Some(ref raw) = body.generic_query_param_name {
                let t = raw.trim();
                if t.is_empty() {
                    return Err(ApiError::bad_request(
                        "generic_query_param_name must be non-empty for query authentication",
                    ));
                }
                if !generic_query_param_name_valid(t) {
                    return Err(ApiError::bad_request(
                        "generic_query_param_name must start with a letter and use only letters, digits, hyphen, or underscore",
                    ));
                }
                Some(t.to_string())
            } else {
                let fallback = summary
                    .generic_query_param_name
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .unwrap_or("token")
                    .to_string();
                if !generic_query_param_name_valid(&fallback) {
                    return Err(ApiError::bad_request(
                        "existing generic_query_param_name is invalid; set generic_query_param_name explicitly",
                    ));
                }
                Some(fallback)
            }
        } else {
            None
        };

        let current_secret = repo
            .get_secret_hash_for_project_registration(project_id, registration_id)
            .await?;

        let (secret_to_store, revealed): (String, Option<String>) = if new_auth == "none" {
            (String::new(), None)
        } else if current_secret.is_empty()
            || summary.generic_inbound_auth.eq_ignore_ascii_case("none")
        {
            let h = generate_inbound_secret_hash();
            (h.clone(), Some(h))
        } else {
            (current_secret, None)
        };

        repo.update_generic_inbound_for_project(
            project_id,
            registration_id,
            &new_auth,
            qn_db.as_deref(),
            &secret_to_store,
        )
        .await?;
        signing_secret_out = revealed;
    }

    if let Some(ref raw) = body.description {
        let stored = raw.trim();
        let desc = if stored.is_empty() {
            None
        } else {
            Some(stored.to_string())
        };
        repo.update_registration_description(project_id, registration_id, desc)
            .await?;
    }

    if let Some(ref pids) = body.target_pipeline_ids {
        let unique: Vec<PipelineId> = {
            let mut seen = HashSet::new();
            pids.iter()
                .copied()
                .filter(|p| seen.insert(p.as_uuid()))
                .collect()
        };
        repo.sync_registration_targets(project_id, registration_id, &unique)
            .await?;
    }

    let summary = repo
        .get_registration_summary_for_project(project_id, registration_id)
        .await?;
    let mut response = ProjectWebhookRegistrationResponse::from_summary(summary, user.org_id);
    response.signing_secret = signing_secret_out;
    Ok(Json(response))
}

#[utoipa::path(
    post,
    path = "/api/v1/projects/{project_id}/webhooks/{registration_id}/rotate-inbound-secret",
    params(
        ("project_id" = String, Path, description = "Project ID"),
        ("registration_id" = String, Path, description = "Webhook registration ID"),
    ),
    responses(
        (status = 200, description = "New secret (shown once)", body = RotateInboundSecretResponse),
        (status = 400, description = "Cannot rotate (e.g. open/generic none)"),
    ),
    tag = "webhooks",
)]
#[instrument(skip(state))]
async fn rotate_project_webhook_inbound_secret(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path((project_id, registration_id)): Path<(ProjectId, TriggerId)>,
) -> ApiResult<Json<RotateInboundSecretResponse>> {
    require_project_in_user_org(state.db(), &user, project_id).await?;
    let repo = WebhookRepo::new(state.db());
    repo.assert_registration_in_project(project_id, registration_id)
        .await?;

    let summary = repo
        .get_registration_summary_for_project(project_id, registration_id)
        .await?;
    if summary.provider.eq_ignore_ascii_case("generic")
        && summary.generic_inbound_auth.eq_ignore_ascii_case("none")
    {
        return Err(ApiError::bad_request(
            "cannot rotate secret while inbound authentication is disabled",
        ));
    }

    let signing_secret = generate_inbound_secret_hash();
    repo.update_registration_secret_hash(project_id, registration_id, &signing_secret)
        .await?;

    Ok(Json(RotateInboundSecretResponse { signing_secret }))
}

#[utoipa::path(
    post,
    path = "/api/v1/projects/{project_id}/webhooks/{registration_id}/clear-inbound-secret",
    params(
        ("project_id" = String, Path, description = "Project ID"),
        ("registration_id" = String, Path, description = "Webhook registration ID"),
    ),
    responses(
        (status = 200, description = "Updated registration", body = ProjectWebhookRegistrationResponse),
    ),
    tag = "webhooks",
)]
#[instrument(skip(state))]
async fn clear_project_webhook_inbound_secret(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path((project_id, registration_id)): Path<(ProjectId, TriggerId)>,
) -> ApiResult<Json<ProjectWebhookRegistrationResponse>> {
    require_project_in_user_org(state.db(), &user, project_id).await?;
    let repo = WebhookRepo::new(state.db());
    repo.assert_registration_in_project(project_id, registration_id)
        .await?;

    let summary = repo
        .get_registration_summary_for_project(project_id, registration_id)
        .await?;
    repo.clear_registration_inbound_secret(project_id, registration_id, &summary.provider)
        .await?;

    let summary = repo
        .get_registration_summary_for_project(project_id, registration_id)
        .await?;
    Ok(Json(ProjectWebhookRegistrationResponse::from_summary(
        summary,
        user.org_id,
    )))
}

#[utoipa::path(
    get,
    path = "/api/v1/projects/{project_id}/webhooks/{registration_id}/targets",
    params(
        ("project_id" = String, Path, description = "Project ID"),
        ("registration_id" = String, Path, description = "Webhook registration ID"),
    ),
    responses(
        (status = 200, description = "Targets", body = Vec<WebhookTargetResponse>),
        (status = 404, description = "Not found"),
    ),
    tag = "webhooks",
)]
#[instrument(skip(state))]
async fn list_webhook_targets(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path((project_id, registration_id)): Path<(ProjectId, TriggerId)>,
) -> ApiResult<Json<Vec<WebhookTargetResponse>>> {
    require_project_in_user_org(state.db(), &user, project_id).await?;
    let repo = WebhookRepo::new(state.db());
    repo.assert_registration_in_project(project_id, registration_id)
        .await?;
    let rows = repo.list_targets(registration_id).await?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

#[utoipa::path(
    post,
    path = "/api/v1/projects/{project_id}/webhooks/{registration_id}/targets",
    params(
        ("project_id" = String, Path, description = "Project ID"),
        ("registration_id" = String, Path, description = "Webhook registration ID"),
    ),
    request_body = CreateWebhookTargetRequest,
    responses(
        (status = 200, description = "Created", body = WebhookTargetResponse),
        (status = 404, description = "Not found"),
    ),
    tag = "webhooks",
)]
#[instrument(skip(state, req))]
async fn create_webhook_target(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path((project_id, registration_id)): Path<(ProjectId, TriggerId)>,
    Json(req): Json<CreateWebhookTargetRequest>,
) -> ApiResult<Json<WebhookTargetResponse>> {
    require_project_in_user_org(state.db(), &user, project_id).await?;
    let repo = WebhookRepo::new(state.db());
    repo.assert_registration_in_project(project_id, registration_id)
        .await?;

    let pipeline = PipelineRepo::new(state.db()).get(req.pipeline_id).await?;
    if pipeline.project_id != project_id {
        return Err(ApiError::unprocessable(
            "pipeline does not belong to this project",
        ));
    }

    let row = repo
        .insert_target(
            registration_id,
            &CreateWebhookTarget {
                pipeline_id: req.pipeline_id,
                enabled: req.enabled,
                filter_config: req.filter_config,
            },
        )
        .await?;
    Ok(Json(row.into()))
}

#[utoipa::path(
    patch,
    path = "/api/v1/projects/{project_id}/webhooks/{registration_id}/targets/{target_id}",
    params(
        ("project_id" = String, Path, description = "Project ID"),
        ("registration_id" = String, Path, description = "Webhook registration ID"),
        ("target_id" = String, Path, description = "Target ID"),
    ),
    request_body = UpdateWebhookTargetRequest,
    responses(
        (status = 200, description = "Updated", body = WebhookTargetResponse),
        (status = 404, description = "Not found"),
    ),
    tag = "webhooks",
)]
#[instrument(skip(state, req))]
async fn update_webhook_target(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path((project_id, registration_id, target_id)): Path<(ProjectId, TriggerId, Uuid)>,
    Json(req): Json<UpdateWebhookTargetRequest>,
) -> ApiResult<Json<WebhookTargetResponse>> {
    require_project_in_user_org(state.db(), &user, project_id).await?;
    let repo = WebhookRepo::new(state.db());
    repo.assert_registration_in_project(project_id, registration_id)
        .await?;

    let row = repo
        .update_target(
            target_id,
            registration_id,
            &UpdateWebhookTarget {
                enabled: req.enabled,
                filter_config: req.filter_config,
            },
        )
        .await?;
    Ok(Json(row.into()))
}

#[utoipa::path(
    delete,
    path = "/api/v1/projects/{project_id}/webhooks/{registration_id}/targets/{target_id}",
    params(
        ("project_id" = String, Path, description = "Project ID"),
        ("registration_id" = String, Path, description = "Webhook registration ID"),
        ("target_id" = String, Path, description = "Target ID"),
    ),
    responses((status = 200, description = "Deleted")),
    tag = "webhooks",
)]
#[instrument(skip(state))]
async fn delete_webhook_target(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path((project_id, registration_id, target_id)): Path<(ProjectId, TriggerId, Uuid)>,
) -> ApiResult<()> {
    require_project_in_user_org(state.db(), &user, project_id).await?;
    let repo = WebhookRepo::new(state.db());
    repo.assert_registration_in_project(project_id, registration_id)
        .await?;
    repo.delete_target(target_id, registration_id).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    #[test]
    fn github_signature_accepts_valid_hex() {
        let secret = b"test-secret";
        let body = br#"{"action":"opened"}"#;
        type HmacSha256 = Hmac<Sha256>;
        let mut mac = HmacSha256::new_from_slice(secret).unwrap();
        mac.update(body);
        let sig = mac.finalize().into_bytes();
        let hex: String = sig.iter().map(|b| format!("{:02x}", b)).collect();
        let header = format!("sha256={hex}");
        assert!(verify_github_signature(secret, body, &header));
    }

    #[test]
    fn github_signature_rejects_tampered_body() {
        let secret = b"test-secret";
        let body = br#"{"action":"opened"}"#;
        let tampered = br#"{"action":"closed"}"#;
        type HmacSha256 = Hmac<Sha256>;
        let mut mac = HmacSha256::new_from_slice(secret).unwrap();
        mac.update(body);
        let hex: String = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        let header = format!("sha256={hex}");
        assert!(!verify_github_signature(secret, tampered, &header));
    }
}
