//! Custom Resource Definitions for the agent operator.

use k8s_openapi::api::core::v1::PodTemplateSpec;
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// AgentPool custom resource for managing pools of build agents.
#[derive(CustomResource, Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "meticulous.dev",
    version = "v1alpha1",
    kind = "AgentPool",
    namespaced,
    status = "AgentPoolStatus",
    shortname = "ap",
    printcolumn = r#"{"name":"Ready","type":"integer","jsonPath":".status.ready"}"#,
    printcolumn = r#"{"name":"Busy","type":"integer","jsonPath":".status.busy"}"#,
    printcolumn = r#"{"name":"Age","type":"date","jsonPath":".metadata.creationTimestamp"}"#
)]
pub struct AgentPoolSpec {
    /// Replica configuration.
    pub replicas: ReplicaConfig,

    /// Selector for agent capabilities.
    #[serde(default)]
    pub selector: AgentSelector,

    /// Pool tags applied to agents in this pool.
    #[serde(default)]
    pub pool_tags: Vec<String>,

    /// Pod template for agent pods.
    #[schemars(schema_with = "pod_template_schema")]
    pub template: PodTemplateSpec,

    /// Controller URL.
    pub controller_url: String,

    /// Reference to the join token secret.
    pub join_token_secret_ref: SecretRef,

    /// Whether to use Docker-in-Docker sidecar.
    #[serde(default)]
    pub dind_enabled: bool,
}

fn pod_template_schema(_: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
    schemars::schema::Schema::Object(schemars::schema::SchemaObject {
        instance_type: Some(schemars::schema::InstanceType::Object.into()),
        ..Default::default()
    })
}

/// Replica configuration for the agent pool.
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct ReplicaConfig {
    /// Minimum number of agents.
    #[serde(default)]
    pub min: i32,

    /// Maximum number of agents.
    #[serde(default)]
    pub max: Option<i32>,

    /// Number of idle agents to keep warm.
    #[serde(default = "default_idle")]
    pub idle: i32,
}

fn default_idle() -> i32 {
    1
}

/// Selector for agent capabilities.
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct AgentSelector {
    /// Operating system.
    #[serde(default = "default_os")]
    pub os: String,

    /// CPU architecture.
    #[serde(default = "default_arch")]
    pub arch: String,

    /// Required labels.
    #[serde(default)]
    pub labels: Vec<String>,
}

fn default_os() -> String {
    "linux".to_string()
}

fn default_arch() -> String {
    "amd64".to_string()
}

/// Reference to a Kubernetes secret.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct SecretRef {
    /// Secret name.
    pub name: String,

    /// Key in the secret.
    #[serde(default = "default_key")]
    pub key: String,
}

fn default_key() -> String {
    "token".to_string()
}

/// Status of an AgentPool.
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct AgentPoolStatus {
    /// Number of ready agents.
    #[serde(default)]
    pub ready: i32,

    /// Number of busy agents.
    #[serde(default)]
    pub busy: i32,

    /// Number of idle agents.
    #[serde(default)]
    pub idle: i32,

    /// Total jobs completed by this pool.
    #[serde(default)]
    pub total_jobs_completed: i64,

    /// Last scale event timestamp.
    pub last_scale_time: Option<String>,

    /// Current conditions.
    #[serde(default)]
    pub conditions: Vec<PoolCondition>,

    /// Observed generation.
    #[serde(default)]
    pub observed_generation: i64,
}

/// Condition of an agent pool.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct PoolCondition {
    /// Condition type.
    pub r#type: String,

    /// Status (True, False, Unknown).
    pub status: String,

    /// Reason for the condition.
    #[serde(default)]
    pub reason: String,

    /// Human-readable message.
    #[serde(default)]
    pub message: String,

    /// Last transition time.
    pub last_transition_time: String,
}

/// AgentPoolAutoscaler custom resource for auto-scaling agent pools.
#[derive(CustomResource, Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "meticulous.dev",
    version = "v1alpha1",
    kind = "AgentPoolAutoscaler",
    namespaced,
    shortname = "apa"
)]
pub struct AgentPoolAutoscalerSpec {
    /// Reference to the AgentPool to scale.
    pub pool_ref: PoolRef,

    /// Scaling metrics.
    pub metrics: Vec<ScalingMetric>,

    /// Scaling behavior.
    #[serde(default)]
    pub behavior: ScalingBehavior,
}

/// Reference to an AgentPool.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct PoolRef {
    /// Pool name.
    pub name: String,
}

/// A metric used for scaling decisions.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct ScalingMetric {
    /// Metric type.
    pub r#type: MetricType,

    /// Target configuration.
    pub target: MetricTarget,
}

/// Type of scaling metric.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub enum MetricType {
    /// Queue depth (pending jobs).
    QueueDepth,
    /// Number of idle agents.
    IdleAgents,
    /// CPU utilization.
    Cpu,
    /// Memory utilization.
    Memory,
}

/// Target for a scaling metric.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct MetricTarget {
    /// NATS subject to check (for QueueDepth).
    #[serde(default)]
    pub subject: String,

    /// Threshold value.
    #[serde(default)]
    pub threshold: i32,

    /// Minimum value (for IdleAgents).
    #[serde(default)]
    pub min: Option<i32>,
}

/// Scaling behavior configuration.
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct ScalingBehavior {
    /// Scale up behavior.
    #[serde(default)]
    pub scale_up: ScalingPolicy,

    /// Scale down behavior.
    #[serde(default)]
    pub scale_down: ScalingPolicy,
}

/// Scaling policy for scale up or scale down.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct ScalingPolicy {
    /// Stabilization window in seconds.
    #[serde(default = "default_stabilization")]
    pub stabilization_window_seconds: i32,

    /// Policies.
    #[serde(default)]
    pub policies: Vec<ScalingPolicyRule>,
}

impl Default for ScalingPolicy {
    fn default() -> Self {
        Self {
            stabilization_window_seconds: 60,
            policies: Vec::new(),
        }
    }
}

fn default_stabilization() -> i32 {
    60
}

/// A scaling policy rule.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct ScalingPolicyRule {
    /// Policy type.
    pub r#type: PolicyType,

    /// Value.
    pub value: i32,

    /// Period in seconds.
    pub period_seconds: i32,
}

/// Type of scaling policy.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub enum PolicyType {
    /// Scale by a number of pods.
    Pods,
    /// Scale by a percentage.
    Percent,
}
