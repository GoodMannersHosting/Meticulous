//! Append-only description history for join tokens.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ids::JoinTokenId;

/// One row in the join token description audit trail.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct JoinTokenDescriptionHistory {
    pub id: Uuid,
    pub join_token_id: JoinTokenId,
    pub description: String,
    pub changed_at: DateTime<Utc>,
    pub changed_by: Option<Uuid>,
}
