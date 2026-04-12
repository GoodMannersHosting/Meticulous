//! YAML schema types for pipeline definitions.
//!
//! These types represent the raw YAML structure before validation and resolution.
//! They use permissive deserialization to capture as much information as possible
//! for error reporting.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Raw pipeline definition from YAML.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RawPipeline {
    /// Pipeline name.
    pub name: String,

    /// Trigger configurations.
    #[serde(default)]
    pub triggers: RawTriggers,

    /// Agent pool selector.
    #[serde(default)]
    pub runs_on: Option<RawPoolSelector>,

    /// Secret references.
    #[serde(default)]
    pub secrets: IndexMap<String, RawSecretRef>,

    /// Plain-text variables.
    #[serde(default)]
    pub vars: IndexMap<String, String>,

    /// Workflow invocations.
    #[serde(default)]
    pub workflows: Vec<RawWorkflowInvocation>,

    /// Optional same-agent affinity and shared workspace policy (pipeline-level).
    #[serde(default)]
    pub agent_affinity: Option<RawAgentAffinity>,

    /// Allow secret workflow outputs to flow into dependent job environment as plaintext (opt-in).
    #[serde(default)]
    pub expose_workflow_secret_outputs: bool,
}

/// Pipeline-level agent affinity defaults (optional `affinity-group` on invocations still applies).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RawAgentAffinity {
    /// When a workflow omits `affinity-group`, pin to this group for same-agent scheduling only.
    /// Does **not** enable a shared workspace; set `affinity-group` on each invocation that should
    /// share a workspace when [`Self::share_workspace`] is true.
    #[serde(default)]
    pub default_group: Option<String>,
    /// When true, invocations that **explicitly** set `affinity-group` share one workspace directory
    /// for the run (serial-only within that group). Jobs using only `default-group` are not shared.
    #[serde(default)]
    pub share_workspace: bool,
}

/// Trigger configurations.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RawTriggers {
    /// Manual trigger (always available if present).
    #[serde(default)]
    pub manual: Option<RawManualTrigger>,

    /// Webhook trigger for SCM events.
    #[serde(default)]
    pub webhook: Option<RawWebhookTrigger>,

    /// Tag/release trigger.
    #[serde(default)]
    pub tag: Option<RawTagTrigger>,

    /// Scheduled trigger.
    #[serde(default)]
    pub schedule: Option<RawScheduleTrigger>,

    /// Release trigger (alias for tag in some contexts).
    #[serde(default)]
    pub release: Option<RawReleaseTrigger>,
}

/// Manual trigger configuration.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RawManualTrigger {
    /// Optional description.
    #[serde(default)]
    pub description: Option<String>,
}

/// Webhook trigger configuration.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RawWebhookTrigger {
    /// Events to trigger on.
    #[serde(default)]
    pub events: Vec<String>,

    /// Branch patterns to match.
    #[serde(default)]
    pub branches: Vec<String>,

    /// Path patterns to match (changes in these paths trigger the pipeline).
    #[serde(default)]
    pub paths: Vec<String>,

    /// Path patterns to ignore.
    #[serde(default)]
    pub paths_ignore: Vec<String>,

    /// Stable id for syncing this webhook trigger from Git into the `triggers` table.
    #[serde(default, rename = "sync-key")]
    pub sync_key: Option<String>,
}

/// Tag trigger configuration.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RawTagTrigger {
    /// Tag patterns to match.
    #[serde(default)]
    pub patterns: Vec<String>,
}

/// Release trigger configuration.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RawReleaseTrigger {
    /// Tag patterns to match.
    #[serde(default)]
    pub tag: Vec<String>,
}

/// Schedule trigger configuration.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RawScheduleTrigger {
    /// Cron expression (UTC).
    pub cron: String,

    /// Optional timezone override.
    #[serde(default)]
    pub timezone: Option<String>,
}

/// Agent pool selector.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RawPoolSelector {
    /// Tag requirements.
    #[serde(default)]
    pub tags: Vec<IndexMap<String, serde_yaml::Value>>,

    /// Pool name (alternative to tags).
    #[serde(default)]
    pub pool: Option<String>,
}

/// Secret reference - exactly one of the provider fields should be set.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RawSecretRef {
    /// AWS Secrets Manager reference.
    #[serde(default)]
    pub aws: Option<RawAwsSecretRef>,
    /// HashiCorp Vault reference.
    #[serde(default)]
    pub vault: Option<RawVaultSecretRef>,
    /// Built-in secret store (discouraged).
    #[serde(default)]
    pub builtin: Option<RawBuiltinSecretRef>,
    /// Platform-stored secret (preferred over `builtin`).
    #[serde(default)]
    pub stored: Option<RawStoredSecretRef>,
    /// GCP Secret Manager reference (ADR-020).
    #[serde(default)]
    pub gcp: Option<RawGcpSecretRef>,
    /// Azure Key Vault reference (ADR-020).
    #[serde(default)]
    pub azure: Option<RawAzureSecretRef>,
    /// Kubernetes secret reference (ADR-020).
    #[serde(default)]
    pub kubernetes: Option<RawKubernetesSecretRef>,
    /// Resolution mode override: `"local"` or `"remote"` (ADR-020).
    #[serde(default)]
    pub resolution: Option<String>,
}

/// Platform-stored secret reference.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RawStoredSecretRef {
    /// Logical name in the platform secret store (row `path`).
    pub name: String,
}

/// AWS Secrets Manager secret reference.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RawAwsSecretRef {
    /// ARN of the secret.
    pub arn: String,
    /// Optional key within the secret (for JSON secrets).
    #[serde(default)]
    pub key: Option<String>,
}

/// HashiCorp Vault secret reference.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RawVaultSecretRef {
    /// Vault path.
    pub path: String,
    /// Key within the secret.
    pub key: String,
    /// Optional Vault mount.
    #[serde(default)]
    pub mount: Option<String>,
}

/// Built-in secret store reference (discouraged).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RawBuiltinSecretRef {
    /// Secret name.
    pub name: String,
}

/// GCP Secret Manager reference (ADR-020).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RawGcpSecretRef {
    /// Secret name in GCP.
    pub name: String,
    /// GCP project ID (optional; uses provider config default).
    #[serde(default)]
    pub project: Option<String>,
    /// Secret version (optional; defaults to `"latest"`).
    #[serde(default)]
    pub version: Option<String>,
}

/// Azure Key Vault reference (ADR-020).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RawAzureSecretRef {
    /// Secret name in the vault.
    pub name: String,
    /// Vault URL (optional; uses provider config default).
    #[serde(default)]
    pub vault_url: Option<String>,
    /// Secret version (optional; defaults to latest).
    #[serde(default)]
    pub version: Option<String>,
}

/// Kubernetes secret reference (ADR-020).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RawKubernetesSecretRef {
    /// Secret name.
    pub name: String,
    /// Key within the secret data map.
    pub key: String,
    /// Namespace (optional; defaults to provider config).
    #[serde(default)]
    pub namespace: Option<String>,
}

/// Workflow invocation in a pipeline.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RawWorkflowInvocation {
    /// Display name.
    pub name: String,

    /// Unique identifier within the pipeline.
    pub id: String,

    /// Workflow reference ("global/<name>" or "project/<name>").
    pub workflow: String,

    /// Version (semver or tag).
    #[serde(default)]
    pub version: Option<String>,

    /// Input values.
    #[serde(default)]
    pub inputs: IndexMap<String, serde_yaml::Value>,

    /// Dependencies (IDs of workflows that must complete first).
    #[serde(default)]
    pub depends_on: Vec<String>,

    /// Conditional execution (CEL expression).
    #[serde(default)]
    pub condition: Option<String>,

    /// Timeout.
    #[serde(default, with = "humantime_serde_opt")]
    pub timeout: Option<Duration>,

    /// Retry policy.
    #[serde(default)]
    pub retry: Option<RawRetryPolicy>,

    /// Cache configuration.
    #[serde(default)]
    pub cache: Option<RawCacheConfig>,

    /// Same-agent affinity group (overrides pipeline `agent-affinity.default-group` when set).
    /// When `agent-affinity.share-workspace` is true, this invocation opts into the shared workspace.
    #[serde(default)]
    pub affinity_group: Option<String>,

    /// Target deployment environment (ADR-016).
    #[serde(default)]
    pub environment: Option<String>,

    /// Workspace snapshot transfer (ADR-014).
    #[serde(default)]
    pub workspace: Option<RawWorkspaceTransfer>,
}

/// Workspace snapshot transfer between workflow invocations (ADR-014).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RawWorkspaceTransfer {
    /// Invocation ID to restore workspace from.
    pub from: String,
    /// Paths to archive after this invocation completes.
    #[serde(default)]
    pub outputs: Vec<String>,
}

/// Retry policy configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RawRetryPolicy {
    /// Maximum number of attempts.
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,

    /// Backoff duration between retries.
    #[serde(default, with = "humantime_serde_opt")]
    pub backoff: Option<Duration>,
}

fn default_max_attempts() -> u32 {
    3
}

/// Cache configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RawCacheConfig {
    /// Cache key template.
    pub key: String,

    /// Paths to cache.
    #[serde(default)]
    pub paths: Vec<String>,

    /// Fallback keys for partial matches.
    #[serde(default)]
    pub restore_keys: Vec<String>,
}

/// Reusable workflow definition.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RawWorkflowDef {
    /// Workflow name.
    pub name: String,

    /// Description.
    #[serde(default)]
    pub description: Option<String>,

    /// Version (semver).
    #[serde(default)]
    pub version: Option<String>,

    /// Input declarations.
    #[serde(default)]
    pub inputs: IndexMap<String, RawInputDef>,

    /// Output declarations.
    #[serde(default)]
    pub outputs: IndexMap<String, RawOutputDef>,

    /// Jobs in the workflow.
    #[serde(default)]
    pub jobs: Vec<RawJob>,
}

/// Input parameter definition.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub struct RawInputDef {
    /// Input type.
    #[serde(rename = "type", default = "default_input_type")]
    pub input_type: String,

    /// Whether the input is required.
    #[serde(default)]
    pub required: bool,

    /// Default value.
    #[serde(default)]
    pub default: Option<serde_yaml::Value>,

    /// Description.
    #[serde(default)]
    pub description: Option<String>,
}

fn default_input_type() -> String {
    "string".to_string()
}

/// Output definition.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RawOutputDef {
    /// Output value expression (optional when outputs are only produced via `met-output`).
    #[serde(default)]
    pub value: Option<String>,

    /// Description.
    #[serde(default)]
    pub description: Option<String>,

    /// When true, this output is sensitive; plaintext env interpolation requires `expose-workflow-secret-outputs` on the pipeline.
    #[serde(default)]
    pub secret: bool,
}

/// OCI container environment for a job (ADR-015).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RawEnvironment {
    /// Fully-qualified image reference (should include `@sha256:` digest).
    pub image: String,
    /// Signature verification method: `"cosign"` or `"none"` (default).
    #[serde(default)]
    pub verify: Option<String>,
    /// Registry credential reference (must be a `stored:` secret).
    #[serde(default)]
    pub credentials: Option<RawStoredSecretRef>,
    /// Pull policy: `"always"`, `"if-not-present"`, or `"never"`.
    #[serde(default)]
    pub pull_policy: Option<String>,
}

/// Job definition within a workflow.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RawJob {
    /// Job name.
    pub name: String,

    /// Unique identifier within the workflow.
    pub id: String,

    /// Pool selector override.
    #[serde(default)]
    pub runs_on: Option<RawPoolSelector>,

    /// OCI container environment (ADR-015).
    #[serde(default)]
    pub environment: Option<RawEnvironment>,

    /// Steps to execute.
    #[serde(default)]
    pub steps: Vec<RawStep>,

    /// Sidecar services.
    #[serde(default)]
    pub services: Vec<RawService>,

    /// Dependencies within the workflow.
    #[serde(default)]
    pub depends_on: Vec<String>,

    /// Conditional execution.
    #[serde(default)]
    pub condition: Option<String>,

    /// Timeout.
    #[serde(default, with = "humantime_serde_opt")]
    pub timeout: Option<Duration>,

    /// Retry policy.
    #[serde(default)]
    pub retry: Option<RawRetryPolicy>,
}

/// Step definition.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RawStep {
    /// Step name.
    pub name: String,

    /// Unique identifier within the job.
    #[serde(default)]
    pub id: Option<String>,

    /// Shell command to run.
    #[serde(default)]
    pub run: Option<String>,

    /// Shell type.
    #[serde(default)]
    pub shell: Option<String>,

    /// Action to use (alternative to run).
    #[serde(default)]
    pub uses: Option<String>,

    /// Action inputs.
    #[serde(default, rename = "with")]
    pub action_inputs: IndexMap<String, serde_yaml::Value>,

    /// Environment variables.
    #[serde(default)]
    pub env: IndexMap<String, String>,

    /// Working directory.
    #[serde(default)]
    pub working_directory: Option<String>,

    /// Timeout.
    #[serde(default, with = "humantime_serde_opt")]
    pub timeout: Option<Duration>,

    /// Continue on error.
    #[serde(default)]
    pub continue_on_error: bool,

    /// Optional declared outputs from this step (names only; values come from `met-output` at runtime).
    #[serde(default)]
    pub outputs: IndexMap<String, RawStepOutputDef>,
}

/// Declared step output (documentation / validation).
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct RawStepOutputDef {
    #[serde(default)]
    pub description: Option<String>,

    #[serde(default)]
    pub secret: bool,
}

/// Service (sidecar container) definition.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RawService {
    /// Service name.
    pub name: String,

    /// Container image.
    pub image: String,

    /// Exposed ports.
    #[serde(default)]
    pub ports: Vec<u16>,

    /// Environment variables.
    #[serde(default)]
    pub env: IndexMap<String, String>,

    /// Command override.
    #[serde(default)]
    pub command: Option<Vec<String>>,

    /// Health check configuration.
    #[serde(default)]
    pub health_check: Option<RawHealthCheck>,
}

/// Health check configuration for services.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RawHealthCheck {
    /// Command to run for health check.
    #[serde(default)]
    pub cmd: Option<Vec<String>>,

    /// HTTP endpoint to check.
    #[serde(default)]
    pub http: Option<String>,

    /// TCP port to check.
    #[serde(default)]
    pub tcp: Option<u16>,

    /// Interval between checks.
    #[serde(default, with = "humantime_serde_opt")]
    pub interval: Option<Duration>,

    /// Timeout for each check.
    #[serde(default, with = "humantime_serde_opt")]
    pub timeout: Option<Duration>,

    /// Number of retries before considering unhealthy.
    #[serde(default)]
    pub retries: Option<u32>,
}

mod humantime_serde_opt {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(value: &Option<Duration>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(d) => {
                let s = humantime::format_duration(*d).to_string();
                serializer.serialize_some(&s)
            }
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt: Option<String> = Option::deserialize(deserializer)?;
        match opt {
            Some(s) => humantime::parse_duration(&s)
                .map(Some)
                .map_err(serde::de::Error::custom),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_pipeline() {
        let yaml = r#"
name: test pipeline
triggers:
  manual: {}
vars:
  FOO: bar
workflows:
  - name: Build
    id: build
    workflow: global/docker-build
    version: v1.0
    inputs:
      image: test
"#;

        let pipeline: RawPipeline = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(pipeline.name, "test pipeline");
        assert!(pipeline.triggers.manual.is_some());
        assert_eq!(pipeline.vars.get("FOO"), Some(&"bar".to_string()));
        assert_eq!(pipeline.workflows.len(), 1);
        assert_eq!(pipeline.workflows[0].id, "build");
    }

    #[test]
    fn test_parse_workflow_def() {
        let yaml = r#"
name: Docker Build
description: Build a Docker image
version: "1.0.0"
inputs:
  image:
    type: string
    required: true
  tag:
    type: string
    default: latest
outputs:
  image_sha:
    value: ${{ steps.build.outputs.digest }}
jobs:
  - name: Build Image
    id: build
    steps:
      - name: Build
        id: build
        run: docker build -t ${{ inputs.image }}:${{ inputs.tag }} .
"#;

        let workflow: RawWorkflowDef = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(workflow.name, "Docker Build");
        assert_eq!(workflow.inputs.len(), 2);
        assert!(workflow.inputs.get("image").unwrap().required);
        assert_eq!(workflow.jobs.len(), 1);
    }

    #[test]
    fn test_parse_secret_refs() {
        let yaml = r#"
name: secrets test
triggers: {}
secrets:
  AWS_SECRET:
    aws:
      arn: arn:aws:secretsmanager:us-east-1:123:secret:test
  VAULT_SECRET:
    vault:
      path: secret/data/myapp
      key: password
workflows: []
"#;

        let pipeline: RawPipeline = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(pipeline.secrets.len(), 2);
        assert!(pipeline.secrets.get("AWS_SECRET").unwrap().aws.is_some());
        assert!(
            pipeline
                .secrets
                .get("VAULT_SECRET")
                .unwrap()
                .vault
                .is_some()
        );
    }
}
