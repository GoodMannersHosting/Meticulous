//! Dual-window token-bucket limits per credential class, driven by [`OrgPolicy`](met_store::repos::OrgPolicy).

use crate::extractors::CurrentUser;
use met_core::ids::AppInstallationId;
use met_store::repos::OrgPolicy;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

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

    fn try_consume(&mut self, capacity: u32, rate_per_sec: f64) -> Result<(), ()> {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * rate_per_sec).min(capacity as f64);
        self.last_refill = now;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            Ok(())
        } else {
            Err(())
        }
    }
}

/// In-memory limiter: primary + secondary windows per key (JWT user, API token id, or app installation).
#[derive(Default)]
pub struct CredentialRateLimiter {
    user_buckets: Mutex<HashMap<String, (Bucket, Bucket)>>,
    app_buckets: Mutex<HashMap<String, (Bucket, Bucket)>>,
}

impl CredentialRateLimiter {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn window(max_req: i32, period_secs: i32) -> (u32, f64) {
        let cap = max_req.max(1) as u32;
        let p = f64::from(period_secs.max(1));
        let rate = f64::from(cap) / p;
        (cap, rate)
    }

    /// User session JWT or user API token (`Authorization: Token`).
    pub fn check_user(&self, user: &CurrentUser, policy: &OrgPolicy) -> Result<(), ()> {
        let key = if let Some(tid) = user.api_token_id {
            format!("utok:{tid}")
        } else {
            format!("ujwt:{}", user.user_id)
        };
        let (p_cap, p_rate) = Self::window(
            policy.user_rl_primary_max,
            policy.user_rl_primary_period_secs,
        );
        let (s_cap, s_rate) = Self::window(
            policy.user_rl_secondary_max,
            policy.user_rl_secondary_period_secs,
        );
        let mut map = self.user_buckets.lock().unwrap_or_else(|e| e.into_inner());
        let entry = map
            .entry(key)
            .or_insert_with(|| (Bucket::new(p_cap), Bucket::new(s_cap)));
        entry.0.try_consume(p_cap, p_rate)?;
        entry.1.try_consume(s_cap, s_rate)?;
        Ok(())
    }

    /// Meticulous App installation JWT (`/api/v1/integration/*`).
    pub fn check_app(
        &self,
        installation_id: AppInstallationId,
        policy: &OrgPolicy,
    ) -> Result<(), ()> {
        let key = format!("appi:{installation_id}");
        let (p_cap, p_rate) =
            Self::window(policy.app_rl_primary_max, policy.app_rl_primary_period_secs);
        let (s_cap, s_rate) = Self::window(
            policy.app_rl_secondary_max,
            policy.app_rl_secondary_period_secs,
        );
        let mut map = self.app_buckets.lock().unwrap_or_else(|e| e.into_inner());
        let entry = map
            .entry(key)
            .or_insert_with(|| (Bucket::new(p_cap), Bucket::new(s_cap)));
        entry.0.try_consume(p_cap, p_rate)?;
        entry.1.try_consume(s_cap, s_rate)?;
        Ok(())
    }
}
