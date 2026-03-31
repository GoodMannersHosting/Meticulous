//! Context propagation for distributed tracing.
//!
//! Provides inject/extract functions for HTTP, gRPC, and NATS headers
//! to propagate trace context across service boundaries.

use opentelemetry::{
    global,
    propagation::{Extractor, Injector},
    Context,
};
use std::collections::HashMap;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt as _;

/// HTTP header carrier for context propagation.
pub struct HttpHeaderCarrier<'a> {
    headers: &'a http::HeaderMap,
}

impl<'a> HttpHeaderCarrier<'a> {
    pub fn new(headers: &'a http::HeaderMap) -> Self {
        Self { headers }
    }
}

impl Extractor for HttpHeaderCarrier<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.headers.get(key).and_then(|v| v.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.headers.keys().map(|k| k.as_str()).collect()
    }
}

/// Mutable HTTP header carrier for injection.
pub struct HttpHeaderInjector<'a> {
    headers: &'a mut http::HeaderMap,
}

impl<'a> HttpHeaderInjector<'a> {
    pub fn new(headers: &'a mut http::HeaderMap) -> Self {
        Self { headers }
    }
}

impl Injector for HttpHeaderInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        if let Ok(name) = http::HeaderName::try_from(key)
            && let Ok(val) = http::HeaderValue::try_from(value)
        {
            self.headers.insert(name, val);
        }
    }
}

/// Simple string map carrier for NATS headers.
pub struct NatsHeaderCarrier {
    headers: HashMap<String, String>,
}

impl NatsHeaderCarrier {
    pub fn new() -> Self {
        Self { headers: HashMap::new() }
    }

    pub fn from_nats_headers(headers: &async_nats::HeaderMap) -> Self {
        let mut map = HashMap::new();
        for (name, values) in headers.iter() {
            if let Some(value) = values.iter().next() {
                map.insert(name.to_string(), value.to_string());
            }
        }
        Self { headers: map }
    }

    pub fn into_nats_headers(self) -> async_nats::HeaderMap {
        let mut headers = async_nats::HeaderMap::new();
        for (key, value) in self.headers {
            headers.insert(key.as_str(), value.as_str());
        }
        headers
    }

    pub fn headers(&self) -> &HashMap<String, String> {
        &self.headers
    }
}

impl Default for NatsHeaderCarrier {
    fn default() -> Self {
        Self::new()
    }
}

impl Extractor for NatsHeaderCarrier {
    fn get(&self, key: &str) -> Option<&str> {
        self.headers.get(key).map(|s| s.as_str())
    }

    fn keys(&self) -> Vec<&str> {
        self.headers.keys().map(|s| s.as_str()).collect()
    }
}

impl Injector for NatsHeaderCarrier {
    fn set(&mut self, key: &str, value: String) {
        self.headers.insert(key.to_string(), value);
    }
}

/// Extract trace context from HTTP headers and set it on the current span.
pub fn extract_http(headers: &http::HeaderMap) {
    let context = extract_http_context(headers);
    Span::current().set_parent(context);
}

/// Extract trace context from HTTP headers and return the context.
pub fn extract_http_context(headers: &http::HeaderMap) -> Context {
    global::get_text_map_propagator(|p| p.extract(&HttpHeaderCarrier::new(headers)))
}

/// Inject current trace context into HTTP headers.
pub fn inject_http(headers: &mut http::HeaderMap) {
    let context = Span::current().context();
    inject_http_context(&context, headers);
}

/// Inject a specific context into HTTP headers.
pub fn inject_http_context(context: &Context, headers: &mut http::HeaderMap) {
    global::get_text_map_propagator(|p| {
        p.inject_context(context, &mut HttpHeaderInjector::new(headers));
    });
}

/// Extract trace context from NATS headers and set it on the current span.
pub fn extract_nats(headers: &async_nats::HeaderMap) {
    let context = extract_nats_context(headers);
    Span::current().set_parent(context);
}

/// Extract trace context from NATS headers and return the context.
pub fn extract_nats_context(headers: &async_nats::HeaderMap) -> Context {
    let carrier = NatsHeaderCarrier::from_nats_headers(headers);
    global::get_text_map_propagator(|p| p.extract(&carrier))
}

/// Inject current trace context into NATS headers.
pub fn inject_nats() -> async_nats::HeaderMap {
    let context = Span::current().context();
    inject_nats_context(&context)
}

/// Inject a specific context into NATS headers.
pub fn inject_nats_context(context: &Context) -> async_nats::HeaderMap {
    let mut carrier = NatsHeaderCarrier::new();
    global::get_text_map_propagator(|p| {
        p.inject_context(context, &mut carrier);
    });
    carrier.into_nats_headers()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_carrier_extraction() {
        let mut headers = http::HeaderMap::new();
        headers.insert("traceparent", "00-abc-def-01".parse().unwrap());

        let carrier = HttpHeaderCarrier::new(&headers);
        assert_eq!(carrier.get("traceparent"), Some("00-abc-def-01"));
    }

    #[test]
    fn test_http_carrier_injection() {
        let mut headers = http::HeaderMap::new();
        let mut injector = HttpHeaderInjector::new(&mut headers);
        injector.set("traceparent", "00-abc-def-01".to_string());

        assert_eq!(headers.get("traceparent").unwrap(), "00-abc-def-01");
    }

    #[test]
    fn test_nats_carrier() {
        let mut carrier = NatsHeaderCarrier::new();
        carrier.set("traceparent", "00-abc-def-01".to_string());

        assert_eq!(carrier.get("traceparent"), Some("00-abc-def-01"));
        assert!(carrier.keys().contains(&"traceparent"));
    }
}
