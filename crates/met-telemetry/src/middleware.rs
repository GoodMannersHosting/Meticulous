//! Axum middleware for automatic request tracing and metrics.

use crate::{
    metrics::metrics,
    propagation::{extract_http, inject_http},
    tracing::http_request_span,
};
use axum::{body::Body, extract::Request, response::Response};
use futures::future::BoxFuture;
use pin_project_lite::pin_project;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Instant,
};
use tower::{Layer, Service};
use tracing::Instrument;

/// Layer that adds OpenTelemetry tracing and metrics to Axum requests.
#[derive(Clone, Default)]
pub struct TelemetryLayer;

impl TelemetryLayer {
    pub fn new() -> Self {
        Self
    }
}

impl<S> Layer<S> for TelemetryLayer {
    type Service = TelemetryMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        TelemetryMiddleware { inner }
    }
}

/// Middleware service that instruments HTTP requests.
#[derive(Clone)]
pub struct TelemetryMiddleware<S> {
    inner: S,
}

impl<S> Service<Request<Body>> for TelemetryMiddleware<S>
where
    S: Service<Request<Body>, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let start = Instant::now();
        let method = req.method().to_string();
        let path = req.uri().path().to_string();
        let request_id = req
            .headers()
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        extract_http(req.headers());

        let span = http_request_span(&method, &path, request_id.as_deref());
        metrics().api_request_started();

        let mut inner = self.inner.clone();

        Box::pin(
            async move {
                let response = inner.call(req).await;

                let duration = start.elapsed().as_secs_f64();
                let status = response
                    .as_ref()
                    .map(|r| r.status().as_u16())
                    .unwrap_or(500);

                crate::tracing::record_http_status(status);
                metrics().api_request_finished();
                metrics().record_api_request(&method, &path, status, duration);

                response
            }
            .instrument(span),
        )
    }
}

/// Layer for propagating trace context to outgoing requests.
#[derive(Clone, Default)]
pub struct PropagationLayer;

impl PropagationLayer {
    pub fn new() -> Self {
        Self
    }
}

impl<S> Layer<S> for PropagationLayer {
    type Service = PropagationMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        PropagationMiddleware { inner }
    }
}

/// Middleware that injects trace context into outgoing requests.
#[derive(Clone)]
pub struct PropagationMiddleware<S> {
    inner: S,
}

impl<S, B> Service<Request<B>> for PropagationMiddleware<S>
where
    S: Service<Request<B>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    B: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = PropagationFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request<B>) -> Self::Future {
        inject_http(req.headers_mut());
        PropagationFuture {
            inner: self.inner.call(req),
        }
    }
}

pin_project! {
    pub struct PropagationFuture<F> {
        #[pin]
        inner: F,
    }
}

impl<F, T, E> Future for PropagationFuture<F>
where
    F: Future<Output = Result<T, E>>,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.project().inner.poll(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telemetry_layer_creation() {
        let _layer = TelemetryLayer::new();
    }

    #[test]
    fn test_propagation_layer_creation() {
        let _layer = PropagationLayer::new();
    }
}
