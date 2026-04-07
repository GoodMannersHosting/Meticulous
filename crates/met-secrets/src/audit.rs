//! Audit logging for security-relevant events.
//!
//! This module provides types and traits for recording audit events,
//! enabling compliance and security monitoring.
//!
//! # Event Types
//!
//! Audit events cover all security-relevant operations:
//! - Authentication (login, logout, token creation)
//! - Authorization (permission checks, role changes)
//! - Resource access (secrets, pipelines, etc.)
//! - Administrative actions (user management, config changes)
//!
//! # Example
//!
//! ```ignore
//! use met_secrets::audit::{AuditEvent, AuditAction, AuditLogger};
//!
//! let event = AuditEvent::new(AuditAction::SecretAccess)
//!     .with_actor("usr_123")
//!     .with_resource("secret", "sec_456")
//!     .with_outcome(Outcome::Success);
//!
//! logger.log(event).await?;
//! ```

use std::collections::HashMap;
use std::net::IpAddr;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use met_core::{OrganizationId, ProjectId};

use crate::error::AuditError;
use crate::rbac::ActorId;

/// Result type for audit operations.
pub type Result<T> = std::result::Result<T, AuditError>;

/// An audit event recording a security-relevant action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Unique event identifier.
    pub id: Uuid,
    /// When the event occurred.
    pub timestamp: DateTime<Utc>,
    /// The action that was performed.
    pub action: AuditAction,
    /// Who performed the action.
    pub actor: AuditActor,
    /// The resource that was acted upon.
    pub resource: Option<AuditResource>,
    /// The outcome of the action.
    pub outcome: Outcome,
    /// Organization context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org_id: Option<OrganizationId>,
    /// Project context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
    /// Client IP address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_ip: Option<IpAddr>,
    /// User agent string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    /// Request ID for correlation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// Additional metadata.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
    /// Error message if action failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl AuditEvent {
    /// Create a new audit event.
    pub fn new(action: AuditAction) -> Self {
        Self {
            id: Uuid::now_v7(),
            timestamp: Utc::now(),
            action,
            actor: AuditActor::Anonymous,
            resource: None,
            outcome: Outcome::Unknown,
            org_id: None,
            project_id: None,
            client_ip: None,
            user_agent: None,
            request_id: None,
            metadata: HashMap::new(),
            error: None,
        }
    }

    /// Set the actor.
    pub fn with_actor(mut self, actor: impl Into<AuditActor>) -> Self {
        self.actor = actor.into();
        self
    }

    /// Set the actor from an ActorId.
    pub fn with_actor_id(mut self, actor_id: &ActorId) -> Self {
        self.actor = AuditActor::from(actor_id.clone());
        self
    }

    /// Set the resource.
    pub fn with_resource(
        mut self,
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
    ) -> Self {
        self.resource = Some(AuditResource {
            resource_type: resource_type.into(),
            resource_id: resource_id.into(),
            resource_name: None,
        });
        self
    }

    /// Set the resource with name.
    pub fn with_named_resource(
        mut self,
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
        resource_name: impl Into<String>,
    ) -> Self {
        self.resource = Some(AuditResource {
            resource_type: resource_type.into(),
            resource_id: resource_id.into(),
            resource_name: Some(resource_name.into()),
        });
        self
    }

    /// Set the outcome.
    pub fn with_outcome(mut self, outcome: Outcome) -> Self {
        self.outcome = outcome;
        self
    }

    /// Mark as successful.
    pub fn success(mut self) -> Self {
        self.outcome = Outcome::Success;
        self
    }

    /// Mark as failed with error.
    pub fn failure(mut self, error: impl Into<String>) -> Self {
        self.outcome = Outcome::Failure;
        self.error = Some(error.into());
        self
    }

    /// Mark as denied (permission failure).
    pub fn denied(mut self, reason: impl Into<String>) -> Self {
        self.outcome = Outcome::Denied;
        self.error = Some(reason.into());
        self
    }

    /// Set the organization context.
    pub fn in_org(mut self, org_id: OrganizationId) -> Self {
        self.org_id = Some(org_id);
        self
    }

    /// Set the project context.
    pub fn in_project(mut self, project_id: ProjectId) -> Self {
        self.project_id = Some(project_id);
        self
    }

    /// Set the client IP.
    pub fn from_ip(mut self, ip: IpAddr) -> Self {
        self.client_ip = Some(ip);
        self
    }

    /// Set the user agent.
    pub fn with_user_agent(mut self, ua: impl Into<String>) -> Self {
        self.user_agent = Some(ua.into());
        self
    }

    /// Set the request ID for correlation.
    pub fn with_request_id(mut self, id: impl Into<String>) -> Self {
        self.request_id = Some(id.into());
        self
    }

    /// Add metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        if let Ok(json_value) = serde_json::to_value(value) {
            self.metadata.insert(key.into(), json_value);
        }
        self
    }

    /// Add multiple metadata entries.
    pub fn with_metadata_map(mut self, entries: HashMap<String, serde_json::Value>) -> Self {
        self.metadata.extend(entries);
        self
    }
}

/// Actions that can be audited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    // Authentication
    /// User login attempt.
    Login,
    /// User logout.
    Logout,
    /// Failed login attempt.
    LoginFailed,
    /// Password change.
    PasswordChange,
    /// MFA enrollment.
    MfaEnroll,
    /// MFA verification.
    MfaVerify,

    // Token management
    /// API token created.
    TokenCreate,
    /// API token revoked.
    TokenRevoke,
    /// Token used for authentication.
    TokenAuth,

    // User management
    /// User created.
    UserCreate,
    /// User updated.
    UserUpdate,
    /// User deleted.
    UserDelete,
    /// User invited.
    UserInvite,
    /// User role changed.
    RoleChange,

    // Organization management
    /// Organization created.
    OrgCreate,
    /// Organization updated.
    OrgUpdate,
    /// Organization deleted.
    OrgDelete,

    // Project management
    /// Project created.
    ProjectCreate,
    /// Project updated.
    ProjectUpdate,
    /// Project deleted.
    ProjectDelete,

    // Pipeline management
    /// Pipeline created.
    PipelineCreate,
    /// Pipeline updated.
    PipelineUpdate,
    /// Pipeline deleted.
    PipelineDelete,

    // Run management
    /// Pipeline run triggered.
    RunTrigger,
    /// Run cancelled.
    RunCancel,
    /// Run retried.
    RunRetry,

    // Secret management
    /// Secret created.
    SecretCreate,
    /// Secret updated.
    SecretUpdate,
    /// Secret deleted.
    SecretDelete,
    /// Secret value accessed.
    SecretAccess,
    /// Secret metadata viewed.
    SecretView,

    // Variable management
    /// Variable created.
    VariableCreate,
    /// Variable updated.
    VariableUpdate,
    /// Variable deleted.
    VariableDelete,

    // Agent management
    /// Agent registered.
    AgentRegister,
    /// Agent updated.
    AgentUpdate,
    /// Agent removed.
    AgentRemove,

    // Permission checks
    /// Permission granted.
    PermissionGrant,
    /// Permission denied.
    PermissionDeny,

    // Configuration
    /// Configuration changed.
    ConfigChange,

    // Export/Import
    /// Data exported.
    DataExport,
    /// Data imported.
    DataImport,
}

impl AuditAction {
    /// Get the severity level for this action.
    pub fn severity(&self) -> Severity {
        match self {
            // Critical security events
            AuditAction::Login
            | AuditAction::LoginFailed
            | AuditAction::PasswordChange
            | AuditAction::TokenCreate
            | AuditAction::TokenRevoke
            | AuditAction::RoleChange
            | AuditAction::PermissionGrant
            | AuditAction::PermissionDeny => Severity::High,

            // Administrative actions
            AuditAction::UserCreate
            | AuditAction::UserDelete
            | AuditAction::OrgCreate
            | AuditAction::OrgDelete
            | AuditAction::SecretCreate
            | AuditAction::SecretUpdate
            | AuditAction::SecretDelete
            | AuditAction::SecretAccess
            | AuditAction::ConfigChange
            | AuditAction::DataExport
            | AuditAction::DataImport => Severity::Medium,

            // Regular operations
            _ => Severity::Low,
        }
    }

    /// Get a human-readable description.
    pub fn description(&self) -> &'static str {
        match self {
            AuditAction::Login => "User login",
            AuditAction::Logout => "User logout",
            AuditAction::LoginFailed => "Failed login attempt",
            AuditAction::PasswordChange => "Password changed",
            AuditAction::MfaEnroll => "MFA enrolled",
            AuditAction::MfaVerify => "MFA verified",
            AuditAction::TokenCreate => "API token created",
            AuditAction::TokenRevoke => "API token revoked",
            AuditAction::TokenAuth => "Token authentication",
            AuditAction::UserCreate => "User created",
            AuditAction::UserUpdate => "User updated",
            AuditAction::UserDelete => "User deleted",
            AuditAction::UserInvite => "User invited",
            AuditAction::RoleChange => "User role changed",
            AuditAction::OrgCreate => "Organization created",
            AuditAction::OrgUpdate => "Organization updated",
            AuditAction::OrgDelete => "Organization deleted",
            AuditAction::ProjectCreate => "Project created",
            AuditAction::ProjectUpdate => "Project updated",
            AuditAction::ProjectDelete => "Project deleted",
            AuditAction::PipelineCreate => "Pipeline created",
            AuditAction::PipelineUpdate => "Pipeline updated",
            AuditAction::PipelineDelete => "Pipeline deleted",
            AuditAction::RunTrigger => "Pipeline run triggered",
            AuditAction::RunCancel => "Pipeline run cancelled",
            AuditAction::RunRetry => "Pipeline run retried",
            AuditAction::SecretCreate => "Secret created",
            AuditAction::SecretUpdate => "Secret updated",
            AuditAction::SecretDelete => "Secret deleted",
            AuditAction::SecretAccess => "Secret value accessed",
            AuditAction::SecretView => "Secret metadata viewed",
            AuditAction::VariableCreate => "Variable created",
            AuditAction::VariableUpdate => "Variable updated",
            AuditAction::VariableDelete => "Variable deleted",
            AuditAction::AgentRegister => "Agent registered",
            AuditAction::AgentUpdate => "Agent updated",
            AuditAction::AgentRemove => "Agent removed",
            AuditAction::PermissionGrant => "Permission granted",
            AuditAction::PermissionDeny => "Permission denied",
            AuditAction::ConfigChange => "Configuration changed",
            AuditAction::DataExport => "Data exported",
            AuditAction::DataImport => "Data imported",
        }
    }
}

impl std::fmt::Display for AuditAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.description())
    }
}

/// The actor who performed an action.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuditActor {
    /// Anonymous/unauthenticated actor.
    Anonymous,
    /// A human user.
    User {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        username: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        email: Option<String>,
    },
    /// A service account.
    ServiceAccount {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    /// An agent.
    Agent {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    /// The system itself.
    System {
        #[serde(skip_serializing_if = "Option::is_none")]
        component: Option<String>,
    },
}

impl From<ActorId> for AuditActor {
    fn from(actor_id: ActorId) -> Self {
        match actor_id {
            ActorId::User(id) => AuditActor::User {
                id: id.to_string(),
                username: None,
                email: None,
            },
            ActorId::ServiceAccount(name) => AuditActor::ServiceAccount {
                id: name.clone(),
                name: Some(name),
            },
            ActorId::Agent(id) => AuditActor::Agent { id, name: None },
            ActorId::System => AuditActor::System { component: None },
        }
    }
}

impl From<&str> for AuditActor {
    fn from(s: &str) -> Self {
        // Simple parsing: "user:id", "sa:name", "agent:id", "system"
        if let Some(id) = s.strip_prefix("user:") {
            AuditActor::User {
                id: id.to_string(),
                username: None,
                email: None,
            }
        } else if let Some(name) = s.strip_prefix("sa:") {
            AuditActor::ServiceAccount {
                id: name.to_string(),
                name: Some(name.to_string()),
            }
        } else if let Some(id) = s.strip_prefix("agent:") {
            AuditActor::Agent {
                id: id.to_string(),
                name: None,
            }
        } else if s == "system" {
            AuditActor::System { component: None }
        } else {
            AuditActor::User {
                id: s.to_string(),
                username: None,
                email: None,
            }
        }
    }
}

impl From<String> for AuditActor {
    fn from(s: String) -> Self {
        AuditActor::from(s.as_str())
    }
}

/// A resource that was acted upon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditResource {
    /// Type of resource (e.g., "secret", "pipeline").
    pub resource_type: String,
    /// Resource identifier.
    pub resource_id: String,
    /// Human-readable resource name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_name: Option<String>,
}

/// Outcome of an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Action completed successfully.
    Success,
    /// Action failed due to an error.
    Failure,
    /// Action was denied due to permissions.
    Denied,
    /// Outcome is unknown (event in progress).
    #[default]
    Unknown,
}

impl std::fmt::Display for Outcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Outcome::Success => write!(f, "success"),
            Outcome::Failure => write!(f, "failure"),
            Outcome::Denied => write!(f, "denied"),
            Outcome::Unknown => write!(f, "unknown"),
        }
    }
}

/// Severity level for audit events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Low severity (routine operations).
    Low,
    /// Medium severity (administrative actions).
    Medium,
    /// High severity (security-critical events).
    High,
}

/// Trait for audit log backends.
#[async_trait]
pub trait AuditLogger: Send + Sync {
    /// Log an audit event.
    async fn log(&self, event: AuditEvent) -> Result<()>;

    /// Query audit events (for viewing logs).
    async fn query(&self, filter: AuditFilter) -> Result<Vec<AuditEvent>>;

    /// Flush any buffered events.
    async fn flush(&self) -> Result<()>;
}

/// Filter for querying audit events.
#[derive(Debug, Clone, Default)]
pub struct AuditFilter {
    /// Filter by action types.
    pub actions: Option<Vec<AuditAction>>,
    /// Filter by actor ID.
    pub actor_id: Option<String>,
    /// Filter by resource type.
    pub resource_type: Option<String>,
    /// Filter by resource ID.
    pub resource_id: Option<String>,
    /// Filter by organization.
    pub org_id: Option<OrganizationId>,
    /// Filter by project.
    pub project_id: Option<ProjectId>,
    /// Filter by outcome.
    pub outcome: Option<Outcome>,
    /// Start time (inclusive).
    pub start_time: Option<DateTime<Utc>>,
    /// End time (exclusive).
    pub end_time: Option<DateTime<Utc>>,
    /// Maximum results to return.
    pub limit: Option<usize>,
    /// Offset for pagination.
    pub offset: Option<usize>,
}

impl AuditFilter {
    /// Create a new empty filter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by action.
    pub fn action(mut self, action: AuditAction) -> Self {
        self.actions.get_or_insert_with(Vec::new).push(action);
        self
    }

    /// Filter by actor.
    pub fn actor(mut self, actor_id: impl Into<String>) -> Self {
        self.actor_id = Some(actor_id.into());
        self
    }

    /// Filter by resource.
    pub fn resource(
        mut self,
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
    ) -> Self {
        self.resource_type = Some(resource_type.into());
        self.resource_id = Some(resource_id.into());
        self
    }

    /// Filter by organization.
    pub fn in_org(mut self, org_id: OrganizationId) -> Self {
        self.org_id = Some(org_id);
        self
    }

    /// Filter by time range.
    pub fn time_range(mut self, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        self.start_time = Some(start);
        self.end_time = Some(end);
        self
    }

    /// Limit results.
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set offset for pagination.
    pub fn offset(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }
}

/// A no-op audit logger for testing or when auditing is disabled.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoOpAuditLogger;

#[async_trait]
impl AuditLogger for NoOpAuditLogger {
    async fn log(&self, event: AuditEvent) -> Result<()> {
        tracing::trace!(
            action = ?event.action,
            actor = ?event.actor,
            outcome = ?event.outcome,
            "Audit event (no-op)"
        );
        Ok(())
    }

    async fn query(&self, _filter: AuditFilter) -> Result<Vec<AuditEvent>> {
        Ok(Vec::new())
    }

    async fn flush(&self) -> Result<()> {
        Ok(())
    }
}

/// A logging audit backend that writes events to tracing.
#[derive(Debug, Clone, Copy, Default)]
pub struct TracingAuditLogger;

#[async_trait]
impl AuditLogger for TracingAuditLogger {
    async fn log(&self, event: AuditEvent) -> Result<()> {
        let severity = event.action.severity();

        match severity {
            Severity::High => {
                tracing::warn!(
                    id = %event.id,
                    action = ?event.action,
                    actor = ?event.actor,
                    resource = ?event.resource,
                    outcome = %event.outcome,
                    error = ?event.error,
                    "AUDIT"
                );
            }
            Severity::Medium => {
                tracing::info!(
                    id = %event.id,
                    action = ?event.action,
                    actor = ?event.actor,
                    resource = ?event.resource,
                    outcome = %event.outcome,
                    "AUDIT"
                );
            }
            Severity::Low => {
                tracing::debug!(
                    id = %event.id,
                    action = ?event.action,
                    actor = ?event.actor,
                    resource = ?event.resource,
                    outcome = %event.outcome,
                    "AUDIT"
                );
            }
        }

        Ok(())
    }

    async fn query(&self, _filter: AuditFilter) -> Result<Vec<AuditEvent>> {
        // Tracing logger doesn't support queries
        Ok(Vec::new())
    }

    async fn flush(&self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_event_builder() {
        let event = AuditEvent::new(AuditAction::SecretAccess)
            .with_actor("user:usr_123")
            .with_resource("secret", "sec_456")
            .success()
            .with_metadata("path", "secret/myapp/api-key");

        assert_eq!(event.action, AuditAction::SecretAccess);
        assert_eq!(event.outcome, Outcome::Success);
        assert!(event.metadata.contains_key("path"));
    }

    #[test]
    fn test_audit_event_failure() {
        let event = AuditEvent::new(AuditAction::Login)
            .with_actor("user:usr_123")
            .failure("invalid password");

        assert_eq!(event.outcome, Outcome::Failure);
        assert_eq!(event.error, Some("invalid password".to_string()));
    }

    #[test]
    fn test_action_severity() {
        assert_eq!(AuditAction::Login.severity(), Severity::High);
        assert_eq!(AuditAction::SecretAccess.severity(), Severity::Medium);
        assert_eq!(AuditAction::RunTrigger.severity(), Severity::Low);
    }

    #[test]
    fn test_audit_actor_from_string() {
        match AuditActor::from("user:usr_123") {
            AuditActor::User { id, .. } => assert_eq!(id, "usr_123"),
            _ => panic!("Expected User actor"),
        }

        match AuditActor::from("sa:my-service") {
            AuditActor::ServiceAccount { name, .. } => {
                assert_eq!(name, Some("my-service".to_string()))
            }
            _ => panic!("Expected ServiceAccount actor"),
        }

        match AuditActor::from("system") {
            AuditActor::System { .. } => {}
            _ => panic!("Expected System actor"),
        }
    }

    #[test]
    fn test_audit_filter() {
        let filter = AuditFilter::new()
            .action(AuditAction::SecretAccess)
            .actor("usr_123")
            .limit(100);

        assert!(filter.actions.unwrap().contains(&AuditAction::SecretAccess));
        assert_eq!(filter.actor_id, Some("usr_123".to_string()));
        assert_eq!(filter.limit, Some(100));
    }

    #[tokio::test]
    async fn test_noop_logger() {
        let logger = NoOpAuditLogger;
        let event = AuditEvent::new(AuditAction::Login).success();
        assert!(logger.log(event).await.is_ok());
        assert!(logger.query(AuditFilter::new()).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_tracing_logger() {
        let logger = TracingAuditLogger;
        let event = AuditEvent::new(AuditAction::SecretAccess)
            .with_actor("user:test")
            .success();
        assert!(logger.log(event).await.is_ok());
    }
}
