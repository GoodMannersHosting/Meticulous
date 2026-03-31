//! Agent management routes.

use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};
use met_core::{
    ids::{AgentId, OrganizationId},
    models::{Agent, AgentStatus},
};
use met_store::repos::AgentRepo;
use met_store::StoreError;
use serde::{Deserialize, Serialize};
use tracing::instrument;
use utoipa::ToSchema;

use crate::{
    error::{ApiError, ApiResult},
    extractors::{Auth, Pagination, PaginatedResponse},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/agents", get(list_agents))
        .route("/agents/{id}", get(get_agent).delete(delete_agent))
        .route("/agents/{id}/drain", axum::routing::post(drain_agent))
        .route("/agents/{id}/resume", axum::routing::post(resume_agent))
        .route("/agents/{id}/revoke", axum::routing::post(revoke_agent))
}

#[derive(Debug, Deserialize)]
pub struct ListAgentsQuery {
    org_id: Option<OrganizationId>,
    status: Option<String>,
    pool: Option<String>,
    tags: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AgentResponse {
    #[schema(value_type = String)]
    pub id: AgentId,
    #[schema(value_type = String)]
    pub org_id: OrganizationId,
    pub name: String,
    #[schema(value_type = String)]
    pub status: AgentStatus,
    pub pool: Option<String>,
    pub tags: Vec<String>,
    pub os: String,
    pub arch: String,
    pub version: String,
    pub max_jobs: i32,
    pub running_jobs: i32,
    pub available_capacity: i32,
    pub last_heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<Agent> for AgentResponse {
    fn from(a: Agent) -> Self {
        let available_capacity = a.max_jobs - a.running_jobs;
        Self {
            id: a.id,
            org_id: a.org_id,
            name: a.name,
            status: a.status,
            pool: a.pool,
            tags: a.tags,
            os: a.os,
            arch: a.arch,
            version: a.version,
            max_jobs: a.max_jobs,
            running_jobs: a.running_jobs,
            available_capacity,
            last_heartbeat_at: a.last_heartbeat_at,
            created_at: a.created_at,
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/agents",
    params(
        ("status" = Option<String>, Query, description = "Filter by agent status"),
        ("pool" = Option<String>, Query, description = "Filter by pool"),
        ("tags" = Option<String>, Query, description = "Filter by tags (comma-separated)"),
        ("cursor" = Option<String>, Query, description = "Pagination cursor"),
        ("limit" = Option<u32>, Query, description = "Items per page"),
    ),
    responses(
        (status = 200, description = "List of agents", body = serde_json::Value),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "agents",
)]
#[instrument(skip(state))]
async fn list_agents(
    State(state): State<AppState>,
    Auth(user): Auth,
    pagination: Pagination,
    axum::extract::Query(query): axum::extract::Query<ListAgentsQuery>,
) -> ApiResult<Json<PaginatedResponse<AgentResponse>>> {
    let repo = AgentRepo::new(state.db());

    let agents = repo.list_by_org(user.org_id, pagination.sql_limit(), 0).await?;

    let filtered: Vec<AgentResponse> = agents
        .into_iter()
        .filter(|a| {
            if let Some(ref status) = query.status {
                let agent_status = format!("{:?}", a.status).to_lowercase();
                if agent_status != status.to_lowercase() {
                    return false;
                }
            }
            if let Some(ref pool) = query.pool {
                if a.pool.as_ref() != Some(pool) {
                    return false;
                }
            }
            if let Some(ref tags) = query.tags {
                let required_tags: Vec<&str> = tags.split(',').collect();
                for tag in required_tags {
                    if !a.tags.iter().any(|t| t == tag.trim()) {
                        return false;
                    }
                }
            }
            true
        })
        .map(AgentResponse::from)
        .collect();

    let response = PaginatedResponse::new(
        filtered,
        pagination.limit,
        |a| a.id.to_string(),
    );

    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = "/api/v1/agents/{id}",
    params(("id" = String, Path, description = "Agent ID")),
    responses(
        (status = 200, description = "Agent details", body = AgentResponse),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Agent not found"),
    ),
    tag = "agents",
)]
#[instrument(skip(state))]
async fn get_agent(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(id): Path<AgentId>,
) -> ApiResult<Json<AgentResponse>> {
    let repo = AgentRepo::new(state.db());
    let agent = repo.get(id).await?;

    if agent.org_id != user.org_id {
        return Err(ApiError::forbidden("Agent belongs to another organization"));
    }

    Ok(Json(AgentResponse::from(agent)))
}

#[utoipa::path(
    delete,
    path = "/api/v1/agents/{id}",
    params(("id" = String, Path, description = "Agent ID")),
    responses(
        (status = 200, description = "Agent removed from organization"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Agent not found"),
        (status = 409, description = "Agent still has running jobs"),
    ),
    tag = "agents",
)]
#[instrument(skip(state))]
async fn delete_agent(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(id): Path<AgentId>,
) -> ApiResult<Json<serde_json::Value>> {
    let repo = AgentRepo::new(state.db());

    match repo.soft_delete(user.org_id, id).await {
        Ok(()) => Ok(Json(serde_json::json!({
            "message": "agent removed",
            "agent_id": id.to_string()
        }))),
        Err(StoreError::Constraint(msg)) => Err(ApiError::conflict(msg)),
        Err(e) => Err(e.into()),
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AgentActionResponse {
    #[schema(value_type = String)]
    pub agent_id: AgentId,
    pub status: String,
    pub message: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/agents/{id}/drain",
    params(("id" = String, Path, description = "Agent ID")),
    responses(
        (status = 200, description = "Agent draining", body = AgentActionResponse),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Agent not found"),
    ),
    tag = "agents",
)]
#[instrument(skip(state))]
async fn drain_agent(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(id): Path<AgentId>,
) -> ApiResult<Json<AgentActionResponse>> {
    let repo = AgentRepo::new(state.db());

    let agent = repo.get(id).await?;
    if agent.org_id != user.org_id {
        return Err(ApiError::forbidden("Agent belongs to another organization"));
    }

    if agent.status == AgentStatus::Draining {
        return Ok(Json(AgentActionResponse {
            agent_id: id,
            status: "draining".to_string(),
            message: "Agent is already draining".to_string(),
        }));
    }

    repo.update_status(id, AgentStatus::Draining).await?;

    Ok(Json(AgentActionResponse {
        agent_id: id,
        status: "draining".to_string(),
        message: "Agent will stop accepting new jobs".to_string(),
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/agents/{id}/resume",
    params(("id" = String, Path, description = "Agent ID")),
    responses(
        (status = 200, description = "Agent resumed", body = AgentActionResponse),
        (status = 403, description = "Forbidden"),
        (status = 409, description = "Agent not in draining state"),
    ),
    tag = "agents",
)]
#[instrument(skip(state))]
async fn resume_agent(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(id): Path<AgentId>,
) -> ApiResult<Json<AgentActionResponse>> {
    let repo = AgentRepo::new(state.db());

    let agent = repo.get(id).await?;
    if agent.org_id != user.org_id {
        return Err(ApiError::forbidden("Agent belongs to another organization"));
    }

    if agent.status != AgentStatus::Draining {
        return Err(ApiError::conflict(format!(
            "Agent {} is not draining (current status: {:?})",
            id, agent.status
        )));
    }

    let new_status = if agent.running_jobs > 0 {
        AgentStatus::Busy
    } else {
        AgentStatus::Online
    };

    repo.update_status(id, new_status).await?;

    Ok(Json(AgentActionResponse {
        agent_id: id,
        status: format!("{:?}", new_status).to_lowercase(),
        message: "Agent resumed and accepting new jobs".to_string(),
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/agents/{id}/revoke",
    params(("id" = String, Path, description = "Agent ID")),
    responses(
        (status = 200, description = "Agent revoked", body = AgentActionResponse),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Agent not found"),
    ),
    tag = "agents",
)]
#[instrument(skip(state))]
async fn revoke_agent(
    State(state): State<AppState>,
    Auth(user): Auth,
    Path(id): Path<AgentId>,
) -> ApiResult<Json<AgentActionResponse>> {
    let repo = AgentRepo::new(state.db());

    let agent = repo.get(id).await?;
    if agent.org_id != user.org_id {
        return Err(ApiError::forbidden("Agent belongs to another organization"));
    }

    if agent.status == AgentStatus::Revoked {
        return Ok(Json(AgentActionResponse {
            agent_id: id,
            status: "revoked".to_string(),
            message: "Agent is already revoked".to_string(),
        }));
    }

    repo.update_status(id, AgentStatus::Revoked).await?;

    Ok(Json(AgentActionResponse {
        agent_id: id,
        status: "revoked".to_string(),
        message: "Agent has been revoked and can no longer connect".to_string(),
    }))
}
