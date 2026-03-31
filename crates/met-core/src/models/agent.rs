//! Agent and pool models.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::ids::{AgentId, AgentPoolId, JoinTokenId, OrganizationId};

/// JSONB column for `agents.last_security_bundle`: SQL `NULL` decodes to `None`.
/// With `sqlx`, use [`sqlx::types::Json`] so nullable JSONB is supported; without `sqlx`, plain [`JsonValue`].
#[cfg(feature = "sqlx")]
pub type LastSecurityBundleSnapshot = sqlx::types::Json<JsonValue>;
#[cfg(not(feature = "sqlx"))]
pub type LastSecurityBundleSnapshot = JsonValue;

/// Environment type for an agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "environment_type", rename_all = "snake_case"))]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentType {
    /// Physical bare-metal machine.
    Physical,
    /// Virtual machine.
    #[default]
    Virtual,
    /// Container (e.g., Kubernetes pod).
    Container,
}

/// A build agent that executes jobs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct Agent {
    /// Unique identifier.
    pub id: AgentId,
    /// Owning organization.
    pub org_id: OrganizationId,
    /// Agent hostname or display name.
    pub name: String,
    /// Current status.
    pub status: AgentStatus,
    /// Pool membership (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool: Option<String>,
    /// Pools this agent may receive work for (from join token + capabilities).
    #[serde(default = "default_pool_tags")]
    #[cfg_attr(feature = "sqlx", sqlx(default))]
    pub pool_tags: Vec<String>,
    /// Tags for job matching.
    pub tags: Vec<String>,
    /// Agent capabilities (JSON).
    #[cfg_attr(feature = "sqlx", sqlx(json))]
    #[serde(default)]
    pub capabilities: JsonValue,
    /// Operating system.
    pub os: String,
    /// CPU architecture.
    pub arch: String,
    /// Agent version.
    pub version: String,
    /// IP address (stored as string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,
    /// Maximum concurrent jobs.
    pub max_jobs: i32,
    /// Currently running job count.
    #[serde(default)]
    pub running_jobs: i32,
    /// Last heartbeat received.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    /// When the agent was registered.
    pub created_at: DateTime<Utc>,

    // Security bundle fields
    /// Environment type (physical, virtual, container).
    #[serde(default)]
    pub environment_type: EnvironmentType,
    /// Kernel version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kernel_version: Option<String>,
    /// Public IP addresses.
    #[serde(default)]
    pub public_ips: Vec<String>,
    /// Private IP addresses.
    #[serde(default)]
    pub private_ips: Vec<String>,
    /// Whether NTP is synchronized.
    #[serde(default = "default_true")]
    pub ntp_synchronized: bool,
    /// Container runtime (docker, podman, containerd, none).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_runtime: Option<String>,
    /// Container runtime version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_runtime_version: Option<String>,
    /// Agent's long-term X509 public key.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "sqlx", sqlx(default))]
    pub x509_public_key: Option<Vec<u8>>,
    /// Join token used for registration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub join_token_id: Option<JoinTokenId>,
    /// JWT expiration time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jwt_expires_at: Option<DateTime<Utc>>,
    /// Whether the JWT can be renewed.
    #[serde(default = "default_true")]
    pub jwt_renewable: bool,
    /// Heartbeats seen without agent-reported draining while DB status is draining.
    #[serde(default)]
    #[cfg_attr(feature = "sqlx", sqlx(default))]
    pub drain_missed_heartbeats: i32,
    /// When the agent was deregistered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deregistered_at: Option<DateTime<Utc>>,
    /// Snapshot of the registration security bundle (audit / debugging).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_security_bundle: Option<LastSecurityBundleSnapshot>,
}

fn default_true() -> bool {
    true
}

fn default_pool_tags() -> Vec<String> {
    vec!["_default".to_string()]
}

/// Wrap registration bundle JSON for persistence (`agents.last_security_bundle`).
#[cfg(feature = "sqlx")]
#[must_use]
pub fn pack_last_security_bundle(v: JsonValue) -> Option<LastSecurityBundleSnapshot> {
    Some(sqlx::types::Json(v))
}

impl Agent {
    /// Create a new agent in offline status.
    #[must_use]
    pub fn new(
        org_id: OrganizationId,
        name: impl Into<String>,
        os: impl Into<String>,
        arch: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        Self {
            id: AgentId::new(),
            org_id,
            name: name.into(),
            status: AgentStatus::Offline,
            pool: None,
            pool_tags: default_pool_tags(),
            tags: Vec::new(),
            capabilities: JsonValue::Object(serde_json::Map::new()),
            os: os.into(),
            arch: arch.into(),
            version: version.into(),
            ip_address: None,
            max_jobs: 1,
            running_jobs: 0,
            last_heartbeat_at: None,
            created_at: Utc::now(),
            environment_type: EnvironmentType::Virtual,
            kernel_version: None,
            public_ips: Vec::new(),
            private_ips: Vec::new(),
            ntp_synchronized: true,
            container_runtime: None,
            container_runtime_version: None,
            x509_public_key: None,
            join_token_id: None,
            jwt_expires_at: None,
            jwt_renewable: true,
            drain_missed_heartbeats: 0,
            deregistered_at: None,
            last_security_bundle: None,
        }
    }

    /// Check if the agent can accept new jobs.
    #[must_use]
    pub fn can_accept_jobs(&self) -> bool {
        self.status == AgentStatus::Online && self.running_jobs < self.max_jobs
    }

    /// Check if the agent is considered healthy (recent heartbeat).
    #[must_use]
    pub fn is_healthy(&self, max_age: chrono::Duration) -> bool {
        match self.last_heartbeat_at {
            Some(last) => Utc::now() - last < max_age,
            None => false,
        }
    }

    /// Check if the agent's JWT is expired.
    #[must_use]
    pub fn is_jwt_expired(&self) -> bool {
        match self.jwt_expires_at {
            Some(expires_at) => Utc::now() >= expires_at,
            None => true,
        }
    }

    /// Check if the agent is revoked or dead.
    #[must_use]
    pub fn is_revoked_or_dead(&self) -> bool {
        matches!(self.status, AgentStatus::Revoked | AgentStatus::Dead)
    }
}

/// Status of an agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "agent_status", rename_all = "snake_case"))]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    /// Agent is connected and accepting jobs.
    Online,
    /// Agent is not connected.
    #[default]
    Offline,
    /// Agent is connected but at capacity.
    Busy,
    /// Agent is finishing current jobs but not accepting new ones.
    Draining,
    /// Agent has been decommissioned.
    Decommissioned,
    /// Agent has been revoked by admin.
    Revoked,
    /// Agent is unresponsive (missed too many heartbeats).
    Dead,
}

impl AgentStatus {
    /// Check if the agent is available for new jobs.
    #[must_use]
    pub const fn is_available(&self) -> bool {
        matches!(self, Self::Online)
    }

    /// Check if the agent is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Decommissioned | Self::Revoked | Self::Dead)
    }
}

/// An agent pool for grouping and selecting agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct AgentPool {
    /// Unique identifier.
    pub id: AgentPoolId,
    /// Owning organization.
    pub org_id: OrganizationId,
    /// Pool name.
    pub name: String,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether agents auto-scale in this pool.
    #[serde(default)]
    pub auto_scale: bool,
    /// Minimum agents (for auto-scale).
    #[serde(default)]
    pub min_agents: i32,
    /// Maximum agents (for auto-scale).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_agents: Option<i32>,
    /// When the pool was created.
    pub created_at: DateTime<Utc>,
}

impl AgentPool {
    /// Create a new agent pool.
    #[must_use]
    pub fn new(org_id: OrganizationId, name: impl Into<String>) -> Self {
        Self {
            id: AgentPoolId::new(),
            org_id,
            name: name.into(),
            description: None,
            auto_scale: false,
            min_agents: 0,
            max_agents: None,
            created_at: Utc::now(),
        }
    }
}

/// Agent capabilities for matching jobs to agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCapability {
    /// Capability name (e.g., "docker", "gpu").
    pub name: String,
    /// Capability version (if applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Additional metadata.
    #[serde(default, skip_serializing_if = "JsonValue::is_null")]
    pub metadata: JsonValue,
}
