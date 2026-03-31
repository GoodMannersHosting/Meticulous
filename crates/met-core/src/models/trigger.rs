//! Pipeline trigger models.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::ids::{PipelineId, TriggerId};

/// A trigger that can initiate pipeline runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct Trigger {
    /// Unique identifier.
    pub id: TriggerId,
    /// Pipeline to trigger.
    pub pipeline_id: PipelineId,
    /// Trigger type.
    pub kind: TriggerKind,
    /// Trigger-specific configuration.
    #[cfg_attr(feature = "sqlx", sqlx(json))]
    pub config: JsonValue,
    /// Whether the trigger is active.
    pub enabled: bool,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// When the trigger was created.
    pub created_at: DateTime<Utc>,
    /// When the trigger was last updated.
    pub updated_at: DateTime<Utc>,
}

impl Trigger {
    /// Create a new webhook trigger.
    #[must_use]
    pub fn webhook(pipeline_id: PipelineId, config: WebhookConfig) -> Self {
        let now = Utc::now();
        Self {
            id: TriggerId::new(),
            pipeline_id,
            kind: TriggerKind::Webhook,
            config: serde_json::to_value(config).unwrap_or_default(),
            enabled: true,
            description: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a new schedule trigger.
    #[must_use]
    pub fn schedule(pipeline_id: PipelineId, config: ScheduleConfig) -> Self {
        let now = Utc::now();
        Self {
            id: TriggerId::new(),
            pipeline_id,
            kind: TriggerKind::Schedule,
            config: serde_json::to_value(config).unwrap_or_default(),
            enabled: true,
            description: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a new manual trigger.
    #[must_use]
    pub fn manual(pipeline_id: PipelineId) -> Self {
        let now = Utc::now();
        Self {
            id: TriggerId::new(),
            pipeline_id,
            kind: TriggerKind::Manual,
            config: JsonValue::Object(serde_json::Map::new()),
            enabled: true,
            description: None,
            created_at: now,
            updated_at: now,
        }
    }
}

/// Type of trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(
    feature = "sqlx",
    sqlx(type_name = "trigger_kind", rename_all = "snake_case")
)]
#[serde(rename_all = "snake_case")]
pub enum TriggerKind {
    /// Triggered by HTTP webhook.
    #[default]
    Webhook,
    /// Triggered manually by a user.
    Manual,
    /// Triggered by git tag push.
    TagPush,
    /// Triggered on a schedule (cron).
    Schedule,
}

/// Configuration for webhook triggers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    /// Webhook secret for signature verification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
    /// Branch filter (glob patterns).
    #[serde(default)]
    pub branches: Vec<String>,
    /// Path filter (glob patterns).
    #[serde(default)]
    pub paths: Vec<String>,
    /// Event types to respond to.
    #[serde(default)]
    pub events: Vec<String>,
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            secret: None,
            branches: vec!["main".to_string(), "master".to_string()],
            paths: Vec::new(),
            events: vec!["push".to_string()],
        }
    }
}

/// Configuration for schedule triggers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleConfig {
    /// Cron expression (e.g., "0 0 * * *" for daily at midnight).
    pub cron: String,
    /// Timezone (IANA name, e.g., "America/New_York").
    #[serde(default = "default_timezone")]
    pub timezone: String,
}

fn default_timezone() -> String {
    "UTC".to_string()
}

/// Input for creating a trigger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTrigger {
    /// Trigger type.
    pub kind: TriggerKind,
    /// Trigger-specific configuration.
    pub config: JsonValue,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}
