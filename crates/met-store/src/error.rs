//! Database-specific error types.

use met_core::MetError;

/// Database operation errors.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// SQL query or connection error.
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    /// Migration failed.
    #[error("migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    /// Entity not found.
    #[error("not found: {entity} with id {id}")]
    NotFound {
        /// Entity type.
        entity: &'static str,
        /// Entity ID.
        id: String,
    },

    /// Constraint violation (unique, foreign key, etc.).
    #[error("constraint violation: {0}")]
    Constraint(String),

    /// Connection pool exhausted.
    #[error("connection pool exhausted")]
    PoolExhausted,
}

impl StoreError {
    /// Create a not-found error.
    #[must_use]
    pub fn not_found(entity: &'static str, id: impl ToString) -> Self {
        Self::NotFound {
            entity,
            id: id.to_string(),
        }
    }

    /// Check if this is a not-found error.
    #[must_use]
    pub const fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound { .. })
    }

    /// Check if this is a unique constraint violation.
    #[must_use]
    pub fn is_unique_violation(&self) -> bool {
        match self {
            Self::Database(e) => e
                .as_database_error()
                .is_some_and(|de| de.code().is_some_and(|c| c == "23505")),
            _ => false,
        }
    }
}

impl From<StoreError> for MetError {
    fn from(err: StoreError) -> Self {
        match err {
            StoreError::Database(e) => Self::Database(e),
            StoreError::Migration(e) => Self::Internal(format!("migration failed: {e}")),
            StoreError::NotFound { entity, id } => Self::NotFound { entity, id },
            StoreError::Constraint(msg) => Self::Validation(msg),
            StoreError::PoolExhausted => Self::Internal("database pool exhausted".to_string()),
        }
    }
}

/// Result type for store operations.
pub type Result<T> = std::result::Result<T, StoreError>;
