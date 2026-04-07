//! Tool binary tracking and versioning.
//!
//! Tracks tool binaries (executables) used in CI runs with SHA256 hashes.

use chrono::{DateTime, Utc};
use met_core::ids::{JobId, RunId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Errors from tool tracking operations.
#[derive(Debug, Error)]
pub enum ToolError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Tool not found: {0}")]
    NotFound(String),

    #[error("Hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },
}

pub type Result<T> = std::result::Result<T, ToolError>;

/// A tracked tool binary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedTool {
    /// Unique tool ID.
    pub id: Uuid,
    /// Tool name (e.g., "cargo", "npm", "docker").
    pub name: String,
    /// Tool version string.
    pub version: String,
    /// SHA256 hash of the binary.
    pub sha256: String,
    /// Original file path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// First seen timestamp.
    pub first_seen_at: DateTime<Utc>,
    /// Number of times used.
    pub usage_count: u64,
    /// Additional metadata.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl TrackedTool {
    /// Create a new tracked tool.
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        sha256: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            version: version.into(),
            sha256: sha256.into(),
            path: None,
            first_seen_at: Utc::now(),
            usage_count: 0,
            metadata: HashMap::new(),
        }
    }

    /// Set the path.
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Add metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Generate a unique key for lookup.
    pub fn key(&self) -> String {
        format!("{}:{}:{}", self.name, self.version, &self.sha256[..16])
    }
}

/// Usage record for a tool in a specific run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUsage {
    /// The tool ID.
    pub tool_id: Uuid,
    /// Tool name.
    pub tool_name: String,
    /// Tool version.
    pub tool_version: String,
    /// Tool SHA256.
    pub tool_sha256: String,
    /// Run where the tool was used.
    pub run_id: RunId,
    /// Job where the tool was used.
    pub job_id: JobId,
    /// When the tool was invoked.
    pub used_at: DateTime<Utc>,
    /// Command that was run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Exit code of the command.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Duration of the command in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

impl ToolUsage {
    /// Create a new usage record.
    pub fn new(tool: &TrackedTool, run_id: RunId, job_id: JobId) -> Self {
        Self {
            tool_id: tool.id,
            tool_name: tool.name.clone(),
            tool_version: tool.version.clone(),
            tool_sha256: tool.sha256.clone(),
            run_id,
            job_id,
            used_at: Utc::now(),
            command: None,
            exit_code: None,
            duration_ms: None,
        }
    }

    /// Set the command.
    pub fn with_command(mut self, command: impl Into<String>) -> Self {
        self.command = Some(command.into());
        self
    }

    /// Set the result.
    pub fn with_result(mut self, exit_code: i32, duration_ms: u64) -> Self {
        self.exit_code = Some(exit_code);
        self.duration_ms = Some(duration_ms);
        self
    }
}

/// Registry of tracked tools.
pub struct ToolRegistry {
    /// Tools indexed by key.
    tools: Arc<RwLock<HashMap<String, TrackedTool>>>,
    /// Tools indexed by SHA256.
    by_sha: Arc<RwLock<HashMap<String, Vec<Uuid>>>>,
    /// Usage records.
    usages: Arc<RwLock<Vec<ToolUsage>>>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    /// Create a new registry.
    pub fn new() -> Self {
        Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
            by_sha: Arc::new(RwLock::new(HashMap::new())),
            usages: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Compute SHA256 hash of a file.
    pub fn hash_file(path: &Path) -> Result<String> {
        let mut file = std::fs::File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 8192];

        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }

        Ok(format!("{:x}", hasher.finalize()))
    }

    /// Register a tool.
    pub async fn register(&self, tool: TrackedTool) -> TrackedTool {
        let key = tool.key();
        let sha = tool.sha256.clone();
        let id = tool.id;

        let mut tools = self.tools.write().await;
        tools.insert(key, tool.clone());

        let mut by_sha = self.by_sha.write().await;
        by_sha.entry(sha).or_default().push(id);

        tool
    }

    /// Get a tool by its key.
    pub async fn get(&self, name: &str, version: &str, sha256_prefix: &str) -> Option<TrackedTool> {
        let key = format!("{}:{}:{}", name, version, sha256_prefix);
        let tools = self.tools.read().await;
        tools.get(&key).cloned()
    }

    /// Get all tools with a specific SHA256.
    pub async fn get_by_sha(&self, sha256: &str) -> Vec<TrackedTool> {
        let by_sha = self.by_sha.read().await;
        let tools = self.tools.read().await;

        by_sha
            .get(sha256)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| tools.values().find(|t| &t.id == id).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Record a tool usage.
    pub async fn record_usage(&self, usage: ToolUsage) {
        let mut usages = self.usages.write().await;
        usages.push(usage);

        // Update usage count
        let mut tools = self.tools.write().await;
        let key = format!(
            "{}:{}:{}",
            usages.last().unwrap().tool_name,
            usages.last().unwrap().tool_version,
            &usages.last().unwrap().tool_sha256[..16]
        );
        if let Some(tool) = tools.get_mut(&key) {
            tool.usage_count += 1;
        }
    }

    /// Get all usages for a run.
    pub async fn get_usages_by_run(&self, run_id: RunId) -> Vec<ToolUsage> {
        let usages = self.usages.read().await;
        usages
            .iter()
            .filter(|u| u.run_id == run_id)
            .cloned()
            .collect()
    }

    /// Get all usages for a tool SHA.
    pub async fn get_usages_by_sha(&self, sha256: &str) -> Vec<ToolUsage> {
        let usages = self.usages.read().await;
        usages
            .iter()
            .filter(|u| u.tool_sha256 == sha256)
            .cloned()
            .collect()
    }

    /// Get total registered tool count.
    pub async fn tool_count(&self) -> usize {
        let tools = self.tools.read().await;
        tools.len()
    }

    /// Get total usage count.
    pub async fn usage_count(&self) -> usize {
        let usages = self.usages.read().await;
        usages.len()
    }

    /// List all registered tools.
    pub async fn list_tools(&self) -> Vec<TrackedTool> {
        let tools = self.tools.read().await;
        tools.values().cloned().collect()
    }

    /// Search tools by name pattern.
    pub async fn search(&self, pattern: &str) -> Vec<TrackedTool> {
        let pattern_lower = pattern.to_lowercase();
        let tools = self.tools.read().await;
        tools
            .values()
            .filter(|t| t.name.to_lowercase().contains(&pattern_lower))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_and_get() {
        let registry = ToolRegistry::new();

        let tool = TrackedTool::new(
            "cargo",
            "1.75.0",
            "abc123def456789012345678901234567890123456789012345678901234",
        );
        registry.register(tool.clone()).await;

        let retrieved = registry.get("cargo", "1.75.0", "abc123def4567890").await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "cargo");
    }

    #[tokio::test]
    async fn test_record_usage() {
        let registry = ToolRegistry::new();

        let tool = TrackedTool::new(
            "npm",
            "10.0.0",
            "sha256hash1234567890123456789012345678901234567890123456789012",
        );
        let tool = registry.register(tool).await;

        let run_id = RunId::new();
        let job_id = JobId::new();
        let usage = ToolUsage::new(&tool, run_id, job_id)
            .with_command("npm install")
            .with_result(0, 5000);

        registry.record_usage(usage).await;

        let usages = registry.get_usages_by_run(run_id).await;
        assert_eq!(usages.len(), 1);
        assert_eq!(usages[0].command, Some("npm install".to_string()));
    }

    #[tokio::test]
    async fn test_get_by_sha() {
        let registry = ToolRegistry::new();
        let sha = "unique_sha_hash_1234567890123456789012345678901234567890123456";

        let tool1 = TrackedTool::new("tool1", "1.0.0", sha);
        let tool2 = TrackedTool::new("tool2", "2.0.0", sha);

        registry.register(tool1).await;
        registry.register(tool2).await;

        let tools = registry.get_by_sha(sha).await;
        assert_eq!(tools.len(), 2);
    }

    #[tokio::test]
    async fn test_search() {
        let registry = ToolRegistry::new();

        registry
            .register(TrackedTool::new(
                "cargo",
                "1.75.0",
                "hash1234567890123456789012345678901234567890123456789012",
            ))
            .await;
        registry
            .register(TrackedTool::new(
                "cargo-watch",
                "8.0.0",
                "hash2345678901234567890123456789012345678901234567890123",
            ))
            .await;
        registry
            .register(TrackedTool::new(
                "npm",
                "10.0.0",
                "hash3456789012345678901234567890123456789012345678901234",
            ))
            .await;

        let results = registry.search("cargo").await;
        assert_eq!(results.len(), 2);
    }
}
