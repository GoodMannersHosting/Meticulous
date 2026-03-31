//! Domain models for Meticulous CI/CD.
//!
//! These models represent the core entities in the system hierarchy:
//! - Organization → Project → Pipeline → Job → Step
//! - Run → JobRun → StepRun (execution records)
//! - Agent, Secret, Variable, Trigger, Workflow (supporting entities)
//! - JoinToken, AgentHeartbeat, JobAssignment (agent system)

mod agent;
mod artifact;
mod job;
mod job_assignment;
mod join_token;
mod organization;
mod pipeline;
mod project;
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
pub use organization::*;
pub use pipeline::*;
pub use project::*;
pub use run::*;
pub use secret::*;
pub use step::*;
pub use trigger::*;
pub use user::*;
pub use variable::*;
pub use workflow::*;
