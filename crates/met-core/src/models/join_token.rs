//! Join token models for agent enrollment.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{AgentId, JoinTokenId, OrganizationId, PipelineId, ProjectId, UserId};

/// Scope of a join token determining which jobs agents can execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "join_token_scope", rename_all = "snake_case"))]
#[serde(rename_all = "snake_case")]
pub enum JoinTokenScope {
    /// Any job across any tenant.
    Platform,
    /// Any job within a specific tenant.
    #[default]
    Tenant,
    /// Jobs for pipelines in a specific project.
    Project,
    /// Jobs for a specific pipeline only.
    Pipeline,
}

/// A join token for agent enrollment.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct JoinToken {
    /// Unique identifier.
    pub id: JoinTokenId,
    /// SHA-256 hex hash of the token (plaintext never stored).
    pub token_hash: String,
    /// Scope of the token.
    pub scope: JoinTokenScope,
    /// ID of the scoped entity (null for platform scope).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_id: Option<uuid::Uuid>,
    /// Human-readable description (required for new tokens).
    pub description: String,
    /// Optional organization row link for reporting (tenant tokens should set this).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org_id: Option<OrganizationId>,
    /// Maximum uses (always 1 — one-time enrollment).
    pub max_uses: i32,
    /// Current registration count.
    #[serde(default)]
    pub current_uses: i32,
    /// Labels applied to agents using this token.
    #[serde(default)]
    pub labels: Vec<String>,
    /// Pool tags applied to agents using this token.
    #[serde(default)]
    pub pool_tags: Vec<String>,
    /// Token expiration time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    /// Whether the token has been revoked.
    #[serde(default)]
    pub revoked: bool,
    /// User who created the token.
    pub created_by: UserId,
    /// When the token was created.
    pub created_at: DateTime<Utc>,
    /// When the token was last updated.
    pub updated_at: DateTime<Utc>,
    /// Agent that consumed this token (set when registration succeeds).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consumed_by_agent_id: Option<AgentId>,
    /// When the token was consumed for registration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consumed_at: Option<DateTime<Utc>>,
}

impl JoinToken {
    /// Create a new join token with tenant scope.
    #[must_use]
    pub fn new_tenant(
        token_hash: impl Into<String>,
        description: impl Into<String>,
        org_id: OrganizationId,
        created_by: UserId,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: JoinTokenId::new(),
            token_hash: token_hash.into(),
            scope: JoinTokenScope::Tenant,
            scope_id: Some(org_id.as_uuid()),
            description: description.into(),
            org_id: Some(org_id),
            max_uses: 1,
            current_uses: 0,
            labels: Vec::new(),
            pool_tags: Vec::new(),
            expires_at: None,
            revoked: false,
            created_by,
            created_at: now,
            updated_at: now,
            consumed_by_agent_id: None,
            consumed_at: None,
        }
    }

    /// Create a new join token with project scope.
    #[must_use]
    pub fn new_project(
        token_hash: impl Into<String>,
        description: impl Into<String>,
        project_id: ProjectId,
        created_by: UserId,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: JoinTokenId::new(),
            token_hash: token_hash.into(),
            scope: JoinTokenScope::Project,
            scope_id: Some(project_id.as_uuid()),
            description: description.into(),
            org_id: None,
            max_uses: 1,
            current_uses: 0,
            labels: Vec::new(),
            pool_tags: Vec::new(),
            expires_at: None,
            revoked: false,
            created_by,
            created_at: now,
            updated_at: now,
            consumed_by_agent_id: None,
            consumed_at: None,
        }
    }

    /// Create a new join token with pipeline scope.
    #[must_use]
    pub fn new_pipeline(
        token_hash: impl Into<String>,
        description: impl Into<String>,
        pipeline_id: PipelineId,
        created_by: UserId,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: JoinTokenId::new(),
            token_hash: token_hash.into(),
            scope: JoinTokenScope::Pipeline,
            scope_id: Some(pipeline_id.as_uuid()),
            description: description.into(),
            org_id: None,
            max_uses: 1,
            current_uses: 0,
            labels: Vec::new(),
            pool_tags: Vec::new(),
            expires_at: None,
            revoked: false,
            created_by,
            created_at: now,
            updated_at: now,
            consumed_by_agent_id: None,
            consumed_at: None,
        }
    }

    /// Check if the token is valid (not expired, not revoked, not exhausted).
    #[must_use]
    pub fn is_valid(&self) -> bool {
        if self.revoked {
            return false;
        }

        if let Some(expires_at) = self.expires_at
            && Utc::now() >= expires_at
        {
            return false;
        }

        if self.current_uses >= self.max_uses {
            return false;
        }

        true
    }
}

/// Plaintext join token format.
/// Format: met_join_{base62_random(32)}
pub const JOIN_TOKEN_PREFIX: &str = "met_join_";

/// Generate a new plaintext join token.
#[must_use]
pub fn generate_join_token() -> String {
    use uuid::Uuid;
    let random = Uuid::new_v4().simple().to_string();
    format!("{JOIN_TOKEN_PREFIX}{random}")
}
