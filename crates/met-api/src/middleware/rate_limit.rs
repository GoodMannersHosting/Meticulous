//! Rate limiting middleware using the token bucket algorithm.
//!
//! Each client (identified by IP) gets a bucket with a configured capacity
//! and refill rate. Requests that arrive when the bucket is empty receive
//! a `429 Too Many Requests` response with a `Retry-After` header.

use crate::config::RateLimitConfig;
use axum::{
    body::Body,
    http::{Request, Response, StatusCode},
};
use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::Instant,
};
use tower::{Layer, Service};

struct Bucket {
    tokens: f64,
    last_refill: Instant,
}

impl Bucket {
    fn new(capacity: u32) -> Self {
        Self {
            tokens: capacity as f64,
            last_refill: Instant::now(),
        }
    }

    /// Refill tokens based on elapsed time, then try to consume one.
    /// Returns `Ok(())` on success, or `Err(wait_secs)` with the
    /// estimated seconds until a token becomes available.
    fn try_consume(&mut self, capacity: u32, rate: u32) -> Result<(), f64> {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * rate as f64).min(capacity as f64);
        self.last_refill = now;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            Ok(())
        } else {
            let deficit = 1.0 - self.tokens;
            let wait = deficit / rate as f64;
            Err(wait)
        }
    }
}

type BucketMap = Arc<Mutex<HashMap<String, Bucket>>>;

/// Rate limiting layer.
#[derive(Clone)]
pub struct RateLimitLayer {
    config: RateLimitConfig,
    buckets: BucketMap,
}

impl RateLimitLayer {
    /// Create a new rate limit layer.
    pub fn new(config: &RateLimitConfig) -> Self {
        Self {
            config: config.clone(),
            buckets: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl<S> Layer<S> for RateLimitLayer {
    type Service = RateLimitService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RateLimitService {
            inner,
            config: self.config.clone(),
            buckets: Arc::clone(&self.buckets),
        }
    }
}

/// Rate limiting service wrapper.
#[derive(Clone)]
pub struct RateLimitService<S> {
    inner: S,
    config: RateLimitConfig,
    buckets: BucketMap,
}

fn extract_client_ip(req: &Request<Body>) -> String {
    if let Some(forwarded) = req.headers().get("x-forwarded-for") {
        if let Ok(value) = forwarded.to_str() {
            if let Some(first) = value.split(',').next() {
                let ip = first.trim();
                if !ip.is_empty() {
                    return ip.to_string();
                }
            }
        }
    }

    if let Some(real_ip) = req.headers().get("x-real-ip") {
        if let Ok(value) = real_ip.to_str() {
            let ip = value.trim();
            if !ip.is_empty() {
                return ip.to_string();
            }
        }
    }

    "unknown".to_string()
}

impl<S> Service<Request<Body>> for RateLimitService<S>
where
    S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send,
    S::Error: Send,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let client_ip = extract_client_ip(&request);
        let capacity = self.config.burst_size;
        let rate = self.config.requests_per_second;

        let result = {
            let mut map = self.buckets.lock().unwrap_or_else(|e| e.into_inner());
            let bucket = map
                .entry(client_ip)
                .or_insert_with(|| Bucket::new(capacity));
            bucket.try_consume(capacity, rate)
        };

        match result {
            Ok(()) => {
                let future = self.inner.call(request);
                Box::pin(future)
            }
            Err(wait_secs) => {
                let retry_after = wait_secs.ceil() as u64;
                Box::pin(async move {
                    Ok(Response::builder()
                        .status(StatusCode::TOO_MANY_REQUESTS)
                        .header("retry-after", retry_after.max(1).to_string())
                        .header("content-type", "application/json")
                        .body(Body::from(
                            r#"{"error":"rate limit exceeded","message":"Too many requests, please retry later"}"#,
                        ))
                        .expect("valid 429 response"))
                })
            }
        }
    }
}

/// Create a rate limit layer based on configuration.
///
/// If rate limiting is disabled, returns `None`.
pub fn rate_limit_layer(config: &RateLimitConfig) -> Option<RateLimitLayer> {
    if config.enabled {
        Some(RateLimitLayer::new(config))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_layer_creation() {
        let config = RateLimitConfig::default();
        let layer = rate_limit_layer(&config);
        assert!(layer.is_some());
    }

    #[test]
    fn test_rate_limit_disabled() {
        let mut config = RateLimitConfig::default();
        config.enabled = false;
        let layer = rate_limit_layer(&config);
        assert!(layer.is_none());
    }

    #[test]
    fn test_bucket_allows_up_to_capacity() {
        let mut bucket = Bucket::new(3);
        assert!(bucket.try_consume(3, 1).is_ok());
        assert!(bucket.try_consume(3, 1).is_ok());
        assert!(bucket.try_consume(3, 1).is_ok());
        assert!(bucket.try_consume(3, 1).is_err());
    }

    #[test]
    fn test_bucket_refills_over_time() {
        let mut bucket = Bucket::new(1);
        assert!(bucket.try_consume(1, 10).is_ok());
        assert!(bucket.try_consume(1, 10).is_err());

        // Simulate time passing by backdating last_refill
        bucket.last_refill = Instant::now() - std::time::Duration::from_secs(1);
        assert!(bucket.try_consume(1, 10).is_ok());
    }

    #[test]
    fn test_extract_client_ip_forwarded() {
        let req = Request::builder()
            .header("x-forwarded-for", "1.2.3.4, 5.6.7.8")
            .body(Body::empty())
            .unwrap();
        assert_eq!(extract_client_ip(&req), "1.2.3.4");
    }

    #[test]
    fn test_extract_client_ip_real_ip() {
        let req = Request::builder()
            .header("x-real-ip", "10.0.0.1")
            .body(Body::empty())
            .unwrap();
        assert_eq!(extract_client_ip(&req), "10.0.0.1");
    }

    #[test]
    fn test_extract_client_ip_fallback() {
        let req = Request::builder().body(Body::empty()).unwrap();
        assert_eq!(extract_client_ip(&req), "unknown");
    }
}
