//! Event broadcasting for pipeline execution.

use async_nats::jetstream::{self, Context as JetStreamContext};
use met_core::events::{
    EventEnvelope, JobCompleted, JobDispatched, JobStarted, RunCompleted, RunQueued, RunStarted,
    StepCompleted, StepStarted, kinds,
};
use met_core::ids::{AgentId, JobRunId, PipelineId, RunId, StepRunId};
use tracing::{debug, instrument};

use crate::error::Result;

/// NATS subjects for engine events.
pub mod subjects {
    use met_core::ids::PipelineId;

    pub fn run_events(pipeline_id: PipelineId) -> String {
        format!("met.runs.{}", pipeline_id.as_uuid())
    }

    /// Must **not** use `met.jobs.events.*`: the `JOBS` stream owns `met.jobs.>` (controller), and
    /// JetStream rejects overlapping stream subjects (error 10065).
    pub fn job_events(pipeline_id: PipelineId) -> String {
        format!("met.events.jobs.{}", pipeline_id.as_uuid())
    }

    pub fn step_events(pipeline_id: PipelineId) -> String {
        format!("met.steps.events.{}", pipeline_id.as_uuid())
    }

    pub const EVENTS_STREAM: &str = "EVENTS";
}

/// Event broadcaster for publishing pipeline execution events.
#[derive(Clone)]
pub struct EventBroadcaster {
    jetstream: JetStreamContext,
    source: String,
}

impl EventBroadcaster {
    /// Create a new event broadcaster.
    pub fn new(jetstream: JetStreamContext) -> Self {
        Self {
            jetstream,
            source: "met-engine".to_string(),
        }
    }

    /// Ensure the events stream exists.
    pub async fn ensure_stream(&self) -> Result<()> {
        let config = jetstream::stream::Config {
            name: subjects::EVENTS_STREAM.to_string(),
            subjects: vec![
                "met.runs.>".to_string(),
                "met.events.jobs.>".to_string(),
                "met.steps.events.>".to_string(),
            ],
            retention: jetstream::stream::RetentionPolicy::Limits,
            max_age: std::time::Duration::from_secs(7 * 24 * 60 * 60), // 7 days
            storage: jetstream::stream::StorageType::File,
            ..Default::default()
        };

        match self.jetstream.get_stream(subjects::EVENTS_STREAM).await {
            Ok(_) => {
                self.jetstream
                    .update_stream(&config)
                    .await
                    .map_err(|e| crate::error::EngineError::Nats(e.to_string()))?;
                debug!("updated events stream config");
            }
            Err(_) => {
                self.jetstream
                    .create_stream(config)
                    .await
                    .map_err(|e| crate::error::EngineError::Nats(e.to_string()))?;
                debug!("created events stream");
            }
        }

        Ok(())
    }

    async fn publish<T: serde::Serialize>(
        &self,
        subject: &str,
        event: EventEnvelope<T>,
    ) -> Result<()> {
        let payload = event
            .to_bytes()
            .map_err(crate::error::EngineError::Serialization)?;

        self.jetstream
            .publish(subject.to_string(), payload.into())
            .await
            .map_err(|e| crate::error::EngineError::Nats(e.to_string()))?;

        Ok(())
    }

    /// Publish a run queued event.
    #[instrument(skip(self), fields(run_id = %run_id, pipeline_id = %pipeline_id))]
    pub async fn run_queued(
        &self,
        run_id: RunId,
        pipeline_id: PipelineId,
        triggered_by: &str,
        trace_id: Option<&str>,
    ) -> Result<()> {
        let event = EventEnvelope::new(
            kinds::RUN_QUEUED,
            &self.source,
            RunQueued {
                run_id,
                pipeline_id,
                triggered_by: triggered_by.to_string(),
            },
        );
        let event = if let Some(tid) = trace_id {
            event.with_trace_id(tid)
        } else {
            event
        };

        let subject = subjects::run_events(pipeline_id);
        self.publish(&subject, event).await?;
        debug!("published run.queued event");
        Ok(())
    }

    /// Publish a run started event.
    #[instrument(skip(self), fields(run_id = %run_id, pipeline_id = %pipeline_id))]
    pub async fn run_started(
        &self,
        run_id: RunId,
        pipeline_id: PipelineId,
        trace_id: Option<&str>,
    ) -> Result<()> {
        let event = EventEnvelope::new(
            kinds::RUN_STARTED,
            &self.source,
            RunStarted {
                run_id,
                pipeline_id,
            },
        );
        let event = if let Some(tid) = trace_id {
            event.with_trace_id(tid)
        } else {
            event
        };

        let subject = subjects::run_events(pipeline_id);
        self.publish(&subject, event).await?;
        debug!("published run.started event");
        Ok(())
    }

    /// Publish a run completed event.
    #[instrument(skip(self), fields(run_id = %run_id, pipeline_id = %pipeline_id))]
    pub async fn run_completed(
        &self,
        run_id: RunId,
        pipeline_id: PipelineId,
        success: bool,
        duration_ms: u64,
        trace_id: Option<&str>,
    ) -> Result<()> {
        let event = EventEnvelope::new(
            kinds::RUN_COMPLETED,
            &self.source,
            RunCompleted {
                run_id,
                pipeline_id,
                success,
                duration_ms,
            },
        );
        let event = if let Some(tid) = trace_id {
            event.with_trace_id(tid)
        } else {
            event
        };

        let subject = subjects::run_events(pipeline_id);
        self.publish(&subject, event).await?;
        debug!(success, duration_ms, "published run.completed event");
        Ok(())
    }

    /// Publish a job dispatched event.
    #[instrument(skip(self), fields(job_run_id = %job_run_id, run_id = %run_id))]
    pub async fn job_dispatched(
        &self,
        job_run_id: JobRunId,
        run_id: RunId,
        pipeline_id: PipelineId,
        agent_id: AgentId,
        job_name: &str,
        trace_id: Option<&str>,
    ) -> Result<()> {
        let event = EventEnvelope::new(
            kinds::JOB_DISPATCHED,
            &self.source,
            JobDispatched {
                job_run_id,
                run_id,
                agent_id,
                job_name: job_name.to_string(),
            },
        );
        let event = if let Some(tid) = trace_id {
            event.with_trace_id(tid)
        } else {
            event
        };

        let subject = subjects::job_events(pipeline_id);
        self.publish(&subject, event).await?;
        debug!(job_name, "published job.dispatched event");
        Ok(())
    }

    /// Publish a job started event.
    #[instrument(skip(self), fields(job_run_id = %job_run_id, run_id = %run_id))]
    pub async fn job_started(
        &self,
        job_run_id: JobRunId,
        run_id: RunId,
        pipeline_id: PipelineId,
        agent_id: AgentId,
        trace_id: Option<&str>,
    ) -> Result<()> {
        let event = EventEnvelope::new(
            kinds::JOB_STARTED,
            &self.source,
            JobStarted {
                job_run_id,
                run_id,
                agent_id,
            },
        );
        let event = if let Some(tid) = trace_id {
            event.with_trace_id(tid)
        } else {
            event
        };

        let subject = subjects::job_events(pipeline_id);
        self.publish(&subject, event).await?;
        debug!("published job.started event");
        Ok(())
    }

    /// Publish a job completed event.
    #[instrument(skip(self), fields(job_run_id = %job_run_id, run_id = %run_id))]
    pub async fn job_completed(
        &self,
        job_run_id: JobRunId,
        run_id: RunId,
        pipeline_id: PipelineId,
        agent_id: AgentId,
        success: bool,
        exit_code: Option<i32>,
        duration_ms: u64,
        trace_id: Option<&str>,
    ) -> Result<()> {
        let event = EventEnvelope::new(
            kinds::JOB_COMPLETED,
            &self.source,
            JobCompleted {
                job_run_id,
                run_id,
                agent_id,
                success,
                exit_code,
                duration_ms,
            },
        );
        let event = if let Some(tid) = trace_id {
            event.with_trace_id(tid)
        } else {
            event
        };

        let subject = subjects::job_events(pipeline_id);
        self.publish(&subject, event).await?;
        debug!(
            success,
            exit_code, duration_ms, "published job.completed event"
        );
        Ok(())
    }

    /// Publish a step started event.
    #[instrument(skip(self), fields(step_run_id = %step_run_id, job_run_id = %job_run_id))]
    pub async fn step_started(
        &self,
        step_run_id: StepRunId,
        job_run_id: JobRunId,
        pipeline_id: PipelineId,
        step_name: &str,
        trace_id: Option<&str>,
    ) -> Result<()> {
        let event = EventEnvelope::new(
            kinds::STEP_STARTED,
            &self.source,
            StepStarted {
                step_run_id,
                job_run_id,
                step_name: step_name.to_string(),
            },
        );
        let event = if let Some(tid) = trace_id {
            event.with_trace_id(tid)
        } else {
            event
        };

        let subject = subjects::step_events(pipeline_id);
        self.publish(&subject, event).await?;
        debug!(step_name, "published step.started event");
        Ok(())
    }

    /// Publish a step completed event.
    #[instrument(skip(self), fields(step_run_id = %step_run_id, job_run_id = %job_run_id))]
    pub async fn step_completed(
        &self,
        step_run_id: StepRunId,
        job_run_id: JobRunId,
        pipeline_id: PipelineId,
        success: bool,
        exit_code: Option<i32>,
        duration_ms: u64,
        trace_id: Option<&str>,
    ) -> Result<()> {
        let event = EventEnvelope::new(
            kinds::STEP_COMPLETED,
            &self.source,
            StepCompleted {
                step_run_id,
                job_run_id,
                success,
                exit_code,
                duration_ms,
            },
        );
        let event = if let Some(tid) = trace_id {
            event.with_trace_id(tid)
        } else {
            event
        };

        let subject = subjects::step_events(pipeline_id);
        self.publish(&subject, event).await?;
        debug!(
            success,
            exit_code, duration_ms, "published step.completed event"
        );
        Ok(())
    }

    /// Publish a log chunk event for real-time streaming.
    #[instrument(skip(self, content), fields(job_run_id = %job_run_id))]
    pub async fn log_chunk(
        &self,
        job_run_id: JobRunId,
        step_run_id: Option<StepRunId>,
        content: &str,
    ) -> Result<()> {
        #[derive(serde::Serialize)]
        struct LogChunkEvent {
            job_run_id: JobRunId,
            step_run_id: Option<StepRunId>,
            content: String,
        }

        let event = EventEnvelope::new(
            "log.chunk",
            &self.source,
            LogChunkEvent {
                job_run_id,
                step_run_id,
                content: content.to_string(),
            },
        );

        let subject = format!("met.logs.{}", job_run_id.as_uuid());
        self.publish(&subject, event).await?;
        Ok(())
    }

    /// Publish a run cancelled event.
    #[instrument(skip(self), fields(run_id = %run_id, pipeline_id = %pipeline_id))]
    pub async fn run_cancelled(
        &self,
        run_id: RunId,
        pipeline_id: PipelineId,
        cancelled_by: Option<&str>,
        trace_id: Option<&str>,
    ) -> Result<()> {
        #[derive(serde::Serialize)]
        struct RunCancelled {
            run_id: RunId,
            pipeline_id: PipelineId,
            cancelled_by: Option<String>,
        }

        let event = EventEnvelope::new(
            "run.cancelled",
            &self.source,
            RunCancelled {
                run_id,
                pipeline_id,
                cancelled_by: cancelled_by.map(|s| s.to_string()),
            },
        );
        let event = if let Some(tid) = trace_id {
            event.with_trace_id(tid)
        } else {
            event
        };

        let subject = subjects::run_events(pipeline_id);
        self.publish(&subject, event).await?;
        debug!("published run.cancelled event");
        Ok(())
    }
}

/// Null broadcaster for testing without NATS.
pub struct NullBroadcaster;

impl NullBroadcaster {
    pub async fn run_queued(
        &self,
        _: RunId,
        _: PipelineId,
        _: &str,
        _: Option<&str>,
    ) -> Result<()> {
        Ok(())
    }
    pub async fn run_started(&self, _: RunId, _: PipelineId, _: Option<&str>) -> Result<()> {
        Ok(())
    }
    pub async fn run_completed(
        &self,
        _: RunId,
        _: PipelineId,
        _: bool,
        _: u64,
        _: Option<&str>,
    ) -> Result<()> {
        Ok(())
    }
}
