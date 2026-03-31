//! Event envelope types for NATS messaging.
//!
//! All events in Meticulous are wrapped in a standard envelope that provides
//! metadata for tracing, deduplication, and routing.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ids::{AgentId, JobRunId, PipelineId, RunId, StepRunId};

/// A typed wrapper for events published to NATS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope<T> {
    /// Unique event ID for deduplication.
    pub id: Uuid,
    /// When the event was created.
    pub timestamp: DateTime<Utc>,
    /// Event type discriminator.
    pub kind: String,
    /// Source component that emitted the event.
    pub source: String,
    /// Trace context for distributed tracing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    /// The actual event payload.
    pub payload: T,
}

impl<T> EventEnvelope<T> {
    /// Create a new event envelope with the given kind and payload.
    pub fn new(kind: impl Into<String>, source: impl Into<String>, payload: T) -> Self {
        Self {
            id: Uuid::now_v7(),
            timestamp: Utc::now(),
            kind: kind.into(),
            source: source.into(),
            trace_id: None,
            payload,
        }
    }

    /// Set the trace ID for distributed tracing.
    #[must_use]
    pub fn with_trace_id(mut self, trace_id: impl Into<String>) -> Self {
        self.trace_id = Some(trace_id.into());
        self
    }
}

impl<T: Serialize> EventEnvelope<T> {
    /// Serialize the envelope to JSON bytes for NATS.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }
}

impl<T: for<'de> Deserialize<'de>> EventEnvelope<T> {
    /// Deserialize an envelope from JSON bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if deserialization fails.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

// ============================================================================
// Pipeline Events
// ============================================================================

/// Event indicating a pipeline run has been queued.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunQueued {
    /// The run that was queued.
    pub run_id: RunId,
    /// The pipeline being run.
    pub pipeline_id: PipelineId,
    /// Who or what triggered the run.
    pub triggered_by: String,
}

/// Event indicating a pipeline run has started execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunStarted {
    /// The run that started.
    pub run_id: RunId,
    /// The pipeline being run.
    pub pipeline_id: PipelineId,
}

/// Event indicating a pipeline run has completed (success or failure).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunCompleted {
    /// The run that completed.
    pub run_id: RunId,
    /// The pipeline that was run.
    pub pipeline_id: PipelineId,
    /// Whether the run succeeded.
    pub success: bool,
    /// Duration in milliseconds.
    pub duration_ms: u64,
}

// ============================================================================
// Job Events
// ============================================================================

/// Event indicating a job has been dispatched to an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobDispatched {
    /// The job run being dispatched.
    pub job_run_id: JobRunId,
    /// The parent run.
    pub run_id: RunId,
    /// The agent that will execute the job.
    pub agent_id: AgentId,
    /// The job name.
    pub job_name: String,
}

/// Event indicating a job has started execution on an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobStarted {
    /// The job run that started.
    pub job_run_id: JobRunId,
    /// The run it belongs to.
    pub run_id: RunId,
    /// The agent executing the job.
    pub agent_id: AgentId,
}

/// Event indicating a job has completed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobCompleted {
    /// The job run that completed.
    pub job_run_id: JobRunId,
    /// The run it belongs to.
    pub run_id: RunId,
    /// The agent that executed the job.
    pub agent_id: AgentId,
    /// Whether the job succeeded.
    pub success: bool,
    /// Exit code (if applicable).
    pub exit_code: Option<i32>,
    /// Duration in milliseconds.
    pub duration_ms: u64,
}

// ============================================================================
// Step Events
// ============================================================================

/// Event indicating a step has started.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepStarted {
    /// The step run that started.
    pub step_run_id: StepRunId,
    /// The parent job run.
    pub job_run_id: JobRunId,
    /// The step name.
    pub step_name: String,
}

/// Event indicating a step has completed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepCompleted {
    /// The step run that completed.
    pub step_run_id: StepRunId,
    /// The parent job run.
    pub job_run_id: JobRunId,
    /// Whether the step succeeded.
    pub success: bool,
    /// Exit code (if applicable).
    pub exit_code: Option<i32>,
    /// Duration in milliseconds.
    pub duration_ms: u64,
}

// ============================================================================
// Agent Events
// ============================================================================

/// Event indicating an agent has registered with the controller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRegistered {
    /// The agent that registered.
    pub agent_id: AgentId,
    /// The agent's hostname.
    pub hostname: String,
    /// Operating system.
    pub os: String,
    /// Architecture.
    pub arch: String,
    /// Agent pool membership.
    pub pool: Option<String>,
    /// Agent tags.
    pub tags: Vec<String>,
}

/// Event indicating an agent's heartbeat.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentHeartbeat {
    /// The agent sending the heartbeat.
    pub agent_id: AgentId,
    /// Number of jobs currently running.
    pub running_jobs: u32,
    /// Available capacity (slots).
    pub available_capacity: u32,
}

/// Event indicating an agent has gone offline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentOffline {
    /// The agent that went offline.
    pub agent_id: AgentId,
    /// Reason for going offline (if known).
    pub reason: Option<String>,
}

// ============================================================================
// Event Kind Constants
// ============================================================================

/// Well-known event kinds for routing and filtering.
pub mod kinds {
    pub const RUN_QUEUED: &str = "run.queued";
    pub const RUN_STARTED: &str = "run.started";
    pub const RUN_COMPLETED: &str = "run.completed";

    pub const JOB_DISPATCHED: &str = "job.dispatched";
    pub const JOB_STARTED: &str = "job.started";
    pub const JOB_COMPLETED: &str = "job.completed";

    pub const STEP_STARTED: &str = "step.started";
    pub const STEP_COMPLETED: &str = "step.completed";

    pub const AGENT_REGISTERED: &str = "agent.registered";
    pub const AGENT_HEARTBEAT: &str = "agent.heartbeat";
    pub const AGENT_OFFLINE: &str = "agent.offline";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_envelope_serialization() {
        let event = EventEnvelope::new(
            kinds::RUN_QUEUED,
            "met-engine",
            RunQueued {
                run_id: RunId::new(),
                pipeline_id: PipelineId::new(),
                triggered_by: "manual".to_string(),
            },
        );

        let bytes = event.to_bytes().unwrap();
        let parsed: EventEnvelope<RunQueued> = EventEnvelope::from_bytes(&bytes).unwrap();

        assert_eq!(event.id, parsed.id);
        assert_eq!(event.kind, parsed.kind);
    }

    #[test]
    fn test_event_envelope_with_trace() {
        let event = EventEnvelope::new(
            kinds::JOB_STARTED,
            "met-agent",
            JobStarted {
                job_run_id: JobRunId::new(),
                run_id: RunId::new(),
                agent_id: AgentId::new(),
            },
        )
        .with_trace_id("trace-123");

        assert_eq!(event.trace_id, Some("trace-123".to_string()));
    }
}
