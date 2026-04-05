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
pub mod job_claim;
pub mod process_watcher;
pub mod registration;
pub mod seccomp_exec;
pub mod security;
pub mod step_log;
pub mod telemetry;

pub use config::{AgentConfig, JoinTokenSource};
pub use error::{AgentError, Result};
pub use executor::JobExecutor;
pub use process_watcher::{
    compute_file_sha256, merge_execution_metadata, ExecutedBinary, ExecutedBinaryRecord,
    JobExecutionMetadata, ProcessWatcher,
};
pub use registration::{registration_needs_join_token, AgentRegistration, RegistrationSource};
