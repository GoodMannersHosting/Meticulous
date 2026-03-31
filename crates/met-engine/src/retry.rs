//! Retry policy execution with exponential backoff.
//!
//! This module provides retry handling for failed jobs with configurable
//! backoff strategies.

use std::time::Duration;
use tracing::{debug, info, instrument};

use crate::error::Result;

/// Retry policy configuration.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of attempts (including initial attempt).
    pub max_attempts: u32,
    /// Initial backoff duration.
    pub initial_backoff: Duration,
    /// Backoff multiplier for exponential increase.
    pub multiplier: f64,
    /// Maximum backoff duration (cap).
    pub max_backoff: Duration,
    /// Whether to add jitter to backoff.
    pub jitter: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_secs(10),
            multiplier: 2.0,
            max_backoff: Duration::from_secs(300),
            jitter: true,
        }
    }
}

impl RetryPolicy {
    /// Create a new retry policy with default settings.
    pub fn new(max_attempts: u32) -> Self {
        Self {
            max_attempts,
            ..Default::default()
        }
    }

    /// Set the initial backoff duration.
    pub fn with_initial_backoff(mut self, backoff: Duration) -> Self {
        self.initial_backoff = backoff;
        self
    }

    /// Set the backoff multiplier.
    pub fn with_multiplier(mut self, multiplier: f64) -> Self {
        self.multiplier = multiplier;
        self
    }

    /// Set the maximum backoff duration.
    pub fn with_max_backoff(mut self, max_backoff: Duration) -> Self {
        self.max_backoff = max_backoff;
        self
    }

    /// Enable or disable jitter.
    pub fn with_jitter(mut self, jitter: bool) -> Self {
        self.jitter = jitter;
        self
    }

    /// Check if another retry attempt should be made.
    pub fn should_retry(&self, current_attempt: u32) -> bool {
        current_attempt < self.max_attempts
    }

    /// Calculate the backoff duration for a given attempt.
    #[instrument(skip(self))]
    pub fn calculate_backoff(&self, attempt: u32) -> Duration {
        if attempt == 0 {
            return Duration::ZERO;
        }

        let base_backoff = self.initial_backoff.as_secs_f64() 
            * self.multiplier.powi((attempt - 1) as i32);
        
        let backoff_secs = base_backoff.min(self.max_backoff.as_secs_f64());
        
        let final_backoff = if self.jitter {
            let jitter_factor = 0.5 + (rand_jitter() * 0.5);
            backoff_secs * jitter_factor
        } else {
            backoff_secs
        };

        let duration = Duration::from_secs_f64(final_backoff);
        debug!(attempt, backoff_ms = duration.as_millis(), "calculated retry backoff");
        duration
    }

    /// Get remaining attempts.
    pub fn remaining_attempts(&self, current_attempt: u32) -> u32 {
        self.max_attempts.saturating_sub(current_attempt)
    }
}

/// Retry executor for managing job retries.
pub struct RetryExecutor {
    policy: RetryPolicy,
}

impl RetryExecutor {
    /// Create a new retry executor with the given policy.
    pub fn new(policy: RetryPolicy) -> Self {
        Self { policy }
    }

    /// Execute a retry with the calculated backoff.
    #[instrument(skip(self, operation, on_retry))]
    pub async fn execute_with_retry<F, Fut, T, E>(
        &self,
        mut operation: F,
        on_retry: impl Fn(u32, Duration, &E),
    ) -> std::result::Result<T, E>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = std::result::Result<T, E>>,
        E: std::fmt::Debug,
    {
        let mut attempt = 1;

        loop {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    if !self.policy.should_retry(attempt) {
                        info!(attempt, "max retry attempts reached");
                        return Err(e);
                    }

                    let backoff = self.policy.calculate_backoff(attempt);
                    on_retry(attempt, backoff, &e);

                    info!(
                        attempt,
                        next_attempt = attempt + 1,
                        backoff_ms = backoff.as_millis(),
                        "retrying after backoff"
                    );

                    tokio::time::sleep(backoff).await;
                    attempt += 1;
                }
            }
        }
    }

    /// Check if the policy allows another retry.
    pub fn should_retry(&self, attempt: u32) -> bool {
        self.policy.should_retry(attempt)
    }

    /// Get the backoff duration for the next retry.
    pub fn next_backoff(&self, attempt: u32) -> Duration {
        self.policy.calculate_backoff(attempt)
    }
}

/// Retry state for tracking job retry status.
#[derive(Debug, Clone)]
pub struct RetryState {
    /// Current attempt number (1-indexed).
    pub attempt: u32,
    /// Maximum attempts allowed.
    pub max_attempts: u32,
    /// Last error message.
    pub last_error: Option<String>,
    /// Next retry time.
    pub next_retry_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl RetryState {
    /// Create a new retry state.
    pub fn new(max_attempts: u32) -> Self {
        Self {
            attempt: 1,
            max_attempts,
            last_error: None,
            next_retry_at: None,
        }
    }

    /// Check if retry is possible.
    pub fn can_retry(&self) -> bool {
        self.attempt < self.max_attempts
    }

    /// Record a failure and prepare for retry.
    pub fn record_failure(&mut self, error: &str, policy: &RetryPolicy) -> Option<Duration> {
        self.last_error = Some(error.to_string());
        
        if self.can_retry() {
            self.attempt += 1;
            let backoff = policy.calculate_backoff(self.attempt);
            self.next_retry_at = Some(chrono::Utc::now() + chrono::Duration::from_std(backoff).unwrap_or_default());
            Some(backoff)
        } else {
            self.next_retry_at = None;
            None
        }
    }

    /// Mark as succeeded.
    pub fn mark_succeeded(&mut self) {
        self.last_error = None;
        self.next_retry_at = None;
    }
}

/// Generate a random jitter value between 0 and 1.
fn rand_jitter() -> f64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::SystemTime;

    let mut hasher = DefaultHasher::new();
    SystemTime::now().hash(&mut hasher);
    std::thread::current().id().hash(&mut hasher);
    
    let hash = hasher.finish();
    (hash as f64) / (u64::MAX as f64)
}

/// Retry decision based on error type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryDecision {
    /// Retry with backoff.
    Retry,
    /// Do not retry (permanent failure).
    NoRetry,
    /// Retry immediately without backoff.
    RetryImmediately,
}

/// Determine if an error is retryable.
pub fn is_retryable_error(error_message: &str) -> RetryDecision {
    let message_lower = error_message.to_lowercase();
    
    let transient_patterns = [
        "timeout",
        "connection refused",
        "connection reset",
        "temporarily unavailable",
        "service unavailable",
        "rate limit",
        "throttl",
        "too many requests",
        "network",
        "dns",
        "socket",
    ];
    
    for pattern in transient_patterns {
        if message_lower.contains(pattern) {
            return RetryDecision::Retry;
        }
    }
    
    let permanent_patterns = [
        "permission denied",
        "access denied",
        "not found",
        "invalid argument",
        "invalid input",
        "authentication failed",
        "authorization failed",
        "configuration error",
    ];
    
    for pattern in permanent_patterns {
        if message_lower.contains(pattern) {
            return RetryDecision::NoRetry;
        }
    }
    
    RetryDecision::Retry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_policy() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_attempts, 3);
        assert_eq!(policy.initial_backoff, Duration::from_secs(10));
        assert_eq!(policy.multiplier, 2.0);
    }

    #[test]
    fn test_should_retry() {
        let policy = RetryPolicy::new(3);
        
        assert!(policy.should_retry(1));
        assert!(policy.should_retry(2));
        assert!(!policy.should_retry(3));
        assert!(!policy.should_retry(4));
    }

    #[test]
    fn test_calculate_backoff() {
        let policy = RetryPolicy::new(5)
            .with_initial_backoff(Duration::from_secs(1))
            .with_multiplier(2.0)
            .with_jitter(false);

        assert_eq!(policy.calculate_backoff(0), Duration::ZERO);
        assert_eq!(policy.calculate_backoff(1), Duration::from_secs(1));
        assert_eq!(policy.calculate_backoff(2), Duration::from_secs(2));
        assert_eq!(policy.calculate_backoff(3), Duration::from_secs(4));
    }

    #[test]
    fn test_max_backoff_cap() {
        let policy = RetryPolicy::new(10)
            .with_initial_backoff(Duration::from_secs(60))
            .with_multiplier(2.0)
            .with_max_backoff(Duration::from_secs(120))
            .with_jitter(false);

        let backoff = policy.calculate_backoff(5);
        assert!(backoff <= Duration::from_secs(120));
    }

    #[test]
    fn test_retry_state() {
        let policy = RetryPolicy::new(3);
        let mut state = RetryState::new(3);

        assert!(state.can_retry());
        assert_eq!(state.attempt, 1);

        let backoff = state.record_failure("test error", &policy);
        assert!(backoff.is_some());
        assert_eq!(state.attempt, 2);

        let backoff = state.record_failure("test error", &policy);
        assert!(backoff.is_some());
        assert_eq!(state.attempt, 3);

        let backoff = state.record_failure("test error", &policy);
        assert!(backoff.is_none());
        assert!(!state.can_retry());
    }

    #[test]
    fn test_is_retryable_error() {
        assert_eq!(is_retryable_error("Connection timeout"), RetryDecision::Retry);
        assert_eq!(is_retryable_error("Rate limit exceeded"), RetryDecision::Retry);
        assert_eq!(is_retryable_error("Permission denied"), RetryDecision::NoRetry);
        assert_eq!(is_retryable_error("Unknown error"), RetryDecision::Retry);
    }
}
