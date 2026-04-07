//! Pre-defined metric instruments for Meticulous.
//!
//! All metrics use the `met_` prefix for consistent naming across the platform.

use opentelemetry::{
    KeyValue, global,
    metrics::{Counter, Histogram, Meter, UpDownCounter},
};
use std::sync::OnceLock;

static METRICS: OnceLock<MeticulousMetrics> = OnceLock::new();

/// Get the global metrics instance.
///
/// # Panics
///
/// Panics if metrics have not been initialized via `init_metrics()`.
pub fn metrics() -> &'static MeticulousMetrics {
    METRICS
        .get()
        .expect("Metrics not initialized. Call init_metrics() first.")
}

/// Initialize the global metrics instance.
pub fn init_metrics(meter: Meter) {
    let metrics = MeticulousMetrics::new(meter);
    let _ = METRICS.set(metrics);
}

/// Collection of pre-defined metrics for Meticulous.
pub struct MeticulousMetrics {
    // API metrics
    api_request_duration: Histogram<f64>,
    api_requests_total: Counter<u64>,
    api_requests_in_flight: UpDownCounter<i64>,
    api_errors_total: Counter<u64>,

    // Pipeline metrics
    pipeline_runs_total: Counter<u64>,
    pipeline_run_duration: Histogram<f64>,
    pipeline_runs_active: UpDownCounter<i64>,

    // Job metrics
    job_executions_total: Counter<u64>,
    job_execution_duration: Histogram<f64>,
    jobs_queued: UpDownCounter<i64>,
    jobs_running: UpDownCounter<i64>,

    // Agent metrics
    agents_connected: UpDownCounter<i64>,
    agent_heartbeats_total: Counter<u64>,
    agent_job_assignments_total: Counter<u64>,

    // Storage metrics
    storage_operations_total: Counter<u64>,
    storage_operation_duration: Histogram<f64>,
    storage_bytes_transferred: Counter<u64>,
}

impl MeticulousMetrics {
    fn new(meter: Meter) -> Self {
        Self {
            // API metrics
            api_request_duration: meter
                .f64_histogram("met_api_request_duration_seconds")
                .with_description("Duration of HTTP API requests in seconds")
                .with_unit("s")
                .build(),

            api_requests_total: meter
                .u64_counter("met_api_requests_total")
                .with_description("Total number of HTTP API requests")
                .build(),

            api_requests_in_flight: meter
                .i64_up_down_counter("met_api_requests_in_flight")
                .with_description("Number of HTTP API requests currently being processed")
                .build(),

            api_errors_total: meter
                .u64_counter("met_api_errors_total")
                .with_description("Total number of HTTP API errors")
                .build(),

            // Pipeline metrics
            pipeline_runs_total: meter
                .u64_counter("met_pipeline_runs_total")
                .with_description("Total number of pipeline runs")
                .build(),

            pipeline_run_duration: meter
                .f64_histogram("met_pipeline_run_duration_seconds")
                .with_description("Duration of pipeline runs in seconds")
                .with_unit("s")
                .build(),

            pipeline_runs_active: meter
                .i64_up_down_counter("met_pipeline_runs_active")
                .with_description("Number of currently active pipeline runs")
                .build(),

            // Job metrics
            job_executions_total: meter
                .u64_counter("met_job_executions_total")
                .with_description("Total number of job executions")
                .build(),

            job_execution_duration: meter
                .f64_histogram("met_job_execution_duration_seconds")
                .with_description("Duration of job executions in seconds")
                .with_unit("s")
                .build(),

            jobs_queued: meter
                .i64_up_down_counter("met_jobs_queued")
                .with_description("Number of jobs currently queued")
                .build(),

            jobs_running: meter
                .i64_up_down_counter("met_jobs_running")
                .with_description("Number of jobs currently running")
                .build(),

            // Agent metrics
            agents_connected: meter
                .i64_up_down_counter("met_agents_connected")
                .with_description("Number of currently connected agents")
                .build(),

            agent_heartbeats_total: meter
                .u64_counter("met_agent_heartbeats_total")
                .with_description("Total number of agent heartbeats received")
                .build(),

            agent_job_assignments_total: meter
                .u64_counter("met_agent_job_assignments_total")
                .with_description("Total number of jobs assigned to agents")
                .build(),

            // Storage metrics
            storage_operations_total: meter
                .u64_counter("met_storage_operations_total")
                .with_description("Total number of storage operations")
                .build(),

            storage_operation_duration: meter
                .f64_histogram("met_storage_operation_duration_seconds")
                .with_description("Duration of storage operations in seconds")
                .with_unit("s")
                .build(),

            storage_bytes_transferred: meter
                .u64_counter("met_storage_bytes_transferred")
                .with_description("Total bytes transferred to/from storage")
                .with_unit("By")
                .build(),
        }
    }

    // API metrics methods

    /// Record an API request duration.
    pub fn record_api_request(&self, method: &str, path: &str, status: u16, duration_secs: f64) {
        let attrs = [
            KeyValue::new("method", method.to_string()),
            KeyValue::new("path", path.to_string()),
            KeyValue::new("status", i64::from(status)),
        ];
        self.api_request_duration.record(duration_secs, &attrs);
        self.api_requests_total.add(1, &attrs);

        if status >= 400 {
            self.api_errors_total.add(
                1,
                &[
                    KeyValue::new("method", method.to_string()),
                    KeyValue::new("path", path.to_string()),
                    KeyValue::new("status_class", if status >= 500 { "5xx" } else { "4xx" }),
                ],
            );
        }
    }

    /// Increment in-flight requests counter.
    pub fn api_request_started(&self) {
        self.api_requests_in_flight.add(1, &[]);
    }

    /// Decrement in-flight requests counter.
    pub fn api_request_finished(&self) {
        self.api_requests_in_flight.add(-1, &[]);
    }

    // Pipeline metrics methods

    /// Record a pipeline run completion.
    pub fn record_pipeline_run(&self, pipeline_id: &str, status: &str, duration_secs: f64) {
        let attrs = [
            KeyValue::new("pipeline_id", pipeline_id.to_string()),
            KeyValue::new("status", status.to_string()),
        ];
        self.pipeline_runs_total.add(1, &attrs);
        self.pipeline_run_duration.record(duration_secs, &attrs);
    }

    /// Update active pipeline runs count.
    pub fn pipeline_run_started(&self) {
        self.pipeline_runs_active.add(1, &[]);
    }

    /// Update active pipeline runs count.
    pub fn pipeline_run_finished(&self) {
        self.pipeline_runs_active.add(-1, &[]);
    }

    // Job metrics methods

    /// Record a job execution completion.
    pub fn record_job_execution(&self, job_name: &str, status: &str, duration_secs: f64) {
        let attrs = [
            KeyValue::new("job_name", job_name.to_string()),
            KeyValue::new("status", status.to_string()),
        ];
        self.job_executions_total.add(1, &attrs);
        self.job_execution_duration.record(duration_secs, &attrs);
    }

    /// Update queued jobs count.
    pub fn job_queued(&self) {
        self.jobs_queued.add(1, &[]);
    }

    /// Update queued jobs count.
    pub fn job_dequeued(&self) {
        self.jobs_queued.add(-1, &[]);
    }

    /// Update running jobs count.
    pub fn job_started(&self) {
        self.jobs_queued.add(-1, &[]);
        self.jobs_running.add(1, &[]);
    }

    /// Update running jobs count.
    pub fn job_finished(&self) {
        self.jobs_running.add(-1, &[]);
    }

    // Agent metrics methods

    /// Record agent connection.
    pub fn agent_connected(&self, pool: &str) {
        self.agents_connected
            .add(1, &[KeyValue::new("pool", pool.to_string())]);
    }

    /// Record agent disconnection.
    pub fn agent_disconnected(&self, pool: &str) {
        self.agents_connected
            .add(-1, &[KeyValue::new("pool", pool.to_string())]);
    }

    /// Record agent heartbeat.
    pub fn agent_heartbeat(&self, agent_id: &str) {
        self.agent_heartbeats_total
            .add(1, &[KeyValue::new("agent_id", agent_id.to_string())]);
    }

    /// Record job assignment to agent.
    pub fn job_assigned_to_agent(&self, agent_id: &str) {
        self.agent_job_assignments_total
            .add(1, &[KeyValue::new("agent_id", agent_id.to_string())]);
    }

    // Storage metrics methods

    /// Record a storage operation.
    pub fn record_storage_operation(&self, operation: &str, bucket: &str, duration_secs: f64) {
        let attrs = [
            KeyValue::new("operation", operation.to_string()),
            KeyValue::new("bucket", bucket.to_string()),
        ];
        self.storage_operations_total.add(1, &attrs);
        self.storage_operation_duration
            .record(duration_secs, &attrs);
    }

    /// Record bytes transferred to/from storage.
    pub fn record_storage_transfer(&self, direction: &str, bytes: u64) {
        self.storage_bytes_transferred
            .add(bytes, &[KeyValue::new("direction", direction.to_string())]);
    }
}

/// Get the global meter for creating custom metrics.
pub fn meter() -> Meter {
    global::meter("meticulous")
}

#[cfg(test)]
mod tests {
    use opentelemetry::global;

    #[test]
    fn test_meter_creation() {
        let meter = global::meter("test");
        let counter = meter.u64_counter("test_counter").build();
        counter.add(1, &[]);
    }
}
