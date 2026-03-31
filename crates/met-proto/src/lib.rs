//! Protobuf definitions and generated code for Meticulous gRPC services.
//!
//! This crate provides the generated Rust types and gRPC service definitions
//! for agent-controller communication.

#![allow(clippy::all)]
#![allow(clippy::pedantic)]
#![allow(clippy::nursery)]

/// Common types used across services.
pub mod common {
    pub mod v1 {
        tonic::include_proto!("meticulous.common.v1");
    }
}

/// Agent service and types.
pub mod agent {
    pub mod v1 {
        tonic::include_proto!("meticulous.agent.v1");
    }
}

/// Controller message types for NATS.
pub mod controller {
    pub mod v1 {
        tonic::include_proto!("meticulous.controller.v1");
    }
}

// Re-export commonly used types
pub use agent::v1::{
    agent_service_client::AgentServiceClient,
    agent_service_server::{AgentService, AgentServiceServer},
};
pub use common::v1::{AgentStatus, RunStatus, StepKind};
