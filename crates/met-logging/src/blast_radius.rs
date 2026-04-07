//! Blast radius analysis for compromised tools.
//!
//! Determines which runs, jobs, and pipelines are affected when a tool
//! binary is identified as compromised.

use crate::tools::{ToolRegistry, ToolUsage, TrackedTool};
use chrono::{DateTime, Utc};
use met_core::ids::{PipelineId, ProjectId, RunId};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use thiserror::Error;

/// Errors from blast radius queries.
#[derive(Debug, Error)]
pub enum BlastRadiusError {
    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Invalid query: {0}")]
    InvalidQuery(String),
}

pub type Result<T> = std::result::Result<T, BlastRadiusError>;

/// Query parameters for blast radius analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlastRadiusQuery {
    /// SHA256 of the compromised tool.
    pub tool_sha256: String,
    /// Optional: Filter to runs after this date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<DateTime<Utc>>,
    /// Optional: Filter to runs before this date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<DateTime<Utc>>,
    /// Optional: Filter to specific project.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
    /// Optional: Filter to specific pipeline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pipeline_id: Option<PipelineId>,
}

impl BlastRadiusQuery {
    /// Create a new query for a tool SHA.
    pub fn for_sha(sha256: impl Into<String>) -> Self {
        Self {
            tool_sha256: sha256.into(),
            after: None,
            before: None,
            project_id: None,
            pipeline_id: None,
        }
    }

    /// Filter to a time range.
    pub fn with_time_range(
        mut self,
        after: Option<DateTime<Utc>>,
        before: Option<DateTime<Utc>>,
    ) -> Self {
        self.after = after;
        self.before = before;
        self
    }

    /// Filter to a project.
    pub fn with_project(mut self, project_id: ProjectId) -> Self {
        self.project_id = Some(project_id);
        self
    }

    /// Filter to a pipeline.
    pub fn with_pipeline(mut self, pipeline_id: PipelineId) -> Self {
        self.pipeline_id = Some(pipeline_id);
        self
    }
}

/// Information about an affected run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffectedRun {
    /// The run ID.
    pub run_id: RunId,
    /// Project ID (if known).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
    /// Pipeline ID (if known).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pipeline_id: Option<PipelineId>,
    /// When the tool was used.
    pub used_at: DateTime<Utc>,
    /// Command that was run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// All usages in this run.
    pub usages: Vec<ToolUsage>,
}

/// Result of a blast radius query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlastRadiusResult {
    /// The query that was run.
    pub query: BlastRadiusQuery,
    /// Tool information.
    pub tool: Option<TrackedTool>,
    /// When the query was executed.
    pub computed_at: DateTime<Utc>,
    /// Affected runs.
    pub affected_runs: Vec<AffectedRun>,
    /// Summary statistics.
    pub summary: BlastRadiusSummary,
}

/// Summary of blast radius impact.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BlastRadiusSummary {
    /// Total number of affected runs.
    pub total_runs: usize,
    /// Total number of affected jobs.
    pub total_jobs: usize,
    /// Total number of affected projects.
    pub total_projects: usize,
    /// Total number of affected pipelines.
    pub total_pipelines: usize,
    /// Earliest usage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub earliest_usage: Option<DateTime<Utc>>,
    /// Latest usage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_usage: Option<DateTime<Utc>>,
}

impl BlastRadiusResult {
    /// Check if the query matched any affected runs.
    pub fn has_impact(&self) -> bool {
        !self.affected_runs.is_empty()
    }

    /// Get unique project IDs.
    pub fn affected_projects(&self) -> HashSet<ProjectId> {
        self.affected_runs
            .iter()
            .filter_map(|r| r.project_id)
            .collect()
    }

    /// Get unique pipeline IDs.
    pub fn affected_pipelines(&self) -> HashSet<PipelineId> {
        self.affected_runs
            .iter()
            .filter_map(|r| r.pipeline_id)
            .collect()
    }

    /// Get runs in time order (oldest first).
    pub fn runs_chronological(&self) -> Vec<&AffectedRun> {
        let mut runs: Vec<_> = self.affected_runs.iter().collect();
        runs.sort_by_key(|r| r.used_at);
        runs
    }
}

/// Execute blast radius queries.
pub struct BlastRadiusAnalyzer<'a> {
    registry: &'a ToolRegistry,
}

impl<'a> BlastRadiusAnalyzer<'a> {
    /// Create a new analyzer.
    pub fn new(registry: &'a ToolRegistry) -> Self {
        Self { registry }
    }

    /// Execute a blast radius query.
    pub async fn query(&self, query: BlastRadiusQuery) -> Result<BlastRadiusResult> {
        // Get all usages for this SHA
        let usages = self.registry.get_usages_by_sha(&query.tool_sha256).await;
        let tool = self
            .registry
            .get_by_sha(&query.tool_sha256)
            .await
            .first()
            .cloned();

        // Filter usages by query parameters
        let filtered_usages: Vec<_> = usages
            .into_iter()
            .filter(|u| {
                if let Some(after) = query.after {
                    if u.used_at < after {
                        return false;
                    }
                }
                if let Some(before) = query.before {
                    if u.used_at > before {
                        return false;
                    }
                }
                true
            })
            .collect();

        // Group usages by run
        let mut by_run: HashMap<RunId, Vec<ToolUsage>> = HashMap::new();
        for usage in filtered_usages {
            by_run.entry(usage.run_id).or_default().push(usage);
        }

        // Build affected runs
        let affected_runs: Vec<AffectedRun> = by_run
            .into_iter()
            .map(|(run_id, usages)| {
                let first_usage = usages.first().unwrap();
                AffectedRun {
                    run_id,
                    project_id: None, // Would be looked up from DB in production
                    pipeline_id: None,
                    used_at: first_usage.used_at,
                    command: first_usage.command.clone(),
                    usages,
                }
            })
            .collect();

        // Compute summary
        let job_ids: HashSet<_> = affected_runs
            .iter()
            .flat_map(|r| r.usages.iter().map(|u| u.job_id))
            .collect();

        let earliest = affected_runs.iter().map(|r| r.used_at).min();
        let latest = affected_runs.iter().map(|r| r.used_at).max();

        let summary = BlastRadiusSummary {
            total_runs: affected_runs.len(),
            total_jobs: job_ids.len(),
            total_projects: affected_runs
                .iter()
                .filter_map(|r| r.project_id)
                .collect::<HashSet<_>>()
                .len(),
            total_pipelines: affected_runs
                .iter()
                .filter_map(|r| r.pipeline_id)
                .collect::<HashSet<_>>()
                .len(),
            earliest_usage: earliest,
            latest_usage: latest,
        };

        Ok(BlastRadiusResult {
            query,
            tool,
            computed_at: Utc::now(),
            affected_runs,
            summary,
        })
    }

    /// Query multiple SHAs and combine results.
    pub async fn query_multiple(&self, shas: &[String]) -> Result<Vec<BlastRadiusResult>> {
        let mut results = Vec::new();
        for sha in shas {
            let query = BlastRadiusQuery::for_sha(sha);
            results.push(self.query(query).await?);
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{ToolUsage, TrackedTool};
    use met_core::ids::JobId;

    #[tokio::test]
    async fn test_blast_radius_query() {
        let registry = ToolRegistry::new();
        let sha = "compromised_sha_1234567890123456789012345678901234567890123456";

        // Register tool
        let tool = TrackedTool::new("malware", "1.0.0", sha);
        let tool = registry.register(tool).await;

        // Record some usages
        for _ in 0..3 {
            let usage =
                ToolUsage::new(&tool, RunId::new(), JobId::new()).with_command("./malware --evil");
            registry.record_usage(usage).await;
        }

        // Query blast radius
        let analyzer = BlastRadiusAnalyzer::new(&registry);
        let query = BlastRadiusQuery::for_sha(sha);
        let result = analyzer.query(query).await.unwrap();

        assert!(result.has_impact());
        assert_eq!(result.summary.total_runs, 3);
        assert!(result.tool.is_some());
    }

    #[tokio::test]
    async fn test_no_impact() {
        let registry = ToolRegistry::new();
        let analyzer = BlastRadiusAnalyzer::new(&registry);

        let query = BlastRadiusQuery::for_sha("nonexistent_sha_1234567890");
        let result = analyzer.query(query).await.unwrap();

        assert!(!result.has_impact());
        assert_eq!(result.summary.total_runs, 0);
    }

    #[tokio::test]
    async fn test_time_filtering() {
        let registry = ToolRegistry::new();
        let sha = "time_test_sha_1234567890123456789012345678901234567890123456";

        let tool = TrackedTool::new("tool", "1.0.0", sha);
        let tool = registry.register(tool).await;

        let usage = ToolUsage::new(&tool, RunId::new(), JobId::new());
        registry.record_usage(usage).await;

        let analyzer = BlastRadiusAnalyzer::new(&registry);

        // Query with future "after" should return nothing
        let query = BlastRadiusQuery::for_sha(sha)
            .with_time_range(Some(Utc::now() + chrono::Duration::hours(1)), None);
        let result = analyzer.query(query).await.unwrap();
        assert!(!result.has_impact());

        // Query with past "after" should return the usage
        let query = BlastRadiusQuery::for_sha(sha)
            .with_time_range(Some(Utc::now() - chrono::Duration::hours(1)), None);
        let result = analyzer.query(query).await.unwrap();
        assert!(result.has_impact());
    }
}
