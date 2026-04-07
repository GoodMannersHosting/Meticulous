//! Log shipping, streaming, and aggregation for Meticulous CI/CD.
//!
//! This crate handles log capture from job executions, real-time streaming
//! to clients, archival to object storage, and secret redaction.
//!
//! # Architecture
//!
//! ```text
//! ┌────────────┐     ┌─────────────┐     ┌──────────────┐
//! │  Capture   │────▶│  Redaction  │────▶│  Aggregator  │
//! │  (stdout/  │     │  (secrets,  │     │  (buffer,    │
//! │   stderr)  │     │   patterns) │     │   broadcast) │
//! └────────────┘     └─────────────┘     └──────┬───────┘
//!                                               │
//!                    ┌──────────────────────────┼──────────────────────────┐
//!                    ▼                          ▼                          ▼
//!              ┌──────────┐              ┌──────────┐              ┌──────────┐
//!              │ WebSocket│              │  Object  │              │  Database│
//!              │ Clients  │              │  Store   │              │  (index) │
//!              └──────────┘              └──────────┘              └──────────┘
//! ```
//!
//! # Features
//!
//! - **Log Capture**: Streams stdout/stderr from container execution
//! - **Redaction**: Automatically redacts secrets from log output
//! - **Aggregation**: Buffers and broadcasts logs to multiple consumers
//! - **Archival**: Stores logs in object storage with compression
//! - **Streaming**: Real-time WebSocket delivery to clients
//!
//! # SBOM Support
//!
//! - Generate Software Bill of Materials from build artifacts
//! - Diff two SBOMs to show added/removed/changed dependencies
//! - Track tool versions and compute blast radius
//!
//! # Example
//!
//! ```ignore
//! use met_logging::{LogAggregator, Redactor, RedactorConfig};
//!
//! let redactor = Redactor::new(RedactorConfig::default());
//! redactor.add_secret("my-api-key");
//!
//! let aggregator = LogAggregator::new(redactor);
//! let mut rx = aggregator.subscribe();
//!
//! // Capture logs (from container stdout)
//! aggregator.capture_line("Building project...");
//! aggregator.capture_line("Using API key: my-api-key"); // Will be redacted
//!
//! // Receive processed logs
//! while let Some(line) = rx.recv().await {
//!     println!("{}", line.content); // "Using API key: [REDACTED]"
//! }
//! ```

pub mod aggregator;
pub mod archive_codec;
pub mod blast_radius;
pub mod capture;
pub mod redactor;
pub mod sbom;
pub mod shipper;
pub mod tools;

pub use aggregator::{LogAggregator, LogLine, LogSubscription};
pub use archive_codec::{
    ArchiveCodecError, ArchivedLogLine, gunzip_jsonl, gzip_jsonl, job_run_archive_key,
};
pub use blast_radius::{AffectedRun, BlastRadiusQuery, BlastRadiusResult};
pub use capture::{LogCapture, LogCaptureConfig, LogSource};
pub use redactor::{RedactionPattern, Redactor, RedactorConfig};
pub use sbom::{DiffEntry, DiffKind, Sbom, SbomComponent, SbomDiff, SbomFormat};
pub use shipper::{LogArchive, LogShipper, ShipperConfig};
pub use tools::{ToolRegistry, ToolUsage, TrackedTool};
