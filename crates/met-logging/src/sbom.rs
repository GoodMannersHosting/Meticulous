//! Software Bill of Materials (SBOM) support.
//!
//! Provides SBOM parsing, generation, and diffing capabilities.

use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use met_core::ids::{ProjectId, RunId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use thiserror::Error;
use uuid::Uuid;

/// SBOM format types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SbomFormat {
    /// SPDX format.
    Spdx,
    /// CycloneDX format.
    CycloneDx,
    /// Internal normalized format.
    Internal,
}

/// Errors from SBOM operations.
#[derive(Debug, Error)]
pub enum SbomError {
    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Invalid format: {0}")]
    InvalidFormat(String),

    #[error("Component not found: {0}")]
    ComponentNotFound(String),
}

pub type Result<T> = std::result::Result<T, SbomError>;

/// A component in the SBOM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SbomComponent {
    /// Component name (package name).
    pub name: String,
    /// Component version.
    pub version: String,
    /// Package URL (purl).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purl: Option<String>,
    /// Component type (library, application, etc.).
    pub component_type: String,
    /// License identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// SHA256 hash of the component.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// Direct dependencies (names).
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// Additional properties.
    #[serde(default)]
    pub properties: HashMap<String, String>,
}

impl SbomComponent {
    /// Create a new component.
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            purl: None,
            component_type: "library".to_string(),
            license: None,
            sha256: None,
            dependencies: Vec::new(),
            properties: HashMap::new(),
        }
    }

    /// Generate a unique key for this component.
    pub fn key(&self) -> String {
        format!("{}@{}", self.name, self.version)
    }

    /// Set the package URL.
    pub fn with_purl(mut self, purl: impl Into<String>) -> Self {
        self.purl = Some(purl.into());
        self
    }

    /// Set the license.
    pub fn with_license(mut self, license: impl Into<String>) -> Self {
        self.license = Some(license.into());
        self
    }

    /// Set the SHA256 hash.
    pub fn with_sha256(mut self, sha256: impl Into<String>) -> Self {
        self.sha256 = Some(sha256.into());
        self
    }
}

/// A Software Bill of Materials.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sbom {
    /// Unique SBOM ID.
    pub id: Uuid,
    /// SBOM format.
    pub format: SbomFormat,
    /// When the SBOM was generated.
    pub created_at: DateTime<Utc>,
    /// Associated project ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
    /// Associated run ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
    /// Name of the subject (what was scanned).
    pub subject: String,
    /// Subject version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_version: Option<String>,
    /// All components.
    pub components: IndexMap<String, SbomComponent>,
    /// Tool that generated the SBOM.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generator_tool: Option<String>,
    /// Generator tool version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generator_version: Option<String>,
}

impl Sbom {
    /// Create a new SBOM.
    pub fn new(subject: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            format: SbomFormat::Internal,
            created_at: Utc::now(),
            project_id: None,
            run_id: None,
            subject: subject.into(),
            subject_version: None,
            components: IndexMap::new(),
            generator_tool: Some("meticulous".to_string()),
            generator_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        }
    }

    /// Add a component.
    pub fn add_component(&mut self, component: SbomComponent) {
        let key = component.key();
        self.components.insert(key, component);
    }

    /// Get a component by name and version.
    pub fn get_component(&self, name: &str, version: &str) -> Option<&SbomComponent> {
        let key = format!("{}@{}", name, version);
        self.components.get(&key)
    }

    /// Get all components with a given name.
    pub fn get_components_by_name(&self, name: &str) -> Vec<&SbomComponent> {
        self.components
            .values()
            .filter(|c| c.name == name)
            .collect()
    }

    /// Total component count.
    pub fn component_count(&self) -> usize {
        self.components.len()
    }

    /// Compute SHA256 hash of the SBOM content.
    pub fn content_hash(&self) -> String {
        let mut hasher = Sha256::new();

        // Hash components in sorted order for determinism
        let mut keys: Vec<_> = self.components.keys().collect();
        keys.sort();

        for key in keys {
            if let Some(component) = self.components.get(key) {
                hasher.update(component.name.as_bytes());
                hasher.update(component.version.as_bytes());
                if let Some(ref sha) = component.sha256 {
                    hasher.update(sha.as_bytes());
                }
            }
        }

        format!("{:x}", hasher.finalize())
    }

    /// Search components by name pattern.
    pub fn search(&self, pattern: &str) -> Vec<&SbomComponent> {
        let pattern_lower = pattern.to_lowercase();
        self.components
            .values()
            .filter(|c| c.name.to_lowercase().contains(&pattern_lower))
            .collect()
    }

    /// Set the run context.
    pub fn with_run(mut self, project_id: ProjectId, run_id: RunId) -> Self {
        self.project_id = Some(project_id);
        self.run_id = Some(run_id);
        self
    }

    /// Set the original format.
    pub fn with_format(mut self, format: SbomFormat) -> Self {
        self.format = format;
        self
    }
}

/// Kind of change in a diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffKind {
    /// Component was added.
    Added,
    /// Component was removed.
    Removed,
    /// Component version changed.
    Changed,
}

/// A single diff entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffEntry {
    /// Kind of change.
    pub kind: DiffKind,
    /// Component name.
    pub name: String,
    /// Old version (if changed or removed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_version: Option<String>,
    /// New version (if changed or added).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_version: Option<String>,
    /// Old component (full details).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_component: Option<SbomComponent>,
    /// New component (full details).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_component: Option<SbomComponent>,
}

/// Result of diffing two SBOMs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SbomDiff {
    /// Left SBOM ID.
    pub left_id: Uuid,
    /// Right SBOM ID.
    pub right_id: Uuid,
    /// When the diff was computed.
    pub computed_at: DateTime<Utc>,
    /// All changes.
    pub changes: Vec<DiffEntry>,
    /// Summary statistics.
    pub summary: DiffSummary,
}

/// Summary of diff changes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiffSummary {
    /// Number of added components.
    pub added: usize,
    /// Number of removed components.
    pub removed: usize,
    /// Number of changed components.
    pub changed: usize,
    /// Number of unchanged components.
    pub unchanged: usize,
}

impl SbomDiff {
    /// Compute the diff between two SBOMs.
    pub fn compute(left: &Sbom, right: &Sbom) -> Self {
        let mut changes = Vec::new();
        let mut summary = DiffSummary::default();

        // Build maps by component name
        let left_by_name: HashMap<&str, Vec<&SbomComponent>> =
            left.components.values().fold(HashMap::new(), |mut acc, c| {
                acc.entry(c.name.as_str()).or_default().push(c);
                acc
            });

        let right_by_name: HashMap<&str, Vec<&SbomComponent>> =
            right
                .components
                .values()
                .fold(HashMap::new(), |mut acc, c| {
                    acc.entry(c.name.as_str()).or_default().push(c);
                    acc
                });

        let all_names: HashSet<&str> = left_by_name
            .keys()
            .chain(right_by_name.keys())
            .copied()
            .collect();

        for name in all_names {
            let left_components = left_by_name.get(name);
            let right_components = right_by_name.get(name);

            match (left_components, right_components) {
                (None, Some(components)) => {
                    // Added
                    for component in components {
                        changes.push(DiffEntry {
                            kind: DiffKind::Added,
                            name: name.to_string(),
                            old_version: None,
                            new_version: Some(component.version.clone()),
                            old_component: None,
                            new_component: Some((*component).clone()),
                        });
                        summary.added += 1;
                    }
                }
                (Some(components), None) => {
                    // Removed
                    for component in components {
                        changes.push(DiffEntry {
                            kind: DiffKind::Removed,
                            name: name.to_string(),
                            old_version: Some(component.version.clone()),
                            new_version: None,
                            old_component: Some((*component).clone()),
                            new_component: None,
                        });
                        summary.removed += 1;
                    }
                }
                (Some(left_comps), Some(right_comps)) => {
                    // Compare versions
                    let left_versions: HashSet<&str> =
                        left_comps.iter().map(|c| c.version.as_str()).collect();
                    let right_versions: HashSet<&str> =
                        right_comps.iter().map(|c| c.version.as_str()).collect();

                    if left_versions == right_versions {
                        summary.unchanged += left_versions.len();
                    } else {
                        // Version changed
                        for old in left_comps {
                            if !right_versions.contains(old.version.as_str()) {
                                // This version was removed or replaced
                                if let Some(new) = right_comps.first() {
                                    changes.push(DiffEntry {
                                        kind: DiffKind::Changed,
                                        name: name.to_string(),
                                        old_version: Some(old.version.clone()),
                                        new_version: Some(new.version.clone()),
                                        old_component: Some((*old).clone()),
                                        new_component: Some((*new).clone()),
                                    });
                                    summary.changed += 1;
                                }
                            }
                        }
                    }
                }
                (None, None) => unreachable!(),
            }
        }

        // Sort changes for deterministic output
        changes.sort_by(|a, b| a.name.cmp(&b.name));

        Self {
            left_id: left.id,
            right_id: right.id,
            computed_at: Utc::now(),
            changes,
            summary,
        }
    }

    /// Get only added components.
    pub fn added(&self) -> Vec<&DiffEntry> {
        self.changes
            .iter()
            .filter(|e| e.kind == DiffKind::Added)
            .collect()
    }

    /// Get only removed components.
    pub fn removed(&self) -> Vec<&DiffEntry> {
        self.changes
            .iter()
            .filter(|e| e.kind == DiffKind::Removed)
            .collect()
    }

    /// Get only changed components.
    pub fn changed(&self) -> Vec<&DiffEntry> {
        self.changes
            .iter()
            .filter(|e| e.kind == DiffKind::Changed)
            .collect()
    }

    /// Check if there are any changes.
    pub fn has_changes(&self) -> bool {
        !self.changes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sbom_creation() {
        let mut sbom = Sbom::new("my-app");
        sbom.add_component(SbomComponent::new("lodash", "4.17.21"));
        sbom.add_component(SbomComponent::new("express", "4.18.2"));

        assert_eq!(sbom.component_count(), 2);
        assert!(sbom.get_component("lodash", "4.17.21").is_some());
    }

    #[test]
    fn test_sbom_search() {
        let mut sbom = Sbom::new("my-app");
        sbom.add_component(SbomComponent::new("lodash", "4.17.21"));
        sbom.add_component(SbomComponent::new("lodash-es", "4.17.21"));
        sbom.add_component(SbomComponent::new("express", "4.18.2"));

        let results = sbom.search("lodash");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_diff_added() {
        let left = Sbom::new("app");
        let mut right = Sbom::new("app");
        right.add_component(SbomComponent::new("new-dep", "1.0.0"));

        let diff = SbomDiff::compute(&left, &right);
        assert_eq!(diff.summary.added, 1);
        assert_eq!(diff.summary.removed, 0);
    }

    #[test]
    fn test_diff_removed() {
        let mut left = Sbom::new("app");
        left.add_component(SbomComponent::new("old-dep", "1.0.0"));
        let right = Sbom::new("app");

        let diff = SbomDiff::compute(&left, &right);
        assert_eq!(diff.summary.added, 0);
        assert_eq!(diff.summary.removed, 1);
    }

    #[test]
    fn test_diff_changed() {
        let mut left = Sbom::new("app");
        left.add_component(SbomComponent::new("dep", "1.0.0"));

        let mut right = Sbom::new("app");
        right.add_component(SbomComponent::new("dep", "2.0.0"));

        let diff = SbomDiff::compute(&left, &right);
        assert_eq!(diff.summary.changed, 1);
    }

    #[test]
    fn test_content_hash() {
        let mut sbom1 = Sbom::new("app");
        sbom1.add_component(SbomComponent::new("dep", "1.0.0"));

        let mut sbom2 = Sbom::new("app");
        sbom2.add_component(SbomComponent::new("dep", "1.0.0"));

        // Same content should have same hash
        assert_eq!(sbom1.content_hash(), sbom2.content_hash());

        // Different content should have different hash
        sbom2.add_component(SbomComponent::new("other", "2.0.0"));
        assert_ne!(sbom1.content_hash(), sbom2.content_hash());
    }
}
