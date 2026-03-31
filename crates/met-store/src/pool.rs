//! Database connection pool management.

use crate::error::{Result, StoreError};
use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::str::FromStr;
use std::time::Duration;

/// Configuration for the database connection pool.
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Database connection URL.
    pub url: String,
    /// Maximum number of connections.
    pub max_connections: u32,
    /// Minimum number of connections to maintain.
    pub min_connections: u32,
    /// Connection timeout.
    pub connect_timeout: Duration,
    /// Idle connection timeout.
    pub idle_timeout: Duration,
    /// Maximum connection lifetime.
    pub max_lifetime: Duration,
    /// Enable statement logging.
    pub log_statements: bool,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            url: "postgres://meticulous:meticulous@localhost:5432/meticulous".to_string(),
            max_connections: 10,
            min_connections: 1,
            connect_timeout: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(600),
            max_lifetime: Duration::from_secs(1800),
            log_statements: false,
        }
    }
}

impl From<&met_core::config::DatabaseConfig> for PoolConfig {
    fn from(cfg: &met_core::config::DatabaseConfig) -> Self {
        Self {
            url: cfg.url.clone(),
            max_connections: cfg.max_connections,
            min_connections: cfg.min_connections,
            connect_timeout: Duration::from_secs(cfg.connect_timeout_secs),
            log_statements: cfg.log_statements,
            ..Default::default()
        }
    }
}

/// Create a database connection pool.
///
/// # Errors
///
/// Returns an error if the connection URL is invalid or the pool cannot be created.
pub async fn create_pool(config: &PoolConfig) -> Result<PgPool> {
    let connect_options = PgConnectOptions::from_str(&config.url)?;

    let pool = PgPoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(config.connect_timeout)
        .idle_timeout(Some(config.idle_timeout))
        .max_lifetime(Some(config.max_lifetime))
        .connect_with(connect_options)
        .await?;

    tracing::info!(
        max_connections = config.max_connections,
        "database pool created"
    );

    Ok(pool)
}

/// Run database migrations.
///
/// # Errors
///
/// Returns an error if migrations fail.
pub async fn run_migrations(pool: &PgPool) -> Result<()> {
    tracing::info!("running database migrations");

    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .map_err(StoreError::Migration)?;

    tracing::info!("migrations completed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = PoolConfig::default();
        assert_eq!(config.max_connections, 10);
        assert_eq!(config.min_connections, 1);
    }
}
