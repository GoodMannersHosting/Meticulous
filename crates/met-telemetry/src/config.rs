//! Telemetry configuration for OpenTelemetry SDK initialization.

use serde::{Deserialize, Serialize};

/// Configuration for the telemetry subsystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TelemetryConfig {
    /// Enable telemetry collection.
    pub enabled: bool,
    /// Service name for identifying this component in traces.
    pub service_name: String,
    /// Service version for trace metadata.
    pub service_version: Option<String>,
    /// OTLP exporter configuration.
    pub otlp: OtlpConfig,
    /// Metrics configuration.
    pub metrics: MetricsConfig,
    /// Tracing configuration.
    pub tracing: TracingConfig,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            service_name: "meticulous".to_string(),
            service_version: None,
            otlp: OtlpConfig::default(),
            metrics: MetricsConfig::default(),
            tracing: TracingConfig::default(),
        }
    }
}

/// OTLP exporter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OtlpConfig {
    /// OTLP endpoint URL (e.g., "http://localhost:4317").
    pub endpoint: String,
    /// Protocol to use for exporting.
    pub protocol: OtlpProtocol,
    /// Export timeout in seconds.
    pub timeout_secs: u64,
    /// Optional headers to include with exports.
    pub headers: Vec<(String, String)>,
}

impl Default for OtlpConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:4317".to_string(),
            protocol: OtlpProtocol::Grpc,
            timeout_secs: 10,
            headers: Vec::new(),
        }
    }
}

/// OTLP export protocol.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OtlpProtocol {
    /// gRPC protocol (default).
    #[default]
    Grpc,
    /// HTTP/protobuf protocol.
    HttpProto,
}

/// Metrics collection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MetricsConfig {
    /// Enable metrics collection.
    pub enabled: bool,
    /// Export interval in seconds.
    pub export_interval_secs: u64,
    /// Histogram bucket boundaries for request duration.
    pub duration_buckets: Vec<f64>,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            export_interval_secs: 60,
            duration_buckets: vec![
                0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
            ],
        }
    }
}

/// Distributed tracing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TracingConfig {
    /// Enable distributed tracing.
    pub enabled: bool,
    /// Sampling ratio (0.0 to 1.0).
    pub sampling_ratio: f64,
    /// Maximum attributes per span.
    pub max_attributes_per_span: u32,
    /// Maximum events per span.
    pub max_events_per_span: u32,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sampling_ratio: 1.0,
            max_attributes_per_span: 128,
            max_events_per_span: 128,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = TelemetryConfig::default();
        assert!(config.enabled);
        assert_eq!(config.service_name, "meticulous");
        assert_eq!(config.otlp.protocol, OtlpProtocol::Grpc);
    }

    #[test]
    fn test_config_serialization() {
        let config = TelemetryConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: TelemetryConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.service_name, config.service_name);
    }
}
