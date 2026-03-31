//! Webhook ingestion routes for SCM events.

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::HeaderMap,
    routing::post,
    Json, Router,
};
use met_core::ids::{OrganizationId, TriggerId};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tracing::{debug, info, instrument};

use crate::{
    error::{ApiError, ApiResult},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/webhooks/{org_id}/{trigger_id}", post(handle_webhook))
        .route("/webhooks/github/{org_id}/{trigger_id}", post(handle_github_webhook))
        .route("/webhooks/gitlab/{org_id}/{trigger_id}", post(handle_gitlab_webhook))
}

#[derive(Debug, Serialize)]
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

#[instrument(skip(body))]
async fn handle_webhook(
    State(_state): State<AppState>,
    Path((org_id, trigger_id)): Path<(OrganizationId, TriggerId)>,
    _headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<WebhookResponse>> {
    debug!(
        org_id = %org_id,
        trigger_id = %trigger_id,
        body_len = body.len(),
        "received generic webhook"
    );

    let payload: GenericWebhookPayload = serde_json::from_slice(&body)
        .map_err(|e| ApiError::bad_request(format!("Invalid JSON payload: {e}")))?;

    info!(
        event = ?payload.event,
        branch = ?payload.branch,
        "processing webhook"
    );

    Ok(Json(WebhookResponse {
        accepted: true,
        run_id: None,
        message: "Webhook received and queued for processing".to_string(),
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

#[instrument(skip(headers, body))]
async fn handle_github_webhook(
    State(_state): State<AppState>,
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
