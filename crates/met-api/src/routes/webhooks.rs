//! Webhook ingestion routes for SCM events.

use axum::{
    Json, Router,
    body::Bytes,
    extract::{ConnectInfo, Path, State},
    http::HeaderMap,
    routing::post,
};
use std::net::SocketAddr;
use met_core::ids::{OrganizationId, ProjectId, TriggerId};
use met_core::models::{TriggerKind, WebhookConfig, WEBHOOK_MAX_BODY_BYTES};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{debug, info, instrument};
use utoipa::ToSchema;

use crate::{
    error::{ApiError, ApiResult},
    extractors::Auth,
    pipeline_execution,
    state::AppState,
};
use met_store::repos::{get_trigger_for_webhook_dispatch, PipelineRepo};

/// Prefer proxy headers (when present); otherwise use the direct TCP peer address.
fn webhook_client_ip(headers: &HeaderMap, connect: &SocketAddr) -> String {
    const XFF: &str = "x-forwarded-for";
    const XREAL: &str = "x-real-ip";
    if let Some(raw) = headers.get(XFF).and_then(|v| v.to_str().ok()) {
        if let Some(first) = raw.split(',').next().map(str::trim).filter(|s| !s.is_empty()) {
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
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WebhookResponse {
    pub accepted: bool,
    pub run_id: Option<String>,
    pub message: String,
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
#[instrument(skip(state, headers, body))]
async fn handle_webhook(
    State(state): State<AppState>,
    Path((org_id, trigger_id)): Path<(OrganizationId, TriggerId)>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
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

    let trigger =
        get_trigger_for_webhook_dispatch(state.db(), org_id, trigger_id).await?;

    if trigger.kind != TriggerKind::Webhook {
        return Err(ApiError::not_found("trigger"));
    }
    if !trigger.enabled {
        return Err(ApiError::bad_request("trigger is disabled"));
    }

    let cfg: WebhookConfig = serde_json::from_value(trigger.config.clone()).map_err(|_| {
        ApiError::bad_request("trigger has invalid webhook configuration JSON")
    })?;

    if let Some(secret) = cfg.secret.as_deref().filter(|s| !s.is_empty()) {
        let signature = headers
            .get(GITHUB_SIGNATURE_HEADER)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| ApiError::bad_request("Missing X-Hub-Signature-256 header"))?;

        if !verify_github_signature(secret.as_bytes(), &body, signature) {
            return Err(ApiError::forbidden("Invalid webhook signature"));
        }
    }

    let mut vars = cfg
        .map_payload_to_variables(&body)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;

    // Optional compatibility: generic JSON payloads may carry branch/commit hints.
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

    let client_ip = webhook_client_ip(&headers, &addr);

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
        Some(client_ip),
    )
    .await?;

    info!(run_id = %run.id, "webhook started pipeline run");

    Ok(Json(WebhookResponse {
        accepted: true,
        run_id: Some(run.id.to_string()),
        message: "Webhook accepted; pipeline run created".to_string(),
    }))
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
    ),
    tag = "webhooks",
)]
#[instrument(skip(state, headers, body))]
async fn handle_github_webhook(
    State(state): State<AppState>,
    Path((org_id, trigger_id)): Path<(OrganizationId, TriggerId)>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<WebhookResponse>> {
    let event = headers
        .get(GITHUB_EVENT_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    let delivery_id = headers
        .get(GITHUB_DELIVERY_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    debug!(
        org_id = %org_id,
        trigger_id = %trigger_id,
        event = %event,
        delivery_id = ?delivery_id,
        "received GitHub webhook"
    );

    if let Some(secret) = lookup_webhook_secret(state.db(), trigger_id).await? {
        let signature = headers
            .get(GITHUB_SIGNATURE_HEADER)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| ApiError::bad_request("Missing X-Hub-Signature-256 header"))?;

        if !verify_github_signature(secret.as_bytes(), &body, signature) {
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

    info!(
        event = %event,
        branch = ?branch,
        commit = ?commit_sha,
        "GitHub webhook processed"
    );

    Ok(Json(WebhookResponse {
        accepted: true,
        run_id: None,
        message: format!("GitHub {} event received", event),
    }))
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
    ),
    tag = "webhooks",
)]
#[instrument(skip(headers, body))]
async fn handle_gitlab_webhook(
    State(_state): State<AppState>,
    Path((org_id, trigger_id)): Path<(OrganizationId, TriggerId)>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<WebhookResponse>> {
    let event = headers
        .get(GITLAB_EVENT_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    debug!(
        org_id = %org_id,
        trigger_id = %trigger_id,
        event = %event,
        "received GitLab webhook"
    );

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

    info!(
        object_kind = %object_kind,
        branch = ?branch,
        commit = ?commit_sha,
        "GitLab webhook processed"
    );

    Ok(Json(WebhookResponse {
        accepted: true,
        run_id: None,
        message: format!("GitLab {} event received", object_kind),
    }))
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

async fn lookup_webhook_secret(
    pool: &sqlx::PgPool,
    trigger_id: TriggerId,
) -> ApiResult<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT secret_hash FROM webhook_registrations WHERE id = $1 AND active = true",
    )
    .bind(trigger_id.as_uuid())
    .fetch_optional(pool)
    .await
    .map_err(met_store::StoreError::from)?;

    Ok(row.map(|(s,)| s))
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
    ),
    tag = "webhooks",
)]
#[instrument(skip(state, headers, body))]
async fn handle_bitbucket_webhook(
    State(state): State<AppState>,
    Path((org_id, trigger_id)): Path<(OrganizationId, TriggerId)>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<WebhookResponse>> {
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

    let _secret = lookup_webhook_secret(state.db(), trigger_id).await?;

    let payload: BitbucketWebhookPayload = serde_json::from_slice(&body)
        .map_err(|e| ApiError::bad_request(format!("Invalid Bitbucket payload: {e}")))?;

    let (branch, commit_sha) = if let Some(ref push) = payload.push {
        let change = push.changes.first();
        let branch = change.and_then(|c| c.new.as_ref()).map(|r| r.name.clone());
        let commit_sha = change
            .and_then(|c| c.new.as_ref())
            .and_then(|r| r.target.as_ref())
            .map(|t| t.hash.clone());
        (branch, commit_sha)
    } else if let Some(ref pr) = payload.pullrequest {
        (Some(pr.source.branch.name.clone()), None)
    } else {
        (None, None)
    };

    info!(
        event = %event,
        branch = ?branch,
        commit = ?commit_sha,
        "Bitbucket webhook processed"
    );

    Ok(Json(WebhookResponse {
        accepted: true,
        run_id: None,
        message: format!("Bitbucket {} event received", event),
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SetupScmWebhookRequest {
    pub provider: String,
    pub repository_url: String,
    pub events: Option<Vec<String>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SetupScmWebhookResponse {
    pub webhook_id: String,
    pub webhook_url: String,
    pub provider: String,
    pub events: Vec<String>,
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
    Auth(_user): Auth,
    Path(project_id): Path<ProjectId>,
    Json(req): Json<SetupScmWebhookRequest>,
) -> ApiResult<Json<SetupScmWebhookResponse>> {
    let provider = req.provider.to_lowercase();
    if !matches!(provider.as_str(), "github" | "gitlab" | "bitbucket") {
        return Err(ApiError::bad_request(format!(
            "Unsupported SCM provider: {}. Supported: github, gitlab, bitbucket",
            req.provider
        )));
    }

    let events = req
        .events
        .unwrap_or_else(|| vec!["push".to_string(), "pull_request".to_string()]);

    let secret = uuid::Uuid::new_v4().to_string();
    let secret_hash = format!("{:x}", Sha256::digest(secret.as_bytes()));

    let webhook_id: (uuid::Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO webhook_registrations (project_id, provider, secret_hash, events)
        VALUES ($1, $2, $3, $4)
        RETURNING id
        "#,
    )
    .bind(project_id.as_uuid())
    .bind(&provider)
    .bind(&secret_hash)
    .bind(&events)
    .fetch_one(state.db())
    .await
    .map_err(met_store::StoreError::from)?;

    let trigger_id = webhook_id.0;
    let webhook_url = format!(
        "/api/v1/webhooks/{provider}/{org}/{trigger}",
        provider = provider,
        org = _user.org_id,
        trigger = trigger_id,
    );

    Ok(Json(SetupScmWebhookResponse {
        webhook_id: trigger_id.to_string(),
        webhook_url,
        provider,
        events,
    }))
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
