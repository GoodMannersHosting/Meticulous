//! Tracing utilities and span creation helpers.

use opentelemetry::trace::{SpanKind, TraceContextExt as _};
use tracing::{Level, Span};
use tracing_opentelemetry::OpenTelemetrySpanExt as _;

/// Create a new span for an HTTP request.
pub fn http_request_span(method: &str, path: &str, request_id: Option<&str>) -> Span {
    let span = tracing::span!(
        Level::INFO,
        "http.request",
        otel.kind = ?SpanKind::Server,
        http.request.method = %method,
        url.path = %path,
        http.response.status_code = tracing::field::Empty,
        request_id = tracing::field::Empty,
    );

    if let Some(id) = request_id {
        span.record("request_id", id);
    }

    span
}

/// Create a new span for a gRPC call.
pub fn grpc_call_span(service: &str, method: &str) -> Span {
    tracing::span!(
        Level::INFO,
        "grpc.call",
        otel.kind = ?SpanKind::Server,
        rpc.system = "grpc",
        rpc.service = %service,
        rpc.method = %method,
        rpc.grpc.status_code = tracing::field::Empty,
    )
}

/// Create a new span for a NATS message handler.
pub fn nats_handler_span(subject: &str) -> Span {
    tracing::span!(
        Level::INFO,
        "nats.handler",
        otel.kind = ?SpanKind::Consumer,
        messaging.system = "nats",
        messaging.destination.name = %subject,
        messaging.operation.type = "receive",
    )
}

/// Create a new span for publishing a NATS message.
pub fn nats_publish_span(subject: &str) -> Span {
    tracing::span!(
        Level::INFO,
        "nats.publish",
        otel.kind = ?SpanKind::Producer,
        messaging.system = "nats",
        messaging.destination.name = %subject,
        messaging.operation.type = "publish",
    )
}

/// Create a new span for a database query.
pub fn db_query_span(operation: &str, table: &str) -> Span {
    tracing::span!(
        Level::DEBUG,
        "db.query",
        otel.kind = ?SpanKind::Client,
        db.system = "postgresql",
        db.operation.name = %operation,
        db.collection.name = %table,
    )
}

/// Create a new span for an S3 operation.
pub fn s3_operation_span(operation: &str, bucket: &str, key: &str) -> Span {
    tracing::span!(
        Level::DEBUG,
        "s3.operation",
        otel.kind = ?SpanKind::Client,
        rpc.system = "aws-api",
        rpc.service = "S3",
        rpc.method = %operation,
        aws.s3.bucket = %bucket,
        aws.s3.key = %key,
    )
}

/// Create a new span for a pipeline run.
pub fn pipeline_run_span(pipeline_id: &str, run_id: &str) -> Span {
    tracing::span!(
        Level::INFO,
        "pipeline.run",
        otel.kind = ?SpanKind::Internal,
        pipeline.id = %pipeline_id,
        run.id = %run_id,
        run.status = tracing::field::Empty,
    )
}

/// Create a new span for a job execution.
pub fn job_execution_span(job_name: &str, run_id: &str) -> Span {
    tracing::span!(
        Level::INFO,
        "job.execution",
        otel.kind = ?SpanKind::Internal,
        job.name = %job_name,
        run.id = %run_id,
        job.status = tracing::field::Empty,
    )
}

/// Create a new span for a step execution.
pub fn step_execution_span(step_name: &str, job_name: &str) -> Span {
    tracing::span!(
        Level::DEBUG,
        "step.execution",
        otel.kind = ?SpanKind::Internal,
        step.name = %step_name,
        job.name = %job_name,
        step.status = tracing::field::Empty,
    )
}

/// Get the current trace ID if available.
pub fn current_trace_id() -> Option<String> {
    let context = Span::current().context();
    let span_ref = context.span();
    let span_context = span_ref.span_context();

    if span_context.is_valid() {
        Some(span_context.trace_id().to_string())
    } else {
        None
    }
}

/// Get the current span ID if available.
pub fn current_span_id() -> Option<String> {
    let context = Span::current().context();
    let span_ref = context.span();
    let span_context = span_ref.span_context();

    if span_context.is_valid() {
        Some(span_context.span_id().to_string())
    } else {
        None
    }
}

/// Record an error on the current span.
pub fn record_error(error: &dyn std::error::Error) {
    let span = Span::current();
    span.record("error", true);
    span.record("error.message", error.to_string());
}

/// Record a status code on an HTTP span.
pub fn record_http_status(status: u16) {
    Span::current().record("http.response.status_code", status);
}

/// Record a gRPC status code.
pub fn record_grpc_status(code: i32) {
    Span::current().record("rpc.grpc.status_code", code);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_creation() {
        let span = http_request_span("GET", "/api/v1/health", Some("req-123"));
        let _guard = span.enter();
    }

    #[test]
    fn test_grpc_span() {
        let span = grpc_call_span("AgentService", "Register");
        let _guard = span.enter();
    }
}
