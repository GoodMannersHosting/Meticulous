//! OTLP exporter setup for traces and metrics.

use crate::config::{OtlpProtocol, TelemetryConfig};
use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::{MetricExporter, SpanExporter, WithExportConfig};
use opentelemetry_sdk::{
    metrics::{PeriodicReader, SdkMeterProvider},
    propagation::TraceContextPropagator,
    runtime,
    trace::{RandomIdGenerator, Sampler, TracerProvider},
    Resource,
};
use std::time::Duration;

/// Error type for exporter initialization.
#[derive(Debug, thiserror::Error)]
pub enum ExporterError {
    #[error("Failed to create trace exporter: {0}")]
    TraceExporter(String),
    #[error("Failed to create metrics exporter: {0}")]
    MetricsExporter(String),
    #[error("Failed to create tracer provider: {0}")]
    TracerProvider(String),
}

/// Initialize the OTLP trace exporter.
pub fn init_tracer(config: &TelemetryConfig) -> Result<TracerProvider, ExporterError> {
    let resource = Resource::new(service_attributes(config));

    let sampler = if config.tracing.sampling_ratio >= 1.0 {
        Sampler::AlwaysOn
    } else if config.tracing.sampling_ratio <= 0.0 {
        Sampler::AlwaysOff
    } else {
        Sampler::TraceIdRatioBased(config.tracing.sampling_ratio)
    };

    let exporter = create_span_exporter(config)?;

    let provider = TracerProvider::builder()
        .with_batch_exporter(exporter, runtime::Tokio)
        .with_sampler(sampler)
        .with_id_generator(RandomIdGenerator::default())
        .with_resource(resource)
        .build();

    global::set_tracer_provider(provider.clone());
    global::set_text_map_propagator(TraceContextPropagator::new());

    Ok(provider)
}

/// Initialize the OTLP metrics exporter.
pub fn init_meter(config: &TelemetryConfig) -> Result<SdkMeterProvider, ExporterError> {
    let resource = Resource::new(service_attributes(config));

    let exporter = create_metric_exporter(config)?;

    let reader = PeriodicReader::builder(exporter, runtime::Tokio)
        .with_interval(Duration::from_secs(config.metrics.export_interval_secs))
        .build();

    let provider = SdkMeterProvider::builder()
        .with_reader(reader)
        .with_resource(resource)
        .build();

    global::set_meter_provider(provider.clone());

    Ok(provider)
}

fn create_span_exporter(config: &TelemetryConfig) -> Result<SpanExporter, ExporterError> {
    let timeout = Duration::from_secs(config.otlp.timeout_secs);

    match config.otlp.protocol {
        OtlpProtocol::Grpc => SpanExporter::builder()
            .with_tonic()
            .with_endpoint(&config.otlp.endpoint)
            .with_timeout(timeout)
            .build()
            .map_err(|e| ExporterError::TraceExporter(e.to_string())),
        OtlpProtocol::HttpProto => SpanExporter::builder()
            .with_http()
            .with_endpoint(&config.otlp.endpoint)
            .with_timeout(timeout)
            .build()
            .map_err(|e| ExporterError::TraceExporter(e.to_string())),
    }
}

fn create_metric_exporter(config: &TelemetryConfig) -> Result<MetricExporter, ExporterError> {
    let timeout = Duration::from_secs(config.otlp.timeout_secs);

    match config.otlp.protocol {
        OtlpProtocol::Grpc => MetricExporter::builder()
            .with_tonic()
            .with_endpoint(&config.otlp.endpoint)
            .with_timeout(timeout)
            .build()
            .map_err(|e| ExporterError::MetricsExporter(e.to_string())),
        OtlpProtocol::HttpProto => MetricExporter::builder()
            .with_http()
            .with_endpoint(&config.otlp.endpoint)
            .with_timeout(timeout)
            .build()
            .map_err(|e| ExporterError::MetricsExporter(e.to_string())),
    }
}

fn service_attributes(config: &TelemetryConfig) -> Vec<KeyValue> {
    let mut attrs = vec![KeyValue::new("service.name", config.service_name.clone())];

    if let Some(ref version) = config.service_version {
        attrs.push(KeyValue::new("service.version", version.clone()));
    }

    attrs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_attributes() {
        let config = TelemetryConfig {
            service_name: "test-service".to_string(),
            service_version: Some("1.0.0".to_string()),
            ..Default::default()
        };

        let attrs = service_attributes(&config);
        assert_eq!(attrs.len(), 2);
    }
}
