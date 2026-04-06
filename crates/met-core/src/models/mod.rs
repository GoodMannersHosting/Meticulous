//! Domain models for Meticulous CI/CD.
//!
//! These models represent the core entities in the system hierarchy:
//! - Organization → Project → Pipeline → Job → Step
//! - Run → JobRun → StepRun (execution records)
//! - Agent, Secret, Variable, Trigger, Workflow (supporting entities)
//! - JoinToken, AgentHeartbeat, JobAssignment (agent system)
//! - RBAC: UserRole, ApiToken, AuthProvider (access control)

mod agent;
mod artifact;
mod job;
mod job_assignment;
mod join_token;
mod join_token_description_history;
mod meticulous_app;
mod organization;
mod pipeline;
mod project;
mod rbac;
mod run;
mod secret;
mod step;
mod trigger;
mod user;
mod variable;
mod workflow;

pub use agent::*;
pub use artifact::*;
pub use job::*;
pub use job_assignment::*;
pub use join_token::*;
pub use join_token_description_history::*;
pub use meticulous_app::*;
pub use organization::*;
pub use pipeline::*;
pub use project::*;
pub use rbac::*;
pub use run::*;
pub use secret::*;
pub use step::*;
pub use trigger::*;
pub use user::*;
pub use variable::*;
pub use workflow::*;
