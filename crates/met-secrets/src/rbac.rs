//! Role-Based Access Control (RBAC) for Meticulous.
//!
//! This module defines the permission model used throughout Meticulous:
//! - Predefined roles with hierarchical permissions
//! - Fine-grained permission checks
//! - Policy evaluation for access decisions
//!
//! # Role Hierarchy
//!
//! ```text
//! PlatformAdmin
//!     └── OrgAdmin
//!           └── ProjectAdmin
//!                  └── Developer
//!                        └── Viewer
//! ```
//!
//! Higher roles inherit all permissions from lower roles.
//!
//! # Example
//!
//! ```ignore
//! use met_secrets::rbac::{Role, Permission, RbacPolicy, Actor, Resource};
//!
//! let policy = RbacPolicy::new();
//!
//! let actor = Actor::user("usr_123", Role::Developer);
//! let resource = Resource::pipeline("pipe_456", "proj_789");
//!
//! if policy.check(&actor, Permission::RunTrigger, &resource) {
//!     // User can trigger the pipeline
//! }
//! ```

use std::collections::{HashMap, HashSet};

use met_core::{OrganizationId, ProjectId, UserId};
use serde::{Deserialize, Serialize};

use crate::error::RbacError;

/// Result type for RBAC operations.
pub type Result<T> = std::result::Result<T, RbacError>;

/// Roles in the Meticulous permission hierarchy.
///
/// Roles are ordered from most privileged to least privileged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Full platform access, can manage all organizations.
    PlatformAdmin = 100,
    /// Organization administrator, can manage all projects.
    OrgAdmin = 80,
    /// Project administrator, can manage project settings.
    ProjectAdmin = 60,
    /// Developer, can create and run pipelines.
    Developer = 40,
    /// Read-only access to project resources.
    Viewer = 20,
}

impl Role {
    /// Get all roles that this role includes (this role + all lower roles).
    pub fn includes(&self) -> Vec<Role> {
        match self {
            Role::PlatformAdmin => vec![
                Role::PlatformAdmin,
                Role::OrgAdmin,
                Role::ProjectAdmin,
                Role::Developer,
                Role::Viewer,
            ],
            Role::OrgAdmin => vec![
                Role::OrgAdmin,
                Role::ProjectAdmin,
                Role::Developer,
                Role::Viewer,
            ],
            Role::ProjectAdmin => vec![Role::ProjectAdmin, Role::Developer, Role::Viewer],
            Role::Developer => vec![Role::Developer, Role::Viewer],
            Role::Viewer => vec![Role::Viewer],
        }
    }

    /// Check if this role has at least the given privilege level.
    pub fn has_at_least(&self, other: Role) -> bool {
        (*self as u8) >= (other as u8)
    }

    /// Get the display name for this role.
    pub fn display_name(&self) -> &'static str {
        match self {
            Role::PlatformAdmin => "Platform Administrator",
            Role::OrgAdmin => "Organization Administrator",
            Role::ProjectAdmin => "Project Administrator",
            Role::Developer => "Developer",
            Role::Viewer => "Viewer",
        }
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.display_name())
    }
}

impl std::str::FromStr for Role {
    type Err = RbacError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "platform_admin" | "platformadmin" | "super_admin" => Ok(Role::PlatformAdmin),
            "org_admin" | "orgadmin" | "organization_admin" => Ok(Role::OrgAdmin),
            "project_admin" | "projectadmin" => Ok(Role::ProjectAdmin),
            "developer" | "dev" => Ok(Role::Developer),
            "viewer" | "read_only" | "readonly" => Ok(Role::Viewer),
            _ => Err(RbacError::UnknownRole(s.to_string())),
        }
    }
}

/// Permissions that can be granted in Meticulous.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    // Organization permissions
    /// Create new organizations (platform admin only).
    CreateOrganization,
    /// Delete organizations.
    DeleteOrganization,
    /// Manage organization settings.
    ManageOrganization,
    /// View organization details.
    ViewOrganization,

    // Project permissions
    /// Create projects in an organization.
    CreateProject,
    /// Delete projects.
    DeleteProject,
    /// Manage project settings.
    ManageProject,
    /// View project details.
    ViewProject,

    // Pipeline permissions
    /// Create or modify pipeline definitions.
    ManagePipeline,
    /// View pipeline definitions.
    ViewPipeline,
    /// Trigger pipeline runs manually.
    RunTrigger,
    /// Cancel running pipelines.
    CancelRun,
    /// View run history and logs.
    ViewRuns,
    /// Retry failed runs.
    RetryRun,

    // Secret permissions
    /// Create or update secrets.
    ManageSecrets,
    /// View secret metadata (not values).
    ViewSecrets,
    /// Read actual secret values (for job execution).
    ReadSecretValues,

    // Variable permissions
    /// Create or update variables.
    ManageVariables,
    /// View variables.
    ViewVariables,

    // Agent permissions
    /// Register new agents.
    RegisterAgent,
    /// Manage agent configuration.
    ManageAgents,
    /// View agent status.
    ViewAgents,

    // User/group permissions
    /// Invite users to organization.
    InviteUsers,
    /// Manage user roles.
    ManageUsers,
    /// View user list.
    ViewUsers,
    /// Create and manage groups.
    ManageGroups,

    // Audit permissions
    /// View audit logs.
    ViewAuditLogs,

    // Token permissions
    /// Create API tokens.
    CreateTokens,
    /// Revoke API tokens.
    RevokeTokens,
    /// View token list.
    ViewTokens,
}

impl Permission {
    /// Get the minimum role required for this permission.
    pub fn minimum_role(&self) -> Role {
        match self {
            // Platform admin only
            Permission::CreateOrganization | Permission::DeleteOrganization => Role::PlatformAdmin,

            // Org admin
            Permission::ManageOrganization
            | Permission::CreateProject
            | Permission::DeleteProject
            | Permission::InviteUsers
            | Permission::ManageUsers
            | Permission::ManageGroups
            | Permission::RegisterAgent
            | Permission::ManageAgents
            | Permission::ViewAuditLogs => Role::OrgAdmin,

            // Project admin
            Permission::ManageProject
            | Permission::ManageSecrets
            | Permission::ManageVariables
            | Permission::CreateTokens
            | Permission::RevokeTokens => Role::ProjectAdmin,

            // Developer
            Permission::ManagePipeline
            | Permission::RunTrigger
            | Permission::CancelRun
            | Permission::RetryRun
            | Permission::ReadSecretValues => Role::Developer,

            // Viewer
            Permission::ViewOrganization
            | Permission::ViewProject
            | Permission::ViewPipeline
            | Permission::ViewRuns
            | Permission::ViewSecrets
            | Permission::ViewVariables
            | Permission::ViewAgents
            | Permission::ViewUsers
            | Permission::ViewTokens => Role::Viewer,
        }
    }

    /// Get a human-readable description.
    pub fn description(&self) -> &'static str {
        match self {
            Permission::CreateOrganization => "Create new organizations",
            Permission::DeleteOrganization => "Delete organizations",
            Permission::ManageOrganization => "Manage organization settings",
            Permission::ViewOrganization => "View organization details",
            Permission::CreateProject => "Create new projects",
            Permission::DeleteProject => "Delete projects",
            Permission::ManageProject => "Manage project settings",
            Permission::ViewProject => "View project details",
            Permission::ManagePipeline => "Create and modify pipelines",
            Permission::ViewPipeline => "View pipeline definitions",
            Permission::RunTrigger => "Trigger pipeline runs",
            Permission::CancelRun => "Cancel running pipelines",
            Permission::ViewRuns => "View run history and logs",
            Permission::RetryRun => "Retry failed runs",
            Permission::ManageSecrets => "Create and update secrets",
            Permission::ViewSecrets => "View secret metadata",
            Permission::ReadSecretValues => "Read secret values for jobs",
            Permission::ManageVariables => "Create and update variables",
            Permission::ViewVariables => "View variables",
            Permission::RegisterAgent => "Register new agents",
            Permission::ManageAgents => "Manage agent configuration",
            Permission::ViewAgents => "View agent status",
            Permission::InviteUsers => "Invite users to organization",
            Permission::ManageUsers => "Manage user roles",
            Permission::ViewUsers => "View user list",
            Permission::ManageGroups => "Create and manage groups",
            Permission::ViewAuditLogs => "View audit logs",
            Permission::CreateTokens => "Create API tokens",
            Permission::RevokeTokens => "Revoke API tokens",
            Permission::ViewTokens => "View token list",
        }
    }
}

impl std::fmt::Display for Permission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// An actor attempting to perform an action.
#[derive(Debug, Clone)]
pub struct Actor {
    /// The actor's identifier.
    pub id: ActorId,
    /// The actor's role.
    pub role: Role,
    /// Organization scope (None for platform-level).
    pub org_id: Option<OrganizationId>,
    /// Project scope (None for org-level or platform-level).
    pub project_id: Option<ProjectId>,
}

/// Identifier for an actor.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ActorId {
    /// A human user.
    User(UserId),
    /// A service account or API token.
    ServiceAccount(String),
    /// An agent.
    Agent(String),
    /// The system itself (for automated actions).
    System,
}

impl std::fmt::Display for ActorId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActorId::User(id) => write!(f, "user:{id}"),
            ActorId::ServiceAccount(name) => write!(f, "sa:{name}"),
            ActorId::Agent(id) => write!(f, "agent:{id}"),
            ActorId::System => write!(f, "system"),
        }
    }
}

impl Actor {
    /// Create an actor for a user.
    pub fn user(id: UserId, role: Role) -> Self {
        Self {
            id: ActorId::User(id),
            role,
            org_id: None,
            project_id: None,
        }
    }

    /// Create an actor for a service account.
    pub fn service_account(name: impl Into<String>, role: Role) -> Self {
        Self {
            id: ActorId::ServiceAccount(name.into()),
            role,
            org_id: None,
            project_id: None,
        }
    }

    /// Create an actor for an agent.
    pub fn agent(id: impl Into<String>) -> Self {
        Self {
            id: ActorId::Agent(id.into()),
            role: Role::Developer, // Agents have developer-level access for their jobs
            org_id: None,
            project_id: None,
        }
    }

    /// Create a system actor.
    pub fn system() -> Self {
        Self {
            id: ActorId::System,
            role: Role::PlatformAdmin,
            org_id: None,
            project_id: None,
        }
    }

    /// Set the organization scope.
    pub fn in_org(mut self, org_id: OrganizationId) -> Self {
        self.org_id = Some(org_id);
        self
    }

    /// Set the project scope.
    pub fn in_project(mut self, project_id: ProjectId) -> Self {
        self.project_id = Some(project_id);
        self
    }
}

/// A resource being accessed.
#[derive(Debug, Clone)]
pub struct Resource {
    /// The type of resource.
    pub resource_type: ResourceType,
    /// The resource identifier.
    pub id: Option<String>,
    /// Organization that owns this resource.
    pub org_id: Option<OrganizationId>,
    /// Project that owns this resource (if applicable).
    pub project_id: Option<ProjectId>,
}

/// Types of resources in the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    Organization,
    Project,
    Pipeline,
    Run,
    Secret,
    Variable,
    Agent,
    User,
    Group,
    Token,
    AuditLog,
}

impl std::fmt::Display for ResourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResourceType::Organization => write!(f, "organization"),
            ResourceType::Project => write!(f, "project"),
            ResourceType::Pipeline => write!(f, "pipeline"),
            ResourceType::Run => write!(f, "run"),
            ResourceType::Secret => write!(f, "secret"),
            ResourceType::Variable => write!(f, "variable"),
            ResourceType::Agent => write!(f, "agent"),
            ResourceType::User => write!(f, "user"),
            ResourceType::Group => write!(f, "group"),
            ResourceType::Token => write!(f, "token"),
            ResourceType::AuditLog => write!(f, "audit_log"),
        }
    }
}

impl Resource {
    /// Create a resource reference.
    pub fn new(resource_type: ResourceType) -> Self {
        Self {
            resource_type,
            id: None,
            org_id: None,
            project_id: None,
        }
    }

    /// Create an organization resource.
    pub fn organization(org_id: OrganizationId) -> Self {
        Self {
            resource_type: ResourceType::Organization,
            id: Some(org_id.to_string()),
            org_id: Some(org_id),
            project_id: None,
        }
    }

    /// Create a project resource.
    pub fn project(project_id: ProjectId, org_id: OrganizationId) -> Self {
        Self {
            resource_type: ResourceType::Project,
            id: Some(project_id.to_string()),
            org_id: Some(org_id),
            project_id: Some(project_id),
        }
    }

    /// Create a pipeline resource.
    pub fn pipeline(pipeline_id: impl Into<String>, project_id: ProjectId) -> Self {
        Self {
            resource_type: ResourceType::Pipeline,
            id: Some(pipeline_id.into()),
            org_id: None,
            project_id: Some(project_id),
        }
    }

    /// Set the organization.
    pub fn in_org(mut self, org_id: OrganizationId) -> Self {
        self.org_id = Some(org_id);
        self
    }

    /// Set the project.
    pub fn in_project(mut self, project_id: ProjectId) -> Self {
        self.project_id = Some(project_id);
        self
    }

    /// Get a display string for this resource.
    pub fn display(&self) -> String {
        if let Some(id) = &self.id {
            format!("{}:{}", self.resource_type, id)
        } else {
            format!("{}", self.resource_type)
        }
    }
}

/// RBAC policy for checking permissions.
#[derive(Debug, Clone)]
pub struct RbacPolicy {
    /// Custom permission overrides (actor -> permissions).
    overrides: HashMap<String, HashSet<Permission>>,
    /// Denied permissions (actor -> denied permissions).
    denials: HashMap<String, HashSet<Permission>>,
}

impl RbacPolicy {
    /// Create a new RBAC policy with default settings.
    pub fn new() -> Self {
        Self {
            overrides: HashMap::new(),
            denials: HashMap::new(),
        }
    }

    /// Check if an actor has a permission on a resource.
    pub fn check(&self, actor: &Actor, permission: Permission, resource: &Resource) -> bool {
        let actor_key = actor.id.to_string();

        // Check for explicit denial first
        if self
            .denials
            .get(&actor_key)
            .is_some_and(|perms| perms.contains(&permission))
        {
            tracing::trace!(
                actor = %actor_key,
                permission = %permission,
                resource = %resource.display(),
                "Permission explicitly denied"
            );
            return false;
        }

        // Check for explicit override (grant)
        if self
            .overrides
            .get(&actor_key)
            .is_some_and(|perms| perms.contains(&permission))
        {
            tracing::trace!(
                actor = %actor_key,
                permission = %permission,
                resource = %resource.display(),
                "Permission granted via override"
            );
            return true;
        }

        // Check role-based permission
        let min_role = permission.minimum_role();
        let has_permission = actor.role.has_at_least(min_role);

        // Check scope (org/project) if applicable
        let in_scope = self.check_scope(actor, resource);

        let allowed = has_permission && in_scope;

        tracing::trace!(
            actor = %actor_key,
            role = %actor.role,
            permission = %permission,
            resource = %resource.display(),
            min_role = %min_role,
            in_scope = in_scope,
            allowed = allowed,
            "Permission check"
        );

        allowed
    }

    /// Check if the actor's scope matches the resource.
    fn check_scope(&self, actor: &Actor, resource: &Resource) -> bool {
        // System actor has global scope
        if matches!(actor.id, ActorId::System) {
            return true;
        }

        // Platform admins have global scope
        if actor.role == Role::PlatformAdmin {
            return true;
        }

        // Check organization scope
        if let (Some(actor_org), Some(resource_org)) = (&actor.org_id, &resource.org_id)
            && actor_org != resource_org
        {
            return false;
        }

        // For project-scoped resources, check project scope
        // (Only if actor is project-scoped)
        if let (Some(actor_project), Some(resource_project)) =
            (&actor.project_id, &resource.project_id)
            && actor.role < Role::OrgAdmin
            && actor_project != resource_project
        {
            return false;
        }

        true
    }

    /// Grant additional permissions to an actor.
    pub fn grant(&mut self, actor_id: &ActorId, permission: Permission) {
        self.overrides
            .entry(actor_id.to_string())
            .or_default()
            .insert(permission);
    }

    /// Deny a permission to an actor (overrides role-based access).
    pub fn deny(&mut self, actor_id: &ActorId, permission: Permission) {
        self.denials
            .entry(actor_id.to_string())
            .or_default()
            .insert(permission);
    }

    /// Remove a grant or denial for an actor.
    pub fn reset(&mut self, actor_id: &ActorId, permission: Permission) {
        if let Some(perms) = self.overrides.get_mut(&actor_id.to_string()) {
            perms.remove(&permission);
        }
        if let Some(perms) = self.denials.get_mut(&actor_id.to_string()) {
            perms.remove(&permission);
        }
    }

    /// Get all permissions for a role.
    pub fn permissions_for_role(role: Role) -> Vec<Permission> {
        use Permission::*;

        let all_perms = [
            CreateOrganization,
            DeleteOrganization,
            ManageOrganization,
            ViewOrganization,
            CreateProject,
            DeleteProject,
            ManageProject,
            ViewProject,
            ManagePipeline,
            ViewPipeline,
            RunTrigger,
            CancelRun,
            ViewRuns,
            RetryRun,
            ManageSecrets,
            ViewSecrets,
            ReadSecretValues,
            ManageVariables,
            ViewVariables,
            RegisterAgent,
            ManageAgents,
            ViewAgents,
            InviteUsers,
            ManageUsers,
            ViewUsers,
            ManageGroups,
            ViewAuditLogs,
            CreateTokens,
            RevokeTokens,
            ViewTokens,
        ];

        all_perms
            .into_iter()
            .filter(|p| role.has_at_least(p.minimum_role()))
            .collect()
    }
}

impl Default for RbacPolicy {
    fn default() -> Self {
        Self::new()
    }
}

/// Check result with details about why access was granted or denied.
#[derive(Debug, Clone)]
pub struct AccessDecision {
    /// Whether access is allowed.
    pub allowed: bool,
    /// Reason for the decision.
    pub reason: String,
    /// The actor's effective role.
    pub effective_role: Role,
    /// Whether an override was applied.
    pub override_applied: bool,
}

impl AccessDecision {
    /// Create an allowed decision.
    pub fn allowed(reason: impl Into<String>, role: Role) -> Self {
        Self {
            allowed: true,
            reason: reason.into(),
            effective_role: role,
            override_applied: false,
        }
    }

    /// Create a denied decision.
    pub fn denied(reason: impl Into<String>, role: Role) -> Self {
        Self {
            allowed: false,
            reason: reason.into(),
            effective_role: role,
            override_applied: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_hierarchy() {
        assert!(Role::PlatformAdmin.has_at_least(Role::OrgAdmin));
        assert!(Role::PlatformAdmin.has_at_least(Role::Viewer));
        assert!(Role::Developer.has_at_least(Role::Viewer));
        assert!(!Role::Viewer.has_at_least(Role::Developer));
        assert!(!Role::Developer.has_at_least(Role::ProjectAdmin));
    }

    #[test]
    fn test_role_includes() {
        let roles = Role::OrgAdmin.includes();
        assert!(roles.contains(&Role::OrgAdmin));
        assert!(roles.contains(&Role::ProjectAdmin));
        assert!(roles.contains(&Role::Developer));
        assert!(roles.contains(&Role::Viewer));
        assert!(!roles.contains(&Role::PlatformAdmin));
    }

    #[test]
    fn test_permission_minimum_role() {
        assert_eq!(
            Permission::CreateOrganization.minimum_role(),
            Role::PlatformAdmin
        );
        assert_eq!(Permission::ManageSecrets.minimum_role(), Role::ProjectAdmin);
        assert_eq!(Permission::RunTrigger.minimum_role(), Role::Developer);
        assert_eq!(Permission::ViewRuns.minimum_role(), Role::Viewer);
    }

    #[test]
    fn test_policy_role_check() {
        let policy = RbacPolicy::new();
        let org_id = OrganizationId::new();
        let project_id = ProjectId::new();

        let developer = Actor::user(UserId::new(), Role::Developer)
            .in_org(org_id)
            .in_project(project_id);
        let resource = Resource::pipeline("pipe_123", project_id).in_org(org_id);

        assert!(policy.check(&developer, Permission::RunTrigger, &resource));
        assert!(policy.check(&developer, Permission::ViewRuns, &resource));
        assert!(!policy.check(&developer, Permission::ManageSecrets, &resource));
    }

    #[test]
    fn test_policy_override() {
        let mut policy = RbacPolicy::new();
        let user_id = UserId::new();
        let actor_id = ActorId::User(user_id);

        // Grant extra permission to a viewer
        policy.grant(&actor_id, Permission::RunTrigger);

        let viewer = Actor::user(user_id, Role::Viewer);
        let resource = Resource::new(ResourceType::Pipeline);

        assert!(policy.check(&viewer, Permission::RunTrigger, &resource));
        assert!(policy.check(&viewer, Permission::ViewRuns, &resource));
    }

    #[test]
    fn test_policy_denial() {
        let mut policy = RbacPolicy::new();
        let user_id = UserId::new();
        let actor_id = ActorId::User(user_id);

        // Deny permission even though role would allow it
        policy.deny(&actor_id, Permission::RunTrigger);

        let developer = Actor::user(user_id, Role::Developer);
        let resource = Resource::new(ResourceType::Pipeline);

        assert!(!policy.check(&developer, Permission::RunTrigger, &resource));
        assert!(policy.check(&developer, Permission::ViewRuns, &resource)); // Other perms still work
    }

    #[test]
    fn test_scope_check() {
        let policy = RbacPolicy::new();
        let org1 = OrganizationId::new();
        let org2 = OrganizationId::new();
        let project = ProjectId::new();

        let developer = Actor::user(UserId::new(), Role::Developer)
            .in_org(org1)
            .in_project(project);

        let resource_same_org = Resource::pipeline("pipe_123", project).in_org(org1);
        let resource_diff_org = Resource::pipeline("pipe_456", project).in_org(org2);

        assert!(policy.check(&developer, Permission::RunTrigger, &resource_same_org));
        assert!(!policy.check(&developer, Permission::RunTrigger, &resource_diff_org));
    }

    #[test]
    fn test_system_actor() {
        let policy = RbacPolicy::new();
        let system = Actor::system();
        let resource = Resource::new(ResourceType::Organization);

        assert!(policy.check(&system, Permission::CreateOrganization, &resource));
        assert!(policy.check(&system, Permission::DeleteOrganization, &resource));
    }

    #[test]
    fn test_permissions_for_role() {
        let viewer_perms = RbacPolicy::permissions_for_role(Role::Viewer);
        assert!(viewer_perms.contains(&Permission::ViewRuns));
        assert!(!viewer_perms.contains(&Permission::RunTrigger));

        let admin_perms = RbacPolicy::permissions_for_role(Role::PlatformAdmin);
        assert!(admin_perms.contains(&Permission::CreateOrganization));
        assert!(admin_perms.contains(&Permission::ViewRuns));
    }

    #[test]
    fn test_role_from_str() {
        assert_eq!("developer".parse::<Role>().unwrap(), Role::Developer);
        assert_eq!("org_admin".parse::<Role>().unwrap(), Role::OrgAdmin);
        assert!("unknown".parse::<Role>().is_err());
    }
}
