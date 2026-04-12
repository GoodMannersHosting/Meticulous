//! Typed ID wrappers for compile-time safety.
//!
//! Every entity in Meticulous has a unique ID type that wraps a UUIDv7.
//! This prevents accidentally mixing IDs across entity types at compile time.
//!
//! ## Global uniqueness invariant
//!
//! `PipelineId`, `RunId`, `JobRunId`, and `StepRunId` **must** be globally
//! unique (not just unique within a tenant). This is security-critical because:
//!
//! - **OIDC tokens** (ADR-017) embed `job_run_id` in the `sub` claim.
//!   A collision could let one job's token authenticate as another.
//! - **Per-job PKI** (ADR-004) derives encryption keys per `job_run_id`.
//!   A collision would reuse key material.
//! - **Secret resolution hints** reference `pipeline_id` + `job_run_id` to
//!   scope which secrets are delivered to which job.
//!
//! UUIDv7 provides 74 bits of randomness per ID (plus monotonic time), giving
//! a birthday-attack collision probability below 2^-50 even at 10 billion IDs.
//! The Postgres `UUID PRIMARY KEY` constraint provides a database-level backstop.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

/// Macro to define a typed ID wrapper around UUIDv7.
macro_rules! define_id {
    ($(#[$meta:meta])* $name:ident, $prefix:literal) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
        #[cfg_attr(feature = "sqlx", sqlx(transparent))]
        pub struct $name(pub Uuid);

        impl $name {
            /// Create a new ID with a fresh UUIDv7 (time-sortable, globally unique).
            ///
            /// # Panics
            ///
            /// Panics if the system CSPRNG produces a nil UUID (should never happen).
            #[must_use]
            pub fn new() -> Self {
                let id = Uuid::now_v7();
                assert!(!id.is_nil(), "CSPRNG produced a nil UUID");
                Self(id)
            }

            /// Create an ID from an existing UUID.
            #[must_use]
            pub const fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }

            /// Get the inner UUID.
            #[must_use]
            pub const fn as_uuid(&self) -> Uuid {
                self.0
            }

            /// Get the string prefix for this ID type.
            #[must_use]
            pub const fn prefix() -> &'static str {
                $prefix
            }

            /// Parse from a prefixed string (e.g., "org_01234...").
            pub fn from_prefixed(s: &str) -> Result<Self, IdParseError> {
                let expected_prefix = concat!($prefix, "_");
                if let Some(uuid_part) = s.strip_prefix(expected_prefix) {
                    let uuid = Uuid::from_str(uuid_part).map_err(|e| IdParseError::InvalidUuid(e.to_string()))?;
                    Ok(Self(uuid))
                } else if let Ok(uuid) = Uuid::from_str(s) {
                    // Allow raw UUIDs for backward compatibility
                    Ok(Self(uuid))
                } else {
                    Err(IdParseError::InvalidPrefix {
                        expected: $prefix,
                        got: s.to_string(),
                    })
                }
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}_{}", $prefix, self.0)
            }
        }

        impl FromStr for $name {
            type Err = IdParseError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::from_prefixed(s)
            }
        }

        impl From<Uuid> for $name {
            fn from(uuid: Uuid) -> Self {
                Self(uuid)
            }
        }

        impl From<$name> for Uuid {
            fn from(id: $name) -> Self {
                id.0
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                // Serialize as prefixed string: "prefix_uuid"
                serializer.serialize_str(&self.to_string())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let s = String::deserialize(deserializer)?;
                Self::from_str(&s).map_err(serde::de::Error::custom)
            }
        }
    };
}

/// Error returned when parsing an ID from a string fails.
#[derive(Debug, Clone, thiserror::Error)]
pub enum IdParseError {
    /// The UUID portion was invalid.
    #[error("invalid UUID: {0}")]
    InvalidUuid(String),

    /// The prefix didn't match the expected type.
    #[error("invalid prefix: expected '{expected}_', got '{got}'")]
    InvalidPrefix {
        /// The expected prefix.
        expected: &'static str,
        /// What was actually provided.
        got: String,
    },
}

// Organization and tenant hierarchy
define_id!(
    /// Unique identifier for an organization (tenant boundary).
    OrganizationId,
    "org"
);

define_id!(
    /// Unique identifier for a project.
    ProjectId,
    "proj"
);

// Pipeline and execution hierarchy
define_id!(
    /// Unique identifier for a pipeline definition.
    PipelineId,
    "pipe"
);

define_id!(
    /// Unique identifier for a job within a pipeline.
    JobId,
    "job"
);

define_id!(
    /// Unique identifier for a step within a job.
    StepId,
    "step"
);

define_id!(
    /// Unique identifier for a pipeline run (execution instance).
    RunId,
    "run"
);

define_id!(
    /// Unique identifier for a job run (job execution within a run).
    JobRunId,
    "jrun"
);

define_id!(
    /// Unique identifier for a step run (step execution within a job run).
    StepRunId,
    "srun"
);

// Agents
define_id!(
    /// Unique identifier for a build agent.
    AgentId,
    "agt"
);

define_id!(
    /// Unique identifier for an agent pool.
    AgentPoolId,
    "pool"
);

// Secrets and variables
define_id!(
    /// Unique identifier for a secret reference.
    SecretId,
    "sec"
);

define_id!(
    /// Unique identifier for a variable.
    VariableId,
    "var"
);

// Triggers and workflows
define_id!(
    /// Unique identifier for a pipeline trigger.
    TriggerId,
    "trg"
);

define_id!(
    /// Unique identifier for a reusable workflow.
    WorkflowId,
    "wf"
);

// Artifacts
define_id!(
    /// Unique identifier for a build artifact.
    ArtifactId,
    "art"
);

// Users and groups
define_id!(
    /// Unique identifier for a user.
    UserId,
    "usr"
);

define_id!(
    /// Unique identifier for a group.
    GroupId,
    "grp"
);

// Tokens
define_id!(
    /// Unique identifier for an API or agent token.
    TokenId,
    "tok"
);

define_id!(
    /// Unique identifier for an agent join token.
    JoinTokenId,
    "jt"
);

define_id!(
    /// Unique identifier for an agent heartbeat record.
    AgentHeartbeatId,
    "hb"
);

define_id!(
    /// Unique identifier for a job assignment.
    JobAssignmentId,
    "ja"
);

define_id!(
    /// Unique identifier for an API token.
    ApiTokenId,
    "apt"
);

define_id!(
    /// Unique identifier for an auth provider.
    AuthProviderId,
    "auth"
);

define_id!(
    /// Unique identifier for an OIDC group mapping.
    OidcGroupMappingId,
    "ogm"
);

// Meticulous Apps (integration / machine auth)
define_id!(
    /// Internal row id for a Meticulous App registration.
    MeticulousAppId,
    "mapp"
);

define_id!(
    /// Installation of a Meticulous App in a project.
    AppInstallationId,
    "appi"
);

define_id!(
    /// Signing key row for a Meticulous App (JWT `kid` + public key).
    AppKeyId,
    "apk"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_id_generation() {
        let id1 = OrganizationId::new();
        let id2 = OrganizationId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_id_never_nil() {
        let id = PipelineId::new();
        assert!(!id.as_uuid().is_nil());
        let id = RunId::new();
        assert!(!id.as_uuid().is_nil());
        let id = JobRunId::new();
        assert!(!id.as_uuid().is_nil());
    }

    #[test]
    fn test_global_uniqueness_across_types() {
        let ids: Vec<Uuid> = (0..100)
            .flat_map(|_| {
                vec![
                    PipelineId::new().as_uuid(),
                    RunId::new().as_uuid(),
                    JobRunId::new().as_uuid(),
                    StepRunId::new().as_uuid(),
                ]
            })
            .collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique.len(), "UUIDv7 collision detected");
    }

    #[test]
    fn test_id_display() {
        let uuid = Uuid::nil();
        let id = OrganizationId::from_uuid(uuid);
        assert_eq!(id.to_string(), "org_00000000-0000-0000-0000-000000000000");
    }

    #[test]
    fn test_id_parse_prefixed() {
        let id = OrganizationId::new();
        let s = id.to_string();
        let parsed: OrganizationId = s.parse().unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_id_parse_raw_uuid() {
        let uuid = Uuid::now_v7();
        let parsed: OrganizationId = uuid.to_string().parse().unwrap();
        assert_eq!(parsed.as_uuid(), uuid);
    }

    #[test]
    fn test_id_parse_wrong_prefix() {
        let result: Result<OrganizationId, _> = "proj_00000000-0000-0000-0000-000000000000".parse();
        assert!(result.is_err());
    }

    #[test]
    fn test_id_serialization() {
        let id = ProjectId::new();
        let json = serde_json::to_string(&id).unwrap();
        let parsed: ProjectId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }
}
