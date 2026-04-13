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
//! │   ├── Execution Backend (see [`config::ExecutionRuntime`]: native default, optional container)
//! │   │   ├── NativeBackend (host processes; all platforms)
//! │   │   └── ContainerBackend (Docker/Podman; Linux, when configured)
//! │   └── Log Shipper      ← streams logs to controller
//! └── Signal Handler       ← SIGTERM/SIGINT → graceful drain
//! ```
#![allow(clippy::result_large_err)] // `AgentError` includes `tonic::Status`
#![allow(clippy::too_many_arguments)]

pub mod backend;
pub mod config;
pub mod error;
pub mod executor;
pub mod heartbeat;
pub mod job_claim;
mod output_drain;
mod output_seal;
pub mod process_watcher;
pub mod registration;
pub mod script_exec_hints;
pub mod seccomp_exec;
pub mod security;
pub mod step_log;
pub mod telemetry;
mod workflow_outputs;
mod workspace_archive;

pub use config::{AgentConfig, ExecutionRuntime, JoinTokenSource};
pub use error::{AgentError, Result};
pub use executor::JobExecutor;
pub use process_watcher::{
    ExecutedBinary, ExecutedBinaryRecord, JobExecutionMetadata, ProcessWatcher,
    compute_file_sha256, merge_execution_metadata,
};
pub use registration::{AgentRegistration, RegistrationSource, registration_needs_join_token};
