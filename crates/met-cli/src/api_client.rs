use reqwest::{Client, StatusCode};
use serde::{de::DeserializeOwned, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use thiserror::Error;
use tracing::debug;

const MAX_RETRIES: u32 = 3;
const INITIAL_BACKOFF_MS: u64 = 500;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("API error ({status}): {message}")]
    Api { status: u16, message: String },

    #[error("Authentication required. Run `met auth login` first.")]
    Unauthorized,

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, ApiError>;

#[derive(Clone)]
pub struct ApiClient {
    client: Client,
    base_url: String,
    token: Arc<Mutex<Option<String>>>,
    verbose: bool,
}

impl ApiClient {
    pub fn new(base_url: &str, token: Option<String>, verbose: bool) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            token: Arc::new(Mutex::new(token)),
            verbose,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn set_token(&self, token: String) {
        let mut guard = self.token.lock().expect("token lock poisoned");
        *guard = Some(token);
    }

    pub fn has_token(&self) -> bool {
        let guard = self.token.lock().expect("token lock poisoned");
        guard.is_some()
    }

    pub fn ws_url(&self, path: &str) -> String {
        let base = self
            .base_url
            .replace("http://", "ws://")
            .replace("https://", "wss://");
        format!("{}/api/v1{}", base, path)
    }

    fn url(&self, path: &str) -> String {
        let base = self.base_url.trim_end_matches('/');
        // Auth, admin, and OAuth live at the API root (not under `/api/v1`).
        let root_paths = [
            "/auth/",
            "/admin/",
            "/oauth/",
            "/health",
            "/ready",
            "/live",
        ];
        let use_root = root_paths.iter().any(|p| path == *p || path.starts_with(p));
        if use_root {
            format!("{base}{path}")
        } else {
            format!("{base}/api/v1{path}")
        }
    }

    fn add_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let guard = self.token.lock().expect("token lock poisoned");
        if let Some(ref token) = *guard {
            if token.starts_with("eyJ") {
                req.header("Authorization", format!("Bearer {}", token))
            } else {
                req.header("Authorization", format!("Token {}", token))
            }
        } else {
            req
        }
    }

    fn log_request(&self, method: &str, url: &str) {
        if self.verbose {
            eprintln!("  {} {}", method, url);
        }
    }

    fn log_response(&self, status: StatusCode) {
        if self.verbose {
            eprintln!("  <- {}", status);
        }
    }

    async fn handle_response<T: DeserializeOwned>(&self, resp: reqwest::Response) -> Result<T> {
        let status = resp.status();
        self.log_response(status);

        if status == StatusCode::UNAUTHORIZED {
            return Err(ApiError::Unauthorized);
        }

        if status == StatusCode::NOT_FOUND {
            let text = resp.text().await.unwrap_or_default();
            return Err(ApiError::NotFound(text));
        }

        if !status.is_success() {
            let text = resp
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ApiError::Api {
                status: status.as_u16(),
                message: text,
            });
        }

        resp.json::<T>()
            .await
            .map_err(|e| ApiError::InvalidResponse(format!("Failed to parse response: {}", e)))
    }

    async fn handle_empty_response(&self, resp: reqwest::Response) -> Result<()> {
        let status = resp.status();
        self.log_response(status);

        if status == StatusCode::UNAUTHORIZED {
            return Err(ApiError::Unauthorized);
        }

        if !status.is_success() {
            let text = resp
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ApiError::Api {
                status: status.as_u16(),
                message: text,
            });
        }

        Ok(())
    }

    fn is_retryable(err: &ApiError) -> bool {
        match err {
            ApiError::Http(e) => e.is_timeout() || e.is_connect(),
            ApiError::Api { status, .. } => matches!(*status, 429 | 502 | 503 | 504),
            _ => false,
        }
    }

    async fn retry_delay(attempt: u32) {
        let delay = INITIAL_BACKOFF_MS * 2u64.pow(attempt);
        debug!(delay_ms = delay, attempt, "Retrying after backoff");
        tokio::time::sleep(Duration::from_millis(delay)).await;
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = self.url(path);

        for attempt in 0..=MAX_RETRIES {
            self.log_request("GET", &url);
            let req = self.client.get(&url);
            let req = self.add_auth(req);
            match req.send().await {
                Ok(resp) => match self.handle_response(resp).await {
                    Ok(val) => return Ok(val),
                    Err(e) if Self::is_retryable(&e) && attempt < MAX_RETRIES => {
                        Self::retry_delay(attempt).await;
                        continue;
                    }
                    Err(e) => return Err(e),
                },
                Err(e) if attempt < MAX_RETRIES => {
                    let api_err = ApiError::Http(e);
                    if Self::is_retryable(&api_err) {
                        Self::retry_delay(attempt).await;
                        continue;
                    }
                    return Err(api_err);
                }
                Err(e) => return Err(ApiError::Http(e)),
            }
        }
        unreachable!()
    }

    pub async fn get_with_query<T: DeserializeOwned, Q: Serialize>(
        &self,
        path: &str,
        query: &Q,
    ) -> Result<T> {
        let url = self.url(path);
        self.log_request("GET", &url);
        let req = self.client.get(&url).query(query);
        let req = self.add_auth(req);
        let resp = req.send().await?;
        self.handle_response(resp).await
    }

    pub async fn post<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let url = self.url(path);

        for attempt in 0..=MAX_RETRIES {
            self.log_request("POST", &url);
            let req = self.client.post(&url).json(body);
            let req = self.add_auth(req);
            match req.send().await {
                Ok(resp) => match self.handle_response(resp).await {
                    Ok(val) => return Ok(val),
                    Err(e) if Self::is_retryable(&e) && attempt < MAX_RETRIES => {
                        Self::retry_delay(attempt).await;
                        continue;
                    }
                    Err(e) => return Err(e),
                },
                Err(e) if attempt < MAX_RETRIES => {
                    let api_err = ApiError::Http(e);
                    if Self::is_retryable(&api_err) {
                        Self::retry_delay(attempt).await;
                        continue;
                    }
                    return Err(api_err);
                }
                Err(e) => return Err(ApiError::Http(e)),
            }
        }
        unreachable!()
    }

    pub async fn post_empty<B: Serialize>(&self, path: &str, body: &B) -> Result<()> {
        let url = self.url(path);
        self.log_request("POST", &url);
        let req = self.client.post(&url).json(body);
        let req = self.add_auth(req);
        let resp = req.send().await?;
        self.handle_empty_response(resp).await
    }

    pub async fn put<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let url = self.url(path);
        self.log_request("PUT", &url);
        let req = self.client.put(&url).json(body);
        let req = self.add_auth(req);
        let resp = req.send().await?;
        self.handle_response(resp).await
    }

    pub async fn patch<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let url = self.url(path);
        self.log_request("PATCH", &url);
        let req = self.client.patch(&url).json(body);
        let req = self.add_auth(req);
        let resp = req.send().await?;
        self.handle_response(resp).await
    }

    pub async fn delete(&self, path: &str) -> Result<()> {
        let url = self.url(path);
        self.log_request("DELETE", &url);
        let req = self.client.delete(&url);
        let req = self.add_auth(req);
        let resp = req.send().await?;
        self.handle_empty_response(resp).await
    }
}
