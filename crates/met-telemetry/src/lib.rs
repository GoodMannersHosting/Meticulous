//! OpenTelemetry metrics and tracing for Meticulous CI/CD.
//!
//! This crate provides initialization and configuration for distributed tracing
//! and metrics collection using the OpenTelemetry SDK.
//!
//! # Usage
//!
//! ```ignore
//! use met_telemetry::{TelemetryConfig, init_telemetry, shutdown};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = TelemetryConfig::default();
//!     let guard = init_telemetry(&config)?;
//!     
//!     // Your application code here...
//!     
//!     shutdown().await;
//!     Ok(())
//! }
//! ```

pub mod config;
pub mod exporters;
pub mod metrics;
pub mod middleware;
pub mod propagation;
pub mod tracing;

pub use config::{MetricsConfig, OtlpConfig, OtlpProtocol, TelemetryConfig, TracingConfig};
pub use exporters::ExporterError;
pub use metrics::{init_metrics, meter, metrics, MeticulousMetrics};
pub use middleware::{PropagationLayer, TelemetryLayer};
pub use propagation::{
    extract_http, extract_http_context, extract_nats, extract_nats_context, inject_http,
    inject_http_context, inject_nats, inject_nats_context,
};
pub use tracing::{
    current_span_id, current_trace_id, db_query_span, grpc_call_span, http_request_span,
    job_execution_span, nats_handler_span, nats_publish_span, pipeline_run_span, record_error,
    record_grpc_status, record_http_status, s3_operation_span, step_execution_span,
};

use met_core::config::LogConfig;
use opentelemetry::global;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::{metrics::SdkMeterProvider, trace::TracerProvider};
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Guard that holds the telemetry providers for shutdown.
pub struct TelemetryGuard {
    tracer_provider: Option<TracerProvider>,
    meter_provider: Option<SdkMeterProvider>,
}

impl TelemetryGuard {
    fn new(
        tracer_provider: Option<TracerProvider>,
        meter_provider: Option<SdkMeterProvider>,
    ) -> Self {
        Self { tracer_provider, meter_provider }
    }
}

/// Initialize the telemetry subsystem with OpenTelemetry.
///
/// This sets up:
/// - Tracing with OTLP export
/// - Metrics with OTLP export
/// - Context propagation
///
/// # Errors
///
/// Returns an error if initialization fails.
pub fn init_telemetry(
    telemetry_config: &TelemetryConfig,
    log_config: &LogConfig,
) -> Result<TelemetryGuard, Box<dyn std::error::Error + Send + Sync>> {
    if !telemetry_config.enabled {
        init_tracing_only(log_config)?;
        return Ok(TelemetryGuard::new(None, None));
    }

    let tracer_provider = if telemetry_config.tracing.enabled {
        Some(exporters::init_tracer(telemetry_config)?)
    } else {
        None
    };

    let meter_provider = if telemetry_config.metrics.enabled {
        let provider = exporters::init_meter(telemetry_config)?;
        let meter = global::meter("meticulous");
        init_metrics(meter);
        Some(provider)
    } else {
        None
    };

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&log_config.level));

    let registry = tracing_subscriber::registry().with(filter);

    if let Some(ref provider) = tracer_provider {
        let tracer = provider.tracer(telemetry_config.service_name.clone());
        let telemetry_layer = OpenTelemetryLayer::new(tracer);

        match log_config.format {
            met_core::config::LogFormat::Json => {
                registry
                    .with(telemetry_layer)
                    .with(tracing_subscriber::fmt::layer().json())
                    .init();
            }
            met_core::config::LogFormat::Compact => {
                registry
                    .with(telemetry_layer)
                    .with(tracing_subscriber::fmt::layer().compact())
                    .init();
            }
            met_core::config::LogFormat::Text => {
                registry.with(telemetry_layer).with(tracing_subscriber::fmt::layer()).init();
            }
        }
    } else {
        init_tracing_only(log_config)?;
    }

    Ok(TelemetryGuard::new(tracer_provider, meter_provider))
}

/// Initialize only the tracing subscriber without OpenTelemetry.
///
/// # Errors
///
/// Returns an error if initialization fails.
pub fn init_tracing_only(
    config: &LogConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.level));

    let subscriber = tracing_subscriber::registry().with(filter);

    match config.format {
        met_core::config::LogFormat::Json => {
            subscriber.with(tracing_subscriber::fmt::layer().json()).init();
        }
        met_core::config::LogFormat::Compact => {
            subscriber.with(tracing_subscriber::fmt::layer().compact()).init();
        }
        met_core::config::LogFormat::Text => {
            subscriber.with(tracing_subscriber::fmt::layer()).init();
        }
    }

    Ok(())
}

/// Shutdown the telemetry subsystem gracefully.
///
/// This flushes any pending traces and metrics before shutting down exporters.
pub fn shutdown(guard: TelemetryGuard) {
    if let Some(provider) = guard.tracer_provider
        && let Err(e) = provider.shutdown()
    {
        ::tracing::warn!("Error shutting down tracer provider: {}", e);
    }

    if let Some(provider) = guard.meter_provider
        && let Err(e) = provider.shutdown()
    {
        ::tracing::warn!("Error shutting down meter provider: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telemetry_config_default() {
        let config = TelemetryConfig::default();
        assert!(config.enabled);
        assert_eq!(config.service_name, "meticulous");
    }

    #[test]
    fn test_disabled_telemetry() {
        let mut config = TelemetryConfig::default();
        config.enabled = false;
        let log_config = LogConfig::default();

        assert!(!config.enabled);
        assert_eq!(log_config.level, "info");
    }
}
