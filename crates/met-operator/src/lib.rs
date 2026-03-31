//! Kubernetes operator for Meticulous CI/CD agent provisioning.
//!
//! This crate provides a Kubernetes operator that manages agent pods,
//! auto-scaling, and integration with the Meticulous controller.
//!
//! ## Architecture
//!
//! The operator watches `AgentPool` custom resources and reconciles the desired
//! state by creating/deleting agent pods. It supports:
//!
//! - **Declarative pool management**: Define agent pools as K8s resources
//! - **Auto-scaling**: Scale based on queue depth or idle agent count
//! - **Health monitoring**: Replace unhealthy agents automatically
//!
//! ## Custom Resources
//!
//! - `AgentPool`: Defines an agent pool with replica counts and pod template
//! - `AgentPoolAutoscaler`: (Optional) Defines scaling policies

pub mod crd;
pub mod error;
pub mod reconciler;

pub use crd::{AgentPool, AgentPoolSpec, AgentPoolStatus};
pub use error::{OperatorError, Result};
pub use reconciler::AgentPoolReconciler;
