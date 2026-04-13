//! Run state machine and job state tracking.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use met_core::ids::{AgentId, JobId, JobRunId, RunId, StepRunId};
use met_core::models::{JobStatus, RunStatus};
use std::collections::hash_map::Entry;
use tokio::sync::RwLock;

use crate::workspace_snapshots::WorkspaceSnapshotRecord;

/// State of a single job within a run.
#[derive(Debug, Clone)]
pub struct JobState {
    pub job_id: JobId,
    pub job_run_id: JobRunId,
    pub name: String,
    pub status: JobStatus,
    pub agent_id: Option<AgentId>,
    pub attempt: i32,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub exit_code: Option<i32>,
    pub error_message: Option<String>,
    pub step_states: Vec<StepState>,
}

impl JobState {
    pub fn new(job_id: JobId, job_run_id: JobRunId, name: impl Into<String>) -> Self {
        Self {
            job_id,
            job_run_id,
            name: name.into(),
            status: JobStatus::Pending,
            agent_id: None,
            attempt: 0,
            started_at: None,
            finished_at: None,
            exit_code: None,
            error_message: None,
            step_states: Vec::new(),
        }
    }

    pub fn is_complete(&self) -> bool {
        self.status.is_terminal()
    }

    pub fn is_success(&self) -> bool {
        self.status.is_success()
    }
}

/// State of a single step within a job.
#[derive(Debug, Clone)]
pub struct StepState {
    pub step_run_id: StepRunId,
    pub name: String,
    pub status: JobStatus,
    pub exit_code: Option<i32>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

/// Complete state of a pipeline run.
#[derive(Clone)]
pub struct RunState {
    inner: Arc<RunStateInner>,
}

struct RunStateInner {
    pub run_id: RunId,
    status: RwLock<RunStatus>,
    started_at: RwLock<Option<DateTime<Utc>>>,
    finished_at: RwLock<Option<DateTime<Utc>>>,
    jobs: RwLock<HashMap<JobId, JobState>>,
    completed_jobs: RwLock<HashSet<JobId>>,
    failed_jobs: RwLock<HashSet<JobId>>,
    skipped_jobs: RwLock<HashSet<JobId>>,
    pending_jobs: RwLock<HashSet<JobId>>,
    running_jobs: RwLock<HashSet<JobId>>,
    cancellation_requested: RwLock<bool>,
    /// First successful dispatch pins `(affinity group string) -> agent` for the lifetime of the run.
    affinity_pins: RwLock<HashMap<String, AgentId>>,
    /// Passive workspace snapshots: producer [`JobId`] → digest and object key (ADR-014 extension).
    workspace_snapshots: RwLock<HashMap<JobId, WorkspaceSnapshotRecord>>,
    /// Monotonic generation counter for snapshot provenance within this run.
    workspace_snapshot_generation: RwLock<i32>,
}

impl RunState {
    /// Create a new run state.
    pub fn new(run_id: RunId) -> Self {
        Self {
            inner: Arc::new(RunStateInner {
                run_id,
                status: RwLock::new(RunStatus::Pending),
                started_at: RwLock::new(None),
                finished_at: RwLock::new(None),
                jobs: RwLock::new(HashMap::new()),
                completed_jobs: RwLock::new(HashSet::new()),
                failed_jobs: RwLock::new(HashSet::new()),
                skipped_jobs: RwLock::new(HashSet::new()),
                pending_jobs: RwLock::new(HashSet::new()),
                running_jobs: RwLock::new(HashSet::new()),
                cancellation_requested: RwLock::new(false),
                affinity_pins: RwLock::new(HashMap::new()),
                workspace_snapshots: RwLock::new(HashMap::new()),
                workspace_snapshot_generation: RwLock::new(0),
            }),
        }
    }

    /// Next snapshot generation number for this run (producer registration).
    pub async fn next_workspace_snapshot_generation(&self) -> i32 {
        let mut g = self.inner.workspace_snapshot_generation.write().await;
        *g += 1;
        *g
    }

    /// Store snapshot metadata after a producer job uploads successfully.
    pub async fn put_workspace_snapshot(
        &self,
        producer_job_id: JobId,
        record: WorkspaceSnapshotRecord,
    ) {
        self.inner
            .workspace_snapshots
            .write()
            .await
            .insert(producer_job_id, record);
    }

    /// Lookup snapshot metadata from a completed producer job.
    pub async fn get_workspace_snapshot_record(
        &self,
        producer_job_id: &JobId,
    ) -> Option<WorkspaceSnapshotRecord> {
        self.inner
            .workspace_snapshots
            .read()
            .await
            .get(producer_job_id)
            .cloned()
    }

    /// Snapshot records (read-only) for dispatch validation.
    pub async fn workspace_snapshot_records(&self) -> HashMap<JobId, WorkspaceSnapshotRecord> {
        self.inner.workspace_snapshots.read().await.clone()
    }

    /// Resolved agent for an affinity group, if already pinned.
    pub async fn get_affinity_pin(&self, group: &str) -> Option<AgentId> {
        self.inner.affinity_pins.read().await.get(group).copied()
    }

    /// After a successful dispatch, record or verify the affinity pin for this group.
    pub async fn ensure_affinity_pin(
        &self,
        group: impl Into<String>,
        agent: AgentId,
    ) -> Result<(), crate::error::EngineError> {
        let group = group.into();
        let mut pins = self.inner.affinity_pins.write().await;
        match pins.entry(group) {
            Entry::Vacant(e) => {
                e.insert(agent);
                Ok(())
            }
            Entry::Occupied(e) => {
                if *e.get() == agent {
                    Ok(())
                } else {
                    Err(crate::error::EngineError::Internal(format!(
                        "affinity group {:?} pinned to a different agent than dispatch target",
                        e.key()
                    )))
                }
            }
        }
    }

    pub fn run_id(&self) -> RunId {
        self.inner.run_id
    }

    pub async fn status(&self) -> RunStatus {
        *self.inner.status.read().await
    }

    pub async fn set_status(&self, status: RunStatus) {
        let mut s = self.inner.status.write().await;
        *s = status;

        if status == RunStatus::Running && self.inner.started_at.read().await.is_none() {
            *self.inner.started_at.write().await = Some(Utc::now());
        }

        if status.is_terminal() {
            *self.inner.finished_at.write().await = Some(Utc::now());
        }
    }

    pub async fn started_at(&self) -> Option<DateTime<Utc>> {
        *self.inner.started_at.read().await
    }

    pub async fn finished_at(&self) -> Option<DateTime<Utc>> {
        *self.inner.finished_at.read().await
    }

    /// Register a job for tracking.
    pub async fn register_job(&self, job_state: JobState) {
        let job_id = job_state.job_id;
        self.inner.jobs.write().await.insert(job_id, job_state);
        self.inner.pending_jobs.write().await.insert(job_id);
    }

    /// Get job state by JobId.
    pub async fn get_job(&self, job_id: &JobId) -> Option<JobState> {
        self.inner.jobs.read().await.get(job_id).cloned()
    }

    /// Get job state by JobRunId.
    pub async fn get_job_by_run_id(&self, job_run_id: JobRunId) -> Option<JobState> {
        self.inner
            .jobs
            .read()
            .await
            .values()
            .find(|j| j.job_run_id == job_run_id)
            .cloned()
    }

    /// Update job state.
    pub async fn update_job(&self, job_id: &JobId, f: impl FnOnce(&mut JobState)) {
        if let Some(job) = self.inner.jobs.write().await.get_mut(job_id) {
            let old_status = job.status;
            f(job);
            let new_status = job.status;

            if old_status != new_status {
                self.update_job_sets(job_id, old_status, new_status).await;
            }
        }
    }

    async fn update_job_sets(&self, job_id: &JobId, _old: JobStatus, new: JobStatus) {
        self.inner.pending_jobs.write().await.remove(job_id);
        self.inner.running_jobs.write().await.remove(job_id);

        match new {
            JobStatus::Pending => {
                self.inner.pending_jobs.write().await.insert(*job_id);
            }
            JobStatus::Queued | JobStatus::Running => {
                self.inner.running_jobs.write().await.insert(*job_id);
            }
            JobStatus::Succeeded => {
                self.inner.completed_jobs.write().await.insert(*job_id);
            }
            JobStatus::Failed | JobStatus::TimedOut | JobStatus::Cancelled => {
                self.inner.failed_jobs.write().await.insert(*job_id);
            }
            JobStatus::Skipped => {
                self.inner.skipped_jobs.write().await.insert(*job_id);
            }
        }
    }

    /// Mark a job as queued.
    pub async fn mark_job_queued(&self, job_id: &JobId) {
        self.update_job(job_id, |job| {
            job.status = JobStatus::Queued;
        })
        .await;
    }

    /// Mark a job as running.
    pub async fn mark_job_running(&self, job_id: &JobId, agent_id: AgentId, attempt: i32) {
        self.update_job(job_id, |job| {
            job.status = JobStatus::Running;
            job.agent_id = Some(agent_id);
            job.attempt = attempt;
            job.started_at = Some(Utc::now());
        })
        .await;
    }

    /// Mark a job as completed.
    pub async fn mark_job_completed(
        &self,
        job_id: &JobId,
        success: bool,
        exit_code: Option<i32>,
        error_message: Option<String>,
    ) {
        self.update_job(job_id, |job| {
            job.status = if success {
                JobStatus::Succeeded
            } else {
                JobStatus::Failed
            };
            job.exit_code = exit_code;
            job.error_message = error_message;
            job.finished_at = Some(Utc::now());
        })
        .await;
    }

    /// Mark a job as skipped.
    pub async fn mark_job_skipped(&self, job_id: &JobId, reason: Option<String>) {
        self.update_job(job_id, |job| {
            job.status = JobStatus::Skipped;
            job.error_message = reason;
            job.finished_at = Some(Utc::now());
        })
        .await;
    }

    /// Mark a job as timed out.
    pub async fn mark_job_timed_out(&self, job_id: &JobId) {
        self.update_job(job_id, |job| {
            job.status = JobStatus::TimedOut;
            job.error_message = Some("Job execution timed out".to_string());
            job.finished_at = Some(Utc::now());
        })
        .await;
    }

    /// Mark a job as cancelled.
    pub async fn mark_job_cancelled(&self, job_id: &JobId, reason: Option<String>) {
        self.update_job(job_id, |job| {
            job.status = JobStatus::Cancelled;
            job.error_message = Some(reason.unwrap_or_else(|| "Job was cancelled".to_string()));
            job.finished_at = Some(Utc::now());
        })
        .await;
    }

    /// Check if a job is complete.
    pub async fn is_job_complete(&self, job_id: &JobId) -> bool {
        self.inner.completed_jobs.read().await.contains(job_id)
            || self.inner.failed_jobs.read().await.contains(job_id)
            || self.inner.skipped_jobs.read().await.contains(job_id)
    }

    /// Check if a job succeeded.
    pub async fn is_job_success(&self, job_id: &JobId) -> bool {
        self.inner.completed_jobs.read().await.contains(job_id)
            || self.inner.skipped_jobs.read().await.contains(job_id)
    }

    /// Get pending job IDs.
    pub async fn pending_jobs(&self) -> HashSet<JobId> {
        self.inner.pending_jobs.read().await.clone()
    }

    /// Get running job IDs.
    pub async fn running_jobs(&self) -> HashSet<JobId> {
        self.inner.running_jobs.read().await.clone()
    }

    /// Get completed (successful) job IDs.
    pub async fn completed_jobs(&self) -> HashSet<JobId> {
        self.inner.completed_jobs.read().await.clone()
    }

    /// Get failed job IDs.
    pub async fn failed_jobs(&self) -> HashSet<JobId> {
        self.inner.failed_jobs.read().await.clone()
    }

    /// Get all job states.
    pub async fn all_jobs(&self) -> HashMap<JobId, JobState> {
        self.inner.jobs.read().await.clone()
    }

    /// Check if all jobs are complete.
    pub async fn is_complete(&self) -> bool {
        self.inner.pending_jobs.read().await.is_empty()
            && self.inner.running_jobs.read().await.is_empty()
    }

    /// Check if the run has failed (any non-skipped job failed).
    pub async fn has_failures(&self) -> bool {
        !self.inner.failed_jobs.read().await.is_empty()
    }

    /// Request cancellation.
    pub async fn request_cancellation(&self) {
        *self.inner.cancellation_requested.write().await = true;
    }

    /// Check if cancellation was requested.
    pub async fn is_cancellation_requested(&self) -> bool {
        *self.inner.cancellation_requested.read().await
    }

    /// Compute the final run status based on job states.
    pub async fn compute_final_status(&self) -> RunStatus {
        if *self.inner.cancellation_requested.read().await {
            return RunStatus::Cancelled;
        }

        let failed = self.inner.failed_jobs.read().await;
        if !failed.is_empty() {
            let jobs = self.inner.jobs.read().await;
            for job_id in failed.iter() {
                if let Some(job) = jobs.get(job_id) {
                    if job.status == JobStatus::TimedOut {
                        return RunStatus::TimedOut;
                    }
                }
            }
            return RunStatus::Failed;
        }

        RunStatus::Succeeded
    }
}
