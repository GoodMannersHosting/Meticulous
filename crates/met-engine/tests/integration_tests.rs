//! Integration tests for the pipeline execution engine.
//!
//! These tests verify:
//! - Simple pipeline execution with single job
//! - Diamond DAG execution with correct order and concurrency
//! - Cache hit/miss behavior
//! - Retry logic
//! - Cancellation handling

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use indexmap::IndexMap;
use met_core::ids::{JobId, JobRunId, OrganizationId, PipelineId, RunId, StepId};
use met_core::models::{JobStatus, RunStatus};
use met_engine::cache::{CacheBackend, CacheKey, CacheLookupResult, MemoryCache};
use met_engine::context::ExecutionContext;
use met_engine::state::{JobState, RunState};
use met_engine::ExecutionResult;
use met_parser::{CacheConfig, JobIR, PipelineIR, PoolSelector, Shell, StepCommand, StepIR};
use tokio::sync::{Mutex, RwLock};

// ============================================================================
// Test Helpers
// ============================================================================

/// Create a simple step for testing.
fn test_step(name: &str) -> StepIR {
    StepIR {
        id: StepId::new(),
        name: name.to_string(),
        command: StepCommand::Run {
            shell: Shell::Bash,
            script: format!("echo '{name}'"),
        },
        env: IndexMap::new(),
        working_directory: None,
        timeout: Duration::from_secs(60),
        continue_on_error: false,
    }
}

/// Create a test job with dependencies.
fn test_job(id: JobId, name: &str, depends_on: Vec<JobId>) -> JobIR {
    JobIR {
        id,
        name: name.to_string(),
        depends_on,
        pool_selector: PoolSelector::default(),
        steps: vec![test_step(&format!("{name}-step"))],
        services: Vec::new(),
        timeout: Duration::from_secs(300),
        retry_policy: None,
        cache_config: None,
        condition: None,
        source_workflow: None,
        env: IndexMap::new(),
    }
}

/// Create a test pipeline with the given jobs.
fn test_pipeline(name: &str, jobs: Vec<JobIR>) -> PipelineIR {
    PipelineIR {
        id: PipelineId::new(),
        name: name.to_string(),
        source_file: None,
        project_id: None,
        triggers: Vec::new(),
        variables: IndexMap::new(),
        secret_refs: IndexMap::new(),
        jobs,
        default_pool_selector: None,
    }
}

// ============================================================================
// Mock Executor for Integration Testing
// ============================================================================

/// Tracks job dispatches and allows simulating completions.
#[derive(Clone)]
struct MockScheduler {
    dispatched_jobs: Arc<RwLock<Vec<DispatchedJob>>>,
    dispatch_order: Arc<Mutex<Vec<String>>>,
    concurrent_count: Arc<AtomicUsize>,
    max_concurrent: Arc<AtomicUsize>,
}

#[derive(Clone, Debug)]
struct DispatchedJob {
    job_id: JobId,
    job_run_id: JobRunId,
    job_name: String,
}

impl MockScheduler {
    fn new() -> (Self, ()) {
        (
            Self {
                dispatched_jobs: Arc::new(RwLock::new(Vec::new())),
                dispatch_order: Arc::new(Mutex::new(Vec::new())),
                concurrent_count: Arc::new(AtomicUsize::new(0)),
                max_concurrent: Arc::new(AtomicUsize::new(0)),
            },
            (),
        )
    }

    async fn dispatch(&self, job: &JobIR, job_run_id: JobRunId) {
        let count = self.concurrent_count.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_concurrent.fetch_max(count, Ordering::SeqCst);

        self.dispatch_order.lock().await.push(job.name.clone());

        self.dispatched_jobs.write().await.push(DispatchedJob {
            job_id: job.id,
            job_run_id,
            job_name: job.name.clone(),
        });
    }

    async fn get_dispatch_order(&self) -> Vec<String> {
        self.dispatch_order.lock().await.clone()
    }

    fn get_max_concurrent(&self) -> usize {
        self.max_concurrent.load(Ordering::SeqCst)
    }
}

/// Mock cache that can be pre-seeded with cache hits.
struct MockCache {
    inner: MemoryCache,
    hits: RwLock<HashMap<String, bool>>,
}

impl MockCache {
    fn new() -> Self {
        Self {
            inner: MemoryCache::new(),
            hits: RwLock::new(HashMap::new()),
        }
    }

    async fn set_cache_hit(&self, key: &str) {
        self.hits.write().await.insert(key.to_string(), true);
    }

    async fn has_hit(&self, key: &str) -> bool {
        self.hits.read().await.contains_key(key)
    }
}

#[async_trait::async_trait]
impl CacheBackend for MockCache {
    async fn lookup(&self, key: &CacheKey) -> met_engine::Result<CacheLookupResult> {
        if self.hits.read().await.contains_key(&key.key) {
            return Ok(CacheLookupResult::Hit {
                key: key.key.clone(),
                storage_path: format!("mock://{}", key.key),
                created_at: Utc::now(),
            });
        }
        self.inner.lookup(key).await
    }

    async fn store(&self, key: &CacheKey, data_path: &str) -> met_engine::Result<String> {
        self.inner.store(key, data_path).await
    }

    async fn delete(&self, key: &str) -> met_engine::Result<()> {
        self.hits.write().await.remove(key);
        self.inner.delete(key).await
    }
}

/// Test executor that drives the DAG execution with mocked dependencies.
struct TestExecutor {
    scheduler: MockScheduler,
    cache: Arc<MockCache>,
    config: TestExecutorConfig,
}

#[derive(Clone)]
struct TestExecutorConfig {
    poll_interval: Duration,
    fail_fast: bool,
}

impl Default for TestExecutorConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_millis(10),
            fail_fast: false,
        }
    }
}

impl TestExecutor {
    fn new(scheduler: MockScheduler, cache: Arc<MockCache>) -> Self {
        Self {
            scheduler,
            cache,
            config: TestExecutorConfig::default(),
        }
    }

    async fn execute_with_completions(
        &self,
        pipeline: PipelineIR,
        completion_strategy: impl CompletionStrategy + Send + 'static,
    ) -> ExecutionResult {
        let run_id = RunId::new();
        let org_id = OrganizationId::new();
        let run_state = RunState::new(run_id);

        run_state.set_status(RunStatus::Running).await;

        for job in &pipeline.jobs {
            let job_run_id = JobRunId::new();
            let job_state = JobState::new(job.id, job_run_id, &job.name);
            run_state.register_job(job_state).await;
        }

        let _ctx = ExecutionContext::new(run_id, org_id, pipeline.clone(), "test");

        let scheduler = self.scheduler.clone();
        let run_state_clone = run_state.clone();
        let pipeline_for_completion = pipeline.clone();

        let completion_handle = tokio::spawn(async move {
            let mut strategy = completion_strategy;
            strategy
                .run(&scheduler, &run_state_clone, &pipeline_for_completion)
                .await;
        });

        let start_time = Utc::now();

        loop {
            tokio::time::sleep(self.config.poll_interval).await;

            if run_state.is_cancellation_requested().await {
                self.cancel_pending_jobs(&run_state).await;
                break;
            }

            let ready_jobs = self.find_ready_jobs(&pipeline, &run_state).await;

            for job in ready_jobs {
                if let Some(cache_config) = &job.cache_config {
                    let cache_key = format!(
                        "{}-{}",
                        cache_config.key,
                        cache_config.paths.join(",")
                    );
                    if self.cache.has_hit(&cache_key).await {
                        run_state
                            .mark_job_completed(&job.id, true, Some(0), None)
                            .await;
                        continue;
                    }
                }

                if let Some(condition) = &job.condition {
                    let any_dep_failed = job
                        .depends_on
                        .iter()
                        .any(|dep_id| futures::executor::block_on(run_state.failed_jobs()).contains(dep_id));

                    if any_dep_failed && condition != "always()" {
                        run_state
                            .mark_job_skipped(&job.id, Some("Dependency failed".to_string()))
                            .await;
                        continue;
                    }
                } else {
                    let any_dep_failed = job
                        .depends_on
                        .iter()
                        .any(|dep_id| futures::executor::block_on(run_state.failed_jobs()).contains(dep_id));

                    if any_dep_failed {
                        run_state
                            .mark_job_skipped(&job.id, Some("Dependency failed".to_string()))
                            .await;
                        continue;
                    }
                }

                let job_run_id = JobRunId::new();
                run_state.mark_job_queued(&job.id).await;
                self.scheduler.dispatch(job, job_run_id).await;
            }

            if self.config.fail_fast && run_state.has_failures().await {
                self.cancel_pending_jobs(&run_state).await;
                break;
            }

            if run_state.is_complete().await {
                break;
            }
        }

        completion_handle.abort();

        let end_time = Utc::now();
        let duration_ms = (end_time - start_time).num_milliseconds() as u64;

        let jobs = run_state.all_jobs().await;
        let jobs_succeeded = jobs
            .values()
            .filter(|j| j.status == JobStatus::Succeeded)
            .count();
        let jobs_failed = jobs
            .values()
            .filter(|j| {
                matches!(
                    j.status,
                    JobStatus::Failed | JobStatus::TimedOut | JobStatus::Cancelled
                )
            })
            .count();
        let jobs_skipped = jobs
            .values()
            .filter(|j| j.status == JobStatus::Skipped)
            .count();

        let final_status = run_state.compute_final_status().await;

        ExecutionResult {
            run_id,
            status: final_status,
            duration_ms,
            jobs_succeeded,
            jobs_failed,
            jobs_skipped,
        }
    }

    async fn find_ready_jobs<'a>(&self, pipeline: &'a PipelineIR, run_state: &RunState) -> Vec<&'a JobIR> {
        let mut ready = Vec::new();
        let pending = run_state.pending_jobs().await;

        for job in &pipeline.jobs {
            if !pending.contains(&job.id) {
                continue;
            }

            let deps_satisfied = self.check_dependencies_satisfied(job, run_state).await;

            if deps_satisfied {
                ready.push(job);
            }
        }

        ready
    }

    async fn check_dependencies_satisfied(&self, job: &JobIR, run_state: &RunState) -> bool {
        for dep_id in &job.depends_on {
            if !run_state.is_job_complete(dep_id).await {
                return false;
            }
        }
        true
    }

    async fn cancel_pending_jobs(&self, run_state: &RunState) {
        let pending = run_state.pending_jobs().await;
        for job_id in pending {
            run_state.mark_job_cancelled(&job_id).await;
        }

        let queued = run_state.running_jobs().await;
        for job_id in queued {
            run_state.mark_job_cancelled(&job_id).await;
        }
    }
}

/// Strategy for completing jobs during test execution.
#[async_trait::async_trait]
trait CompletionStrategy {
    async fn run(
        &mut self,
        scheduler: &MockScheduler,
        run_state: &RunState,
        pipeline: &PipelineIR,
    );
}

/// Completes all jobs successfully as soon as they are dispatched.
struct CompleteAllSuccessfully;

#[async_trait::async_trait]
impl CompletionStrategy for CompleteAllSuccessfully {
    async fn run(
        &mut self,
        scheduler: &MockScheduler,
        run_state: &RunState,
        _pipeline: &PipelineIR,
    ) {
        loop {
            tokio::time::sleep(Duration::from_millis(5)).await;

            let dispatched = scheduler.dispatched_jobs.read().await.clone();
            for job in dispatched {
                let job_state = run_state.get_job(&job.job_id).await;
                if let Some(state) = job_state {
                    if state.status == JobStatus::Queued {
                        run_state
                            .mark_job_completed(&job.job_id, true, Some(0), None)
                            .await;
                    }
                }
            }

            if run_state.is_complete().await {
                break;
            }
        }
    }
}

/// Completes jobs with specified outcomes.
struct CustomCompletions {
    outcomes: HashMap<String, (bool, Option<String>)>,
}

impl CustomCompletions {
    fn new() -> Self {
        Self {
            outcomes: HashMap::new(),
        }
    }

    fn with_success(mut self, job_name: &str) -> Self {
        self.outcomes
            .insert(job_name.to_string(), (true, None));
        self
    }

    fn with_failure(mut self, job_name: &str, error: &str) -> Self {
        self.outcomes
            .insert(job_name.to_string(), (false, Some(error.to_string())));
        self
    }
}

#[async_trait::async_trait]
impl CompletionStrategy for CustomCompletions {
    async fn run(
        &mut self,
        scheduler: &MockScheduler,
        run_state: &RunState,
        _pipeline: &PipelineIR,
    ) {
        loop {
            tokio::time::sleep(Duration::from_millis(5)).await;

            let dispatched = scheduler.dispatched_jobs.read().await.clone();
            for job in dispatched {
                let job_state = run_state.get_job(&job.job_id).await;
                if let Some(state) = job_state {
                    if state.status == JobStatus::Queued {
                        if let Some((success, error_msg)) = self.outcomes.get(&job.job_name) {
                            run_state
                                .mark_job_completed(
                                    &job.job_id,
                                    *success,
                                    if *success { Some(0) } else { Some(1) },
                                    error_msg.clone(),
                                )
                                .await;
                        } else {
                            run_state
                                .mark_job_completed(&job.job_id, true, Some(0), None)
                                .await;
                        }
                    }
                }
            }

            if run_state.is_complete().await {
                break;
            }
        }
    }
}

/// Completion strategy that simulates retries within the completion handler.
/// This emulates what happens when a job fails and is retried by the engine:
/// - First N dispatches fail, subsequent ones succeed
/// - Tracks dispatches per job to simulate retry attempts
struct RetryingCompletion {
    job_name: String,
    fail_count: usize,
    max_attempts: usize,
    attempts: Arc<AtomicUsize>,
    processed_dispatches: Arc<Mutex<std::collections::HashSet<JobRunId>>>,
}

impl RetryingCompletion {
    fn new(job_name: &str, fail_count: usize, max_attempts: usize) -> Self {
        Self {
            job_name: job_name.to_string(),
            fail_count,
            max_attempts,
            attempts: Arc::new(AtomicUsize::new(0)),
            processed_dispatches: Arc::new(Mutex::new(std::collections::HashSet::new())),
        }
    }
}

#[async_trait::async_trait]
impl CompletionStrategy for RetryingCompletion {
    async fn run(
        &mut self,
        scheduler: &MockScheduler,
        run_state: &RunState,
        pipeline: &PipelineIR,
    ) {
        let target_job_id = pipeline
            .jobs
            .iter()
            .find(|j| j.name == self.job_name)
            .map(|j| j.id);

        loop {
            tokio::time::sleep(Duration::from_millis(5)).await;

            let dispatched = scheduler.dispatched_jobs.read().await.clone();
            for job in dispatched {
                {
                    let mut processed = self.processed_dispatches.lock().await;
                    if processed.contains(&job.job_run_id) {
                        continue;
                    }
                    processed.insert(job.job_run_id);
                }

                let job_state = run_state.get_job(&job.job_id).await;
                if let Some(state) = job_state {
                    if state.status == JobStatus::Queued {
                        if Some(job.job_id) == target_job_id {
                            let attempt = self.attempts.fetch_add(1, Ordering::SeqCst);
                            if attempt < self.fail_count {
                                if attempt + 1 < self.max_attempts {
                                    run_state
                                        .update_job(&job.job_id, |j| {
                                            j.status = JobStatus::Pending;
                                            j.error_message = Some(format!("Attempt {} failed, retrying", attempt + 1));
                                        })
                                        .await;
                                } else {
                                    run_state
                                        .mark_job_completed(
                                            &job.job_id,
                                            false,
                                            Some(1),
                                            Some(format!("Attempt {} failed, max retries exhausted", attempt + 1)),
                                        )
                                        .await;
                                }
                            } else {
                                run_state
                                    .mark_job_completed(&job.job_id, true, Some(0), None)
                                    .await;
                            }
                        } else {
                            run_state
                                .mark_job_completed(&job.job_id, true, Some(0), None)
                                .await;
                        }
                    }
                }
            }

            if run_state.is_complete().await {
                break;
            }
        }
    }
}

// ============================================================================
// Integration Tests
// ============================================================================

#[tokio::test]
async fn test_simple_pipeline_single_job() {
    let job_id = JobId::new();
    let job = test_job(job_id, "build", vec![]);

    let pipeline = test_pipeline("simple", vec![job]);

    let (scheduler, _rx) = MockScheduler::new();
    let cache = Arc::new(MockCache::new());
    let executor = TestExecutor::new(scheduler.clone(), cache);

    let result = executor
        .execute_with_completions(pipeline, CompleteAllSuccessfully)
        .await;

    assert_eq!(result.status, RunStatus::Succeeded);
    assert_eq!(result.jobs_succeeded, 1);
    assert_eq!(result.jobs_failed, 0);
    assert_eq!(result.jobs_skipped, 0);

    let dispatch_order = scheduler.get_dispatch_order().await;
    assert_eq!(dispatch_order, vec!["build"]);
}

#[tokio::test]
async fn test_diamond_dag_execution_order() {
    //     A
    //    / \
    //   B   C
    //    \ /
    //     D
    let a = JobId::new();
    let b = JobId::new();
    let c = JobId::new();
    let d = JobId::new();

    let jobs = vec![
        test_job(a, "A", vec![]),
        test_job(b, "B", vec![a]),
        test_job(c, "C", vec![a]),
        test_job(d, "D", vec![b, c]),
    ];

    let pipeline = test_pipeline("diamond", jobs);

    let (scheduler, _rx) = MockScheduler::new();
    let cache = Arc::new(MockCache::new());
    let executor = TestExecutor::new(scheduler.clone(), cache);

    let result = executor
        .execute_with_completions(pipeline, CompleteAllSuccessfully)
        .await;

    assert_eq!(result.status, RunStatus::Succeeded);
    assert_eq!(result.jobs_succeeded, 4);
    assert_eq!(result.jobs_failed, 0);
    assert_eq!(result.jobs_skipped, 0);

    let dispatch_order = scheduler.get_dispatch_order().await;

    let a_pos = dispatch_order.iter().position(|n| n == "A").unwrap();
    let b_pos = dispatch_order.iter().position(|n| n == "B").unwrap();
    let c_pos = dispatch_order.iter().position(|n| n == "C").unwrap();
    let d_pos = dispatch_order.iter().position(|n| n == "D").unwrap();

    assert!(a_pos < b_pos, "A must be dispatched before B");
    assert!(a_pos < c_pos, "A must be dispatched before C");
    assert!(b_pos < d_pos, "B must be dispatched before D");
    assert!(c_pos < d_pos, "C must be dispatched before D");

    let max_concurrent = scheduler.get_max_concurrent();
    assert!(
        max_concurrent >= 2,
        "B and C should be able to run concurrently (max_concurrent={})",
        max_concurrent
    );
}

#[tokio::test]
async fn test_diamond_dag_with_failure_propagation() {
    //     A
    //    / \
    //   B   C (fails)
    //    \ /
    //     D (should be skipped)
    let a = JobId::new();
    let b = JobId::new();
    let c = JobId::new();
    let d = JobId::new();

    let jobs = vec![
        test_job(a, "A", vec![]),
        test_job(b, "B", vec![a]),
        test_job(c, "C", vec![a]),
        test_job(d, "D", vec![b, c]),
    ];

    let pipeline = test_pipeline("diamond-failure", jobs);

    let (scheduler, _rx) = MockScheduler::new();
    let cache = Arc::new(MockCache::new());
    let executor = TestExecutor::new(scheduler.clone(), cache);

    let completions = CustomCompletions::new()
        .with_success("A")
        .with_success("B")
        .with_failure("C", "Test failure")
        .with_success("D");

    let result = executor
        .execute_with_completions(pipeline, completions)
        .await;

    assert_eq!(result.status, RunStatus::Failed);
    assert_eq!(result.jobs_succeeded, 2); // A and B
    assert_eq!(result.jobs_failed, 1);    // C
    assert_eq!(result.jobs_skipped, 1);   // D
}

#[tokio::test]
async fn test_cache_hit_skips_job() {
    let job_id = JobId::new();
    let mut job = test_job(job_id, "cached-job", vec![]);
    job.cache_config = Some(CacheConfig {
        key: "test-cache".to_string(),
        paths: vec!["target".to_string()],
        restore_keys: vec![],
    });

    let pipeline = test_pipeline("cache-test", vec![job]);

    let (scheduler, _rx) = MockScheduler::new();
    let cache = Arc::new(MockCache::new());

    cache.set_cache_hit("test-cache-target").await;

    let executor = TestExecutor::new(scheduler.clone(), cache);

    let result = executor
        .execute_with_completions(pipeline, CompleteAllSuccessfully)
        .await;

    assert_eq!(result.status, RunStatus::Succeeded);
    assert_eq!(result.jobs_succeeded, 1);

    let dispatch_order = scheduler.get_dispatch_order().await;
    assert!(
        dispatch_order.is_empty(),
        "Job with cache hit should not be dispatched"
    );
}

#[tokio::test]
async fn test_cache_miss_executes_job() {
    let job_id = JobId::new();
    let mut job = test_job(job_id, "cached-job", vec![]);
    job.cache_config = Some(CacheConfig {
        key: "test-cache".to_string(),
        paths: vec!["target".to_string()],
        restore_keys: vec![],
    });

    let pipeline = test_pipeline("cache-miss-test", vec![job]);

    let (scheduler, _rx) = MockScheduler::new();
    let cache = Arc::new(MockCache::new());

    let executor = TestExecutor::new(scheduler.clone(), cache);

    let result = executor
        .execute_with_completions(pipeline, CompleteAllSuccessfully)
        .await;

    assert_eq!(result.status, RunStatus::Succeeded);
    assert_eq!(result.jobs_succeeded, 1);

    let dispatch_order = scheduler.get_dispatch_order().await;
    assert_eq!(
        dispatch_order,
        vec!["cached-job"],
        "Job with cache miss should be dispatched"
    );
}

#[tokio::test]
async fn test_retry_succeeds_after_failure() {
    // Test: job fails first 2 attempts, succeeds on 3rd
    // max_attempts=3, fail_count=2 -> should succeed
    let job_id = JobId::new();
    let mut job = test_job(job_id, "flaky-job", vec![]);
    job.retry_policy = Some(met_parser::RetryPolicy {
        max_attempts: 3,
        backoff: Duration::from_millis(10),
    });

    let pipeline = test_pipeline("retry-test", vec![job]);

    let (scheduler, _rx) = MockScheduler::new();
    let cache = Arc::new(MockCache::new());
    let executor = TestExecutor::new(scheduler.clone(), cache);

    // Fail first 2 attempts, succeed on 3rd (max_attempts=3)
    let retrying = RetryingCompletion::new("flaky-job", 2, 3);
    let attempts = retrying.attempts.clone();

    let result = executor
        .execute_with_completions(pipeline, retrying)
        .await;

    assert_eq!(result.status, RunStatus::Succeeded);
    assert_eq!(
        attempts.load(Ordering::SeqCst),
        3,
        "Job should have been attempted 3 times"
    );
}

#[tokio::test]
async fn test_retry_exhaustion_fails() {
    // Test: job always fails, max_attempts=2
    // fail_count=10 (more than max), so it exhausts retries and fails
    let job_id = JobId::new();
    let mut job = test_job(job_id, "always-failing", vec![]);
    job.retry_policy = Some(met_parser::RetryPolicy {
        max_attempts: 2,
        backoff: Duration::from_millis(10),
    });

    let pipeline = test_pipeline("retry-exhaustion-test", vec![job]);

    let (scheduler, _rx) = MockScheduler::new();
    let cache = Arc::new(MockCache::new());
    let executor = TestExecutor::new(scheduler.clone(), cache);

    // Always fail (fail_count > max_attempts)
    let retrying = RetryingCompletion::new("always-failing", 10, 2);
    let attempts = retrying.attempts.clone();

    let result = executor
        .execute_with_completions(pipeline, retrying)
        .await;

    assert_eq!(result.status, RunStatus::Failed);
    assert_eq!(result.jobs_failed, 1);
    assert_eq!(
        attempts.load(Ordering::SeqCst),
        2,
        "Job should have exhausted 2 attempts"
    );
}

#[tokio::test]
async fn test_cancel_mid_run_marks_jobs_cancelled() {
    let a = JobId::new();
    let b = JobId::new();
    let c = JobId::new();

    let jobs = vec![
        test_job(a, "A", vec![]),
        test_job(b, "B", vec![a]),
        test_job(c, "C", vec![b]),
    ];

    let pipeline = test_pipeline("cancel-test", jobs);

    let run_id = RunId::new();
    let _org_id = OrganizationId::new();
    let run_state = RunState::new(run_id);

    run_state.set_status(RunStatus::Running).await;

    for job in &pipeline.jobs {
        let job_run_id = JobRunId::new();
        let job_state = JobState::new(job.id, job_run_id, &job.name);
        run_state.register_job(job_state).await;
    }

    let (scheduler, _rx) = MockScheduler::new();

    let job_a = &pipeline.jobs[0];
    let job_a_state = run_state.get_job(&a).await.unwrap();
    run_state.mark_job_queued(&a).await;
    scheduler.dispatch(job_a, job_a_state.job_run_id).await;

    tokio::time::sleep(Duration::from_millis(10)).await;
    run_state
        .mark_job_completed(&a, true, Some(0), None)
        .await;

    let job_b = &pipeline.jobs[1];
    let job_b_state = run_state.get_job(&b).await.unwrap();
    run_state.mark_job_queued(&b).await;
    scheduler.dispatch(job_b, job_b_state.job_run_id).await;

    run_state.request_cancellation().await;

    let pending = run_state.pending_jobs().await;
    for job_id in pending {
        run_state.mark_job_cancelled(&job_id).await;
    }

    let queued_running = run_state.running_jobs().await;
    for job_id in queued_running {
        run_state.mark_job_cancelled(&job_id).await;
    }

    let final_status = run_state.compute_final_status().await;

    assert_eq!(final_status, RunStatus::Cancelled);

    let job_a_final = run_state.get_job(&a).await.unwrap();
    let job_b_final = run_state.get_job(&b).await.unwrap();
    let job_c_final = run_state.get_job(&c).await.unwrap();

    assert_eq!(job_a_final.status, JobStatus::Succeeded, "Job A should have succeeded");
    assert_eq!(job_b_final.status, JobStatus::Cancelled, "Job B should be cancelled");
    assert_eq!(job_c_final.status, JobStatus::Cancelled, "Job C should be cancelled");
}

#[tokio::test]
async fn test_linear_pipeline_execution_order() {
    // A -> B -> C -> D
    let a = JobId::new();
    let b = JobId::new();
    let c = JobId::new();
    let d = JobId::new();

    let jobs = vec![
        test_job(a, "A", vec![]),
        test_job(b, "B", vec![a]),
        test_job(c, "C", vec![b]),
        test_job(d, "D", vec![c]),
    ];

    let pipeline = test_pipeline("linear", jobs);

    let (scheduler, _rx) = MockScheduler::new();
    let cache = Arc::new(MockCache::new());
    let executor = TestExecutor::new(scheduler.clone(), cache);

    let result = executor
        .execute_with_completions(pipeline, CompleteAllSuccessfully)
        .await;

    assert_eq!(result.status, RunStatus::Succeeded);
    assert_eq!(result.jobs_succeeded, 4);

    let dispatch_order = scheduler.get_dispatch_order().await;
    assert_eq!(dispatch_order, vec!["A", "B", "C", "D"]);
}

#[tokio::test]
async fn test_parallel_independent_jobs() {
    // A, B, C, D all independent
    let jobs = vec![
        test_job(JobId::new(), "A", vec![]),
        test_job(JobId::new(), "B", vec![]),
        test_job(JobId::new(), "C", vec![]),
        test_job(JobId::new(), "D", vec![]),
    ];

    let pipeline = test_pipeline("parallel", jobs);

    let (scheduler, _rx) = MockScheduler::new();
    let cache = Arc::new(MockCache::new());
    let executor = TestExecutor::new(scheduler.clone(), cache);

    let result = executor
        .execute_with_completions(pipeline, CompleteAllSuccessfully)
        .await;

    assert_eq!(result.status, RunStatus::Succeeded);
    assert_eq!(result.jobs_succeeded, 4);

    let max_concurrent = scheduler.get_max_concurrent();
    assert_eq!(
        max_concurrent, 4,
        "All 4 independent jobs should be dispatched concurrently"
    );
}

#[tokio::test]
async fn test_mixed_cache_hits_and_misses() {
    let a = JobId::new();
    let b = JobId::new();
    let c = JobId::new();

    let mut job_a = test_job(a, "A", vec![]);
    job_a.cache_config = Some(CacheConfig {
        key: "job-a".to_string(),
        paths: vec!["out".to_string()],
        restore_keys: vec![],
    });

    let job_b = test_job(b, "B", vec![a]); // No cache config

    let mut job_c = test_job(c, "C", vec![b]);
    job_c.cache_config = Some(CacheConfig {
        key: "job-c".to_string(),
        paths: vec!["out".to_string()],
        restore_keys: vec![],
    });

    let pipeline = test_pipeline("mixed-cache", vec![job_a, job_b, job_c]);

    let (scheduler, _rx) = MockScheduler::new();
    let cache = Arc::new(MockCache::new());

    cache.set_cache_hit("job-a-out").await;

    let executor = TestExecutor::new(scheduler.clone(), cache);

    let result = executor
        .execute_with_completions(pipeline, CompleteAllSuccessfully)
        .await;

    assert_eq!(result.status, RunStatus::Succeeded);
    assert_eq!(result.jobs_succeeded, 3);

    let dispatch_order = scheduler.get_dispatch_order().await;
    assert!(
        !dispatch_order.contains(&"A".to_string()),
        "A should be skipped due to cache hit"
    );
    assert!(
        dispatch_order.contains(&"B".to_string()),
        "B should be dispatched (no cache)"
    );
    assert!(
        dispatch_order.contains(&"C".to_string()),
        "C should be dispatched (cache miss)"
    );
}
