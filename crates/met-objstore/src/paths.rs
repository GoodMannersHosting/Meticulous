//! Object key path conventions for Meticulous artifacts.
//!
//! This module defines standardized paths for storing various types of objects
//! in the object store, ensuring consistency across the platform.

use std::fmt;

/// An object key representing a path in the object store.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ObjectKey {
    key: String,
}

impl ObjectKey {
    /// Create a new object key from a raw string.
    pub fn new(key: impl Into<String>) -> Self {
        Self { key: key.into() }
    }

    /// Get the key as a string slice.
    pub fn as_str(&self) -> &str {
        &self.key
    }

    /// Get the parent prefix of this key.
    pub fn parent(&self) -> Option<&str> {
        self.key.rsplit_once('/').map(|(parent, _)| parent)
    }

    /// Get the filename portion of this key.
    pub fn filename(&self) -> Option<&str> {
        self.key.rsplit_once('/').map(|(_, name)| name)
    }

    /// Check if this key starts with the given prefix.
    pub fn starts_with(&self, prefix: &str) -> bool {
        self.key.starts_with(prefix)
    }

    /// Join this key with a child path.
    pub fn join(&self, child: &str) -> Self {
        if self.key.is_empty() {
            Self::new(child)
        } else {
            Self::new(format!("{}/{}", self.key, child))
        }
    }
}

impl fmt::Display for ObjectKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.key)
    }
}

impl From<String> for ObjectKey {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<&str> for ObjectKey {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl AsRef<str> for ObjectKey {
    fn as_ref(&self) -> &str {
        &self.key
    }
}

/// Builder for constructing standardized object keys.
pub struct ObjectKeyBuilder {
    organization: Option<String>,
    project: Option<String>,
}

impl ObjectKeyBuilder {
    /// Create a new key builder.
    pub fn new() -> Self {
        Self { organization: None, project: None }
    }

    /// Set the organization context.
    pub fn organization(mut self, org: impl Into<String>) -> Self {
        self.organization = Some(org.into());
        self
    }

    /// Set the project context.
    pub fn project(mut self, project: impl Into<String>) -> Self {
        self.project = Some(project.into());
        self
    }

    fn base_prefix(&self) -> String {
        match (&self.organization, &self.project) {
            (Some(org), Some(proj)) => format!("orgs/{org}/projects/{proj}"),
            (Some(org), None) => format!("orgs/{org}"),
            (None, Some(proj)) => format!("projects/{proj}"),
            (None, None) => String::new(),
        }
    }

    /// Build a key for a pipeline run artifact.
    pub fn artifact(&self, run_id: &str, artifact_name: &str) -> ObjectKey {
        let base = self.base_prefix();
        ObjectKey::new(format!("{base}/runs/{run_id}/artifacts/{artifact_name}"))
    }

    /// Build a key for a job log file.
    pub fn job_log(&self, run_id: &str, job_name: &str) -> ObjectKey {
        let base = self.base_prefix();
        ObjectKey::new(format!("{base}/runs/{run_id}/logs/{job_name}.log"))
    }

    /// Build a key for a step log file.
    pub fn step_log(&self, run_id: &str, job_name: &str, step_name: &str) -> ObjectKey {
        let base = self.base_prefix();
        ObjectKey::new(format!("{base}/runs/{run_id}/logs/{job_name}/{step_name}.log"))
    }

    /// Build a key for an SBOM (Software Bill of Materials).
    pub fn sbom(&self, run_id: &str, format: SbomFormat) -> ObjectKey {
        let base = self.base_prefix();
        let ext = match format {
            SbomFormat::CycloneDx => "cdx.json",
            SbomFormat::Spdx => "spdx.json",
        };
        ObjectKey::new(format!("{base}/runs/{run_id}/sbom/{ext}"))
    }

    /// Build a key for a container image layer cache.
    pub fn layer_cache(&self, image_ref: &str, layer_digest: &str) -> ObjectKey {
        let base = self.base_prefix();
        let safe_image = image_ref.replace(['/', ':'], "_");
        ObjectKey::new(format!("{base}/cache/layers/{safe_image}/{layer_digest}"))
    }

    /// Build a key for a build cache entry.
    pub fn build_cache(&self, cache_key: &str) -> ObjectKey {
        let base = self.base_prefix();
        ObjectKey::new(format!("{base}/cache/builds/{cache_key}"))
    }

    /// Build a key for a temporary upload.
    pub fn temp_upload(&self, upload_id: &str) -> ObjectKey {
        let base = self.base_prefix();
        ObjectKey::new(format!("{base}/tmp/{upload_id}"))
    }
}

impl Default for ObjectKeyBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// SBOM format types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SbomFormat {
    /// CycloneDX format.
    CycloneDx,
    /// SPDX format.
    Spdx,
}

/// Convenience functions for common key patterns without organization/project context.
pub mod keys {
    use super::*;

    /// Key for a run artifact.
    pub fn artifact(run_id: &str, name: &str) -> ObjectKey {
        ObjectKey::new(format!("runs/{run_id}/artifacts/{name}"))
    }

    /// Key for a job log.
    pub fn job_log(run_id: &str, job_name: &str) -> ObjectKey {
        ObjectKey::new(format!("runs/{run_id}/logs/{job_name}.log"))
    }

    /// Key for a step log.
    pub fn step_log(run_id: &str, job_name: &str, step_name: &str) -> ObjectKey {
        ObjectKey::new(format!("runs/{run_id}/logs/{job_name}/{step_name}.log"))
    }

    /// Key for an SBOM.
    pub fn sbom(run_id: &str, format: SbomFormat) -> ObjectKey {
        ObjectKeyBuilder::new().sbom(run_id, format)
    }

    /// Prefix for all artifacts of a run.
    pub fn artifacts_prefix(run_id: &str) -> String {
        format!("runs/{run_id}/artifacts/")
    }

    /// Prefix for all logs of a run.
    pub fn logs_prefix(run_id: &str) -> String {
        format!("runs/{run_id}/logs/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_object_key_basic() {
        let key = ObjectKey::new("test/path/file.txt");
        assert_eq!(key.as_str(), "test/path/file.txt");
        assert_eq!(key.parent(), Some("test/path"));
        assert_eq!(key.filename(), Some("file.txt"));
    }

    #[test]
    fn test_object_key_join() {
        let key = ObjectKey::new("base/path");
        let joined = key.join("child/file.txt");
        assert_eq!(joined.as_str(), "base/path/child/file.txt");
    }

    #[test]
    fn test_key_builder_with_context() {
        let builder = ObjectKeyBuilder::new()
            .organization("acme")
            .project("api");

        let key = builder.artifact("run-123", "test-results.xml");
        assert_eq!(
            key.as_str(),
            "orgs/acme/projects/api/runs/run-123/artifacts/test-results.xml"
        );
    }

    #[test]
    fn test_key_builder_job_log() {
        let builder = ObjectKeyBuilder::new().organization("acme").project("web");
        let key = builder.job_log("run-456", "build");
        assert_eq!(
            key.as_str(),
            "orgs/acme/projects/web/runs/run-456/logs/build.log"
        );
    }

    #[test]
    fn test_convenience_keys() {
        let key = keys::artifact("run-123", "coverage.xml");
        assert_eq!(key.as_str(), "runs/run-123/artifacts/coverage.xml");

        let key = keys::step_log("run-123", "build", "compile");
        assert_eq!(key.as_str(), "runs/run-123/logs/build/compile.log");
    }
}
