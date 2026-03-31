//! Meticulous build agent library.
//!
//! This crate provides the core functionality for the Meticulous build agent:
//!
//! - Configuration loading from files and environment
//! - gRPC client for controller communication
//! - NATS client for job dispatch
//! - Job execution with isolated environments
//! - Log streaming and status reporting
//!
//! ## Architecture
//!
//! ```text
//! met-agent process
//! ├── Config Loader        ← TOML/env config
//! ├── gRPC Client          ← tonic client to met-controller
//! ├── NATS Client          ← async-nats with JetStream
//! ├── Heartbeat Task       ← periodic gRPC heartbeat
//! ├── Job Executor Loop    ← pull NATS → execute → report
//! │   ├── PKI Manager      ← per-job X509 keypair
//! │   ├── Execution Backend
//! │   │   ├── ContainerBackend (Linux)
//! │   │   └── NativeBackend (macOS/Win)
//! │   └── Log Shipper      ← streams logs to controller
//! └── Signal Handler       ← SIGTERM/SIGINT → graceful drain
//! ```

pub mod backend;
pub mod config;
pub mod error;
pub mod executor;
pub mod heartbeat;
pub mod registration;
pub mod security;

pub use config::AgentConfig;
pub use error::{AgentError, Result};
pub use executor::JobExecutor;
pub use registration::AgentRegistration;
