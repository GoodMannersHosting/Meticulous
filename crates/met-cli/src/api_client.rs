//! HTTP client for the Meticulous API.

use reqwest::{Client, StatusCode};
use serde::{de::DeserializeOwned, Serialize};
use std::time::Duration;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    
    #[error("API error ({status}): {message}")]
    Api { status: u16, message: String },
    
    #[error("Authentication required")]
    Unauthorized,
    
    #[error("Not found: {0}")]
    NotFound(String),
    
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
}

pub type Result<T> = std::result::Result<T, ApiError>;

/// HTTP client for the Meticulous API.
#[derive(Clone)]
pub struct ApiClient {
    client: Client,
    base_url: String,
    token: Option<String>,
}

impl ApiClient {
    /// Create a new API client.
    pub fn new(base_url: &str, token: Option<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            token,
        }
    }

    /// Get the WebSocket URL for log streaming.
    pub fn ws_url(&self, path: &str) -> String {
        let base = self.base_url
            .replace("http://", "ws://")
            .replace("https://", "wss://");
        format!("{}/api/v1{}", base, path)
    }

    fn url(&self, path: &str) -> String {
        format!("{}/api/v1{}", self.base_url, path)
    }

    fn add_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(ref token) = self.token {
            req.header("Authorization", format!("Token {}", token))
        } else {
            req
        }
    }

    async fn handle_response<T: DeserializeOwned>(&self, resp: reqwest::Response) -> Result<T> {
        let status = resp.status();

        if status == StatusCode::UNAUTHORIZED {
            return Err(ApiError::Unauthorized);
        }

        if status == StatusCode::NOT_FOUND {
            let text = resp.text().await.unwrap_or_default();
            return Err(ApiError::NotFound(text));
        }

        if !status.is_success() {
            let text = resp.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ApiError::Api {
                status: status.as_u16(),
                message: text,
            });
        }

        resp.json::<T>().await.map_err(|e| {
            ApiError::InvalidResponse(format!("Failed to parse response: {}", e))
        })
    }

    /// Perform a GET request.
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let req = self.client.get(self.url(path));
        let req = self.add_auth(req);
        let resp = req.send().await?;
        self.handle_response(resp).await
    }

    /// Perform a GET request with query parameters.
    pub async fn get_with_query<T: DeserializeOwned, Q: Serialize>(&self, path: &str, query: &Q) -> Result<T> {
        let req = self.client.get(self.url(path)).query(query);
        let req = self.add_auth(req);
        let resp = req.send().await?;
        self.handle_response(resp).await
    }

    /// Perform a POST request.
    pub async fn post<T: DeserializeOwned, B: Serialize>(&self, path: &str, body: &B) -> Result<T> {
        let req = self.client.post(self.url(path)).json(body);
        let req = self.add_auth(req);
        let resp = req.send().await?;
        self.handle_response(resp).await
    }

    /// Perform a DELETE request.
    pub async fn delete(&self, path: &str) -> Result<()> {
        let req = self.client.delete(self.url(path));
        let req = self.add_auth(req);
        let resp = req.send().await?;
        let status = resp.status();

        if !status.is_success() {
            let text = resp.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ApiError::Api {
                status: status.as_u16(),
                message: text,
            });
        }

        Ok(())
    }
}
