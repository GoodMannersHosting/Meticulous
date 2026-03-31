//! Repository modules for database operations.
//!
//! Each repository provides CRUD operations for a specific entity type,
//! with compile-time checked SQL queries.

mod agent_heartbeats;
mod agents;
mod api_tokens;
mod auth_providers;
mod groups;
mod job_assignments;
mod join_tokens;
mod organizations;
mod pipelines;
mod projects;
mod roles;
mod runs;
mod users;

pub use agent_heartbeats::{AgentHeartbeatRepo, HeartbeatStats};
pub use agents::AgentRepo;
pub use api_tokens::ApiTokenRepo;
pub use auth_providers::AuthProviderRepo;
pub use groups::GroupRepo;
pub use job_assignments::JobAssignmentRepo;
pub use join_tokens::JoinTokenRepo;
pub use organizations::OrganizationRepo;
pub use pipelines::PipelineRepo;
pub use projects::ProjectRepo;
pub use roles::RoleRepo;
pub use runs::RunRepo;
pub use users::UserRepo;
