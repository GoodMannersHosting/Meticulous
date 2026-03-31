//! Execution context for pipeline runs.
//!
//! The context holds all state needed during pipeline execution,
//! including resolved secrets, variables, and artifact references.

use indexmap::IndexMap;
use met_core::ids::{JobId, OrganizationId, PipelineId, ProjectId, RunId};
use met_parser::{EnvValue, PipelineIR, SecretRef};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Resolved secret value (decrypted).
#[derive(Clone)]
pub struct ResolvedSecret {
    pub name: String,
    pub value: secrecy::SecretString,
}

/// Artifact reference from a completed job.
#[derive(Debug, Clone)]
pub struct ArtifactRef {
    pub job_id: JobId,
    pub name: String,
    pub storage_path: String,
    pub content_type: Option<String>,
    pub size_bytes: u64,
}

/// Cache hit result.
#[derive(Debug, Clone)]
pub struct CacheHit {
    pub key: String,
    pub storage_path: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Execution context for a pipeline run.
#[derive(Clone, Debug)]
pub struct ExecutionContext {
    inner: Arc<ExecutionContextInner>,
}

struct ExecutionContextInner {
    run_id: RunId,
    pipeline_id: PipelineId,
    org_id: OrganizationId,
    project_id: Option<ProjectId>,
    pipeline_ir: PipelineIR,
    triggered_by: String,
    commit_sha: Option<String>,
    branch: Option<String>,
    trace_id: Option<String>,

    variables: RwLock<IndexMap<String, String>>,
    secrets: RwLock<IndexMap<String, ResolvedSecret>>,
    artifacts: RwLock<IndexMap<String, ArtifactRef>>,
    cache_hits: RwLock<IndexMap<String, CacheHit>>,
    job_outputs: RwLock<IndexMap<JobId, IndexMap<String, String>>>,
}

impl std::fmt::Debug for ExecutionContextInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionContextInner")
            .field("run_id", &self.run_id)
            .field("pipeline_id", &self.pipeline_id)
            .field("pipeline_name", &self.pipeline_ir.name)
            .finish_non_exhaustive()
    }
}

impl ExecutionContext {
    /// Create a new execution context.
    pub fn new(
        run_id: RunId,
        org_id: OrganizationId,
        pipeline_ir: PipelineIR,
        triggered_by: impl Into<String>,
    ) -> Self {
        let pipeline_id = pipeline_ir.id;
        let project_id = pipeline_ir.project_id;
        let variables = pipeline_ir.variables.clone();

        Self {
            inner: Arc::new(ExecutionContextInner {
                run_id,
                pipeline_id,
                org_id,
                project_id,
                pipeline_ir,
                triggered_by: triggered_by.into(),
                commit_sha: None,
                branch: None,
                trace_id: None,
                variables: RwLock::new(variables),
                secrets: RwLock::new(IndexMap::new()),
                artifacts: RwLock::new(IndexMap::new()),
                cache_hits: RwLock::new(IndexMap::new()),
                job_outputs: RwLock::new(IndexMap::new()),
            }),
        }
    }

    pub fn run_id(&self) -> RunId {
        self.inner.run_id
    }

    pub fn pipeline_id(&self) -> PipelineId {
        self.inner.pipeline_id
    }

    pub fn org_id(&self) -> OrganizationId {
        self.inner.org_id
    }

    pub fn project_id(&self) -> Option<ProjectId> {
        self.inner.project_id
    }

    pub fn pipeline(&self) -> &PipelineIR {
        &self.inner.pipeline_ir
    }

    pub fn triggered_by(&self) -> &str {
        &self.inner.triggered_by
    }

    pub fn commit_sha(&self) -> Option<&str> {
        self.inner.commit_sha.as_deref()
    }

    pub fn branch(&self) -> Option<&str> {
        self.inner.branch.as_deref()
    }

    pub fn trace_id(&self) -> Option<&str> {
        self.inner.trace_id.as_deref()
    }

    /// Get a variable value.
    pub async fn get_variable(&self, name: &str) -> Option<String> {
        self.inner.variables.read().await.get(name).cloned()
    }

    /// Set a variable value.
    pub async fn set_variable(&self, name: impl Into<String>, value: impl Into<String>) {
        self.inner
            .variables
            .write()
            .await
            .insert(name.into(), value.into());
    }

    /// Get all variables.
    pub async fn variables(&self) -> IndexMap<String, String> {
        self.inner.variables.read().await.clone()
    }

    /// Register a resolved secret.
    pub async fn register_secret(&self, name: impl Into<String>, value: secrecy::SecretString) {
        let name = name.into();
        self.inner.secrets.write().await.insert(
            name.clone(),
            ResolvedSecret {
                name,
                value,
            },
        );
    }

    /// Get a resolved secret.
    pub async fn get_secret(&self, name: &str) -> Option<ResolvedSecret> {
        self.inner.secrets.read().await.get(name).cloned()
    }

    /// Check if a secret is registered.
    pub async fn has_secret(&self, name: &str) -> bool {
        self.inner.secrets.read().await.contains_key(name)
    }

    /// Get secret references from the pipeline.
    pub fn secret_refs(&self) -> &IndexMap<String, SecretRef> {
        &self.inner.pipeline_ir.secret_refs
    }

    /// Register an artifact.
    pub async fn register_artifact(&self, key: impl Into<String>, artifact: ArtifactRef) {
        self.inner.artifacts.write().await.insert(key.into(), artifact);
    }

    /// Get an artifact by key.
    pub async fn get_artifact(&self, key: &str) -> Option<ArtifactRef> {
        self.inner.artifacts.read().await.get(key).cloned()
    }

    /// Get all artifacts from a job.
    pub async fn job_artifacts(&self, job_id: &JobId) -> Vec<ArtifactRef> {
        self.inner
            .artifacts
            .read()
            .await
            .values()
            .filter(|a| &a.job_id == job_id)
            .cloned()
            .collect()
    }

    /// Register a cache hit.
    pub async fn register_cache_hit(&self, key: impl Into<String>, hit: CacheHit) {
        self.inner.cache_hits.write().await.insert(key.into(), hit);
    }

    /// Get a cache hit.
    pub async fn get_cache_hit(&self, key: &str) -> Option<CacheHit> {
        self.inner.cache_hits.read().await.get(key).cloned()
    }

    /// Register job outputs.
    pub async fn set_job_outputs(&self, job_id: JobId, outputs: IndexMap<String, String>) {
        self.inner.job_outputs.write().await.insert(job_id, outputs);
    }

    /// Get outputs from a completed job.
    pub async fn get_job_outputs(&self, job_id: &JobId) -> Option<IndexMap<String, String>> {
        self.inner.job_outputs.read().await.get(job_id).cloned()
    }

    /// Resolve an environment value to its final string.
    pub async fn resolve_env_value(&self, value: &EnvValue) -> Option<String> {
        match value {
            EnvValue::Literal(s) => Some(s.clone()),
            EnvValue::SecretRef(name) => {
                self.get_secret(name)
                    .await
                    .map(|s| {
                        use secrecy::ExposeSecret;
                        s.value.expose_secret().to_string()
                    })
            }
            EnvValue::Expression(expr) => {
                self.evaluate_expression(expr).await
            }
        }
    }

    /// Evaluate a simple variable expression like ${{ variables.foo }}.
    async fn evaluate_expression(&self, expr: &str) -> Option<String> {
        let trimmed = expr.trim();
        
        if let Some(var_name) = trimmed.strip_prefix("variables.") {
            return self.get_variable(var_name).await;
        }
        
        if let Some(rest) = trimmed.strip_prefix("jobs.") {
            if let Some((job_name, output_name)) = rest.split_once(".outputs.") {
                let jobs = &self.inner.pipeline_ir.jobs;
                if let Some(job) = jobs.iter().find(|j| j.name == job_name) {
                    if let Some(outputs) = self.get_job_outputs(&job.id).await {
                        return outputs.get(output_name).cloned();
                    }
                }
            }
        }

        None
    }
}
