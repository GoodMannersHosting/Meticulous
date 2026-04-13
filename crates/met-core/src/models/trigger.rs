//! Pipeline trigger models.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::ids::{PipelineId, TriggerId, UserId};

/// Maximum raw webhook body size accepted for JSON parsing and variable mapping (defense in depth).
/// Larger than [`WEBHOOK_MAX_TOTAL_MAPPED_BYTES`] so JSON structure overhead can carry several large strings
/// while mapped variable payload stays separately capped.
pub const WEBHOOK_MAX_BODY_BYTES: usize = 512 * 1024;

/// Maximum byte length for a single mapped variable value (UTF-8).
pub const WEBHOOK_MAX_VALUE_BYTES: usize = 64 * 1024;

/// Maximum total UTF-8 bytes across all mapped variable values (excluding keys).
pub const WEBHOOK_MAX_TOTAL_MAPPED_BYTES: usize = 256 * 1024;

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
    /// User who created this trigger (API or UI); unset for repo-synced rows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by_user_id: Option<UserId>,
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
            created_by_user_id: None,
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
            created_by_user_id: None,
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
            created_by_user_id: None,
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
    /// Webhook secret for signature verification (`hmac` / `query`) or omitted when `inbound_auth` is `none`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
    /// Inbound verification: `none`, `hmac` (`X-Hub-Signature-256`), or `query` (secret must match query param value).
    /// When omitted: legacy behavior — `hmac` if `secret` is non-empty, otherwise `none`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inbound_auth: Option<String>,
    /// When `inbound_auth` is `query`: query parameter name whose value must equal `secret`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inbound_query_param: Option<String>,
    /// Branch filter (glob patterns).
    #[serde(default)]
    pub branches: Vec<String>,
    /// Path filter (glob patterns).
    #[serde(default)]
    pub paths: Vec<String>,
    /// Path patterns to ignore (declarative / future filter enforcement).
    #[serde(default)]
    pub paths_ignore: Vec<String>,
    /// Event types to respond to.
    #[serde(default)]
    pub events: Vec<String>,
    /// When true, each top-level key of a JSON object root is mapped to a variable.
    #[serde(default = "default_flatten_top_level")]
    pub flatten_top_level: bool,
    /// If set, the entire raw request body is exposed as a single variable with this name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_raw_body_variable: Option<String>,
    /// Stable key for reconciling this row from Git (`managed_by: "repo"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync_key: Option<String>,
    /// Set to `"repo"` when this trigger is owned by pipeline YAML sync.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub managed_by: Option<String>,
}

fn default_flatten_top_level() -> bool {
    true
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            secret: None,
            inbound_auth: None,
            inbound_query_param: None,
            branches: vec!["main".to_string(), "master".to_string()],
            paths: Vec::new(),
            paths_ignore: Vec::new(),
            events: vec!["push".to_string()],
            flatten_top_level: true,
            include_raw_body_variable: None,
            sync_key: None,
            managed_by: None,
        }
    }
}

/// Error building webhook variable map from JSON payload.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WebhookVariableMapError {
    /// Request body exceeds [`WebhookConfig`] limits.
    #[error("webhook body too large (max {max} bytes, got {got})")]
    BodyTooLarge { max: usize, got: usize },
    /// Total mapped variable payload exceeds limit.
    #[error("mapped webhook variables exceed total size limit (max {max} bytes)")]
    TotalMappedTooLarge { max: usize },
    /// A single mapped value exceeds limit.
    #[error("mapped value for key {key:?} exceeds max length ({max} bytes)")]
    ValueTooLarge {
        /// Variable name.
        key: String,
        /// Configured max.
        max: usize,
    },
    /// JSON parse failure.
    #[error("invalid JSON: {0}")]
    InvalidJson(String),
}

impl WebhookConfig {
    /// Effective inbound mode: `none`, `hmac`, or `query`.
    #[must_use]
    pub fn resolved_inbound_auth(&self) -> String {
        if let Some(raw) = self.inbound_auth.as_deref() {
            match raw.trim().to_lowercase().as_str() {
                "none" => return "none".into(),
                "query" => return "query".into(),
                "hmac" => return "hmac".into(),
                _ => return "hmac".into(),
            }
        }
        if self.secret.as_deref().is_none_or(|s| s.is_empty()) {
            "none".into()
        } else {
            "hmac".into()
        }
    }

    /// Whether `inbound_query_param` is a valid identifier (letter first; letters, digits, `-`, `_`).
    #[must_use]
    pub fn inbound_query_param_name_valid(name: &str) -> bool {
        let mut chars = name.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        if !first.is_alphabetic() {
            return false;
        }
        name.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    }

    /// Validate `inbound_auth` / `secret` / `inbound_query_param` for a persisted pipeline webhook trigger.
    pub fn validate_inbound_for_trigger(&self) -> Result<(), String> {
        match self.resolved_inbound_auth().as_str() {
            "none" => Ok(()),
            "hmac" => {
                if self.secret.as_deref().is_none_or(|s| s.is_empty()) {
                    Err(r#"inbound_auth "hmac" requires a non-empty secret"#.into())
                } else {
                    Ok(())
                }
            }
            "query" => {
                let name = self
                    .inbound_query_param
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                let Some(n) = name else {
                    return Err(
                        r#"inbound_auth "query" requires inbound_query_param (e.g. "token")"#
                            .into(),
                    );
                };
                if !Self::inbound_query_param_name_valid(n) {
                    return Err(
                        "inbound_query_param must start with a letter and use only letters, digits, hyphen, or underscore"
                            .into(),
                    );
                }
                if self.secret.as_deref().is_none_or(|s| s.is_empty()) {
                    return Err(r#"inbound_auth "query" requires a non-empty secret"#.into());
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Parse `raw_body` as JSON (empty body → empty object) and build trigger variables per config.
    ///
    /// `flatten_top_level` applies only when the root is a JSON object. Keys are used as variable names;
    /// scalars are stringified; objects and arrays are JSON-serialized (subject to size limits).
    pub fn map_payload_to_variables(
        &self,
        raw_body: &[u8],
    ) -> Result<HashMap<String, String>, WebhookVariableMapError> {
        if raw_body.len() > WEBHOOK_MAX_BODY_BYTES {
            return Err(WebhookVariableMapError::BodyTooLarge {
                max: WEBHOOK_MAX_BODY_BYTES,
                got: raw_body.len(),
            });
        }

        let root: JsonValue = if raw_body.is_empty() {
            JsonValue::Object(serde_json::Map::new())
        } else {
            serde_json::from_slice(raw_body)
                .map_err(|e| WebhookVariableMapError::InvalidJson(e.to_string()))?
        };

        let mut out = HashMap::new();
        let mut total_mapped = 0usize;

        if let Some(name) = &self.include_raw_body_variable
            && !name.is_empty()
        {
            let s = String::from_utf8_lossy(raw_body).into_owned();
            enforce_value_limit(name, &s, &mut total_mapped)?;
            out.insert(name.clone(), s);
        }

        if self.flatten_top_level
            && let JsonValue::Object(map) = &root
        {
            for (k, v) in map {
                if out.contains_key(k) {
                    continue;
                }
                let value_str = json_value_to_mapped_string(v)?;
                enforce_value_limit(k, &value_str, &mut total_mapped)?;
                out.insert(k.clone(), value_str);
            }
        }

        Ok(out)
    }
}

fn json_value_to_mapped_string(v: &JsonValue) -> Result<String, WebhookVariableMapError> {
    match v {
        JsonValue::Null => Ok(String::new()),
        JsonValue::Bool(b) => Ok(b.to_string()),
        JsonValue::Number(n) => Ok(n.to_string()),
        JsonValue::String(s) => Ok(s.clone()),
        JsonValue::Array(_) | JsonValue::Object(_) => serde_json::to_string(v)
            .map_err(|e| WebhookVariableMapError::InvalidJson(e.to_string())),
    }
}

fn enforce_value_limit(
    key: &str,
    value: &str,
    total_mapped: &mut usize,
) -> Result<(), WebhookVariableMapError> {
    let len = value.len();
    if len > WEBHOOK_MAX_VALUE_BYTES {
        return Err(WebhookVariableMapError::ValueTooLarge {
            key: key.to_string(),
            max: WEBHOOK_MAX_VALUE_BYTES,
        });
    }
    *total_mapped = total_mapped.saturating_add(len);
    if *total_mapped > WEBHOOK_MAX_TOTAL_MAPPED_BYTES {
        return Err(WebhookVariableMapError::TotalMappedTooLarge {
            max: WEBHOOK_MAX_TOTAL_MAPPED_BYTES,
        });
    }
    Ok(())
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

/// Partial update for a trigger.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateTrigger {
    /// When set, replaces enabled flag.
    pub enabled: Option<bool>,
    /// When set, replaces description (use empty string to clear if needed).
    pub description: Option<String>,
    /// When set, deep-merges into existing JSON config (objects are merged recursively; other types replace).
    pub config_patch: Option<JsonValue>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_payload_flattens_top_level() {
        let mut c = WebhookConfig::default();
        c.flatten_top_level = true;
        c.include_raw_body_variable = None;
        let body = br#"{"foo":"bar","n":42,"nested":{"a":1}}"#;
        let m = c.map_payload_to_variables(body).unwrap();
        assert_eq!(m["foo"], "bar");
        assert_eq!(m["n"], "42");
        assert_eq!(m["nested"], r#"{"a":1}"#);
    }

    #[test]
    fn map_payload_raw_body_variable() {
        let mut c = WebhookConfig::default();
        c.flatten_top_level = false;
        c.include_raw_body_variable = Some("RAW".into());
        let body = br#"{"x":1}"#;
        let m = c.map_payload_to_variables(body).unwrap();
        assert_eq!(m["RAW"], r#"{"x":1}"#);
        assert!(!m.contains_key("x"));
    }

    #[test]
    fn map_payload_empty_body() {
        let c = WebhookConfig::default();
        let m = c.map_payload_to_variables(b"").unwrap();
        assert!(m.is_empty());
    }

    #[test]
    fn map_payload_rejects_oversized_single_value() {
        let mut c = WebhookConfig::default();
        c.flatten_top_level = true;
        let big = "x".repeat(WEBHOOK_MAX_VALUE_BYTES + 1);
        let body = format!(r#"{{"k":"{big}"}}"#);
        let err = c.map_payload_to_variables(body.as_bytes()).unwrap_err();
        assert!(matches!(err, WebhookVariableMapError::ValueTooLarge { .. }));
    }

    #[test]
    fn map_payload_rejects_total_mapped_overflow() {
        let mut c = WebhookConfig::default();
        c.flatten_top_level = true;
        // Under per-value cap, but five values overflow total mapped bytes.
        let chunk = 52 * 1024;
        let a = "y".repeat(chunk);
        let b = "y".repeat(chunk);
        let c0 = "y".repeat(chunk);
        let d = "y".repeat(chunk);
        let e = "y".repeat(chunk);
        let body = format!(r#"{{"a":"{a}","b":"{b}","c":"{c0}","d":"{d}","e":"{e}"}}"#);
        let err = c.map_payload_to_variables(body.as_bytes()).unwrap_err();
        assert!(matches!(
            err,
            WebhookVariableMapError::TotalMappedTooLarge { .. }
        ));
    }

    #[test]
    fn map_payload_non_object_root_skips_flatten() {
        let mut c = WebhookConfig::default();
        c.flatten_top_level = true;
        let m = c.map_payload_to_variables(b"[1,2]").unwrap();
        assert!(m.is_empty());
    }
}
