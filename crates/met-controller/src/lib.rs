//! Agent controller for Meticulous CI/CD.
//!
//! This crate provides the controller component that manages agent registration,
//! health monitoring, and job dispatch via NATS.
//!
//! ## Architecture
//!
//! The controller is the central coordination point for agents:
//!
//! - **gRPC Server**: Handles agent registration, heartbeats, status reports, and key exchange
//! - **Agent Registry**: In-memory + DB-backed agent state management
//! - **Health Monitor**: Detects stale agents and requeues their jobs
//! - **NATS Publisher**: Dispatches jobs to agent pools via JetStream
//!
//! ## Usage
//!
//! ```rust,ignore
//! use met_controller::{Controller, ControllerConfig};
//!
//! let config = ControllerConfig::default();
//! let controller = Controller::new(config, pool, nats).await?;
//! controller.run().await?;
//! ```

pub mod config;
pub mod error;
pub mod grpc;
pub mod health;
pub mod jwt;
pub mod nats;
pub mod registry;

pub use config::ControllerConfig;
pub use error::{ControllerError, Result};
pub use grpc::AgentServiceImpl;
pub use health::HealthMonitor;
pub use jwt::JwtManager;
pub use nats::NatsDispatcher;
pub use registry::AgentRegistry;
