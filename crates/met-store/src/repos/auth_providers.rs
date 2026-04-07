//! Auth provider repository.

use chrono::Utc;
use met_core::ids::{AuthProviderId, GroupId, OidcGroupMappingId, OrganizationId};
use met_core::models::{
    AuthProvider, CreateAuthProvider, CreateOidcGroupMapping, GroupRole, OidcGroupMapping,
    UpdateAuthProvider,
};
use sqlx::PgPool;

use crate::error::{Result, StoreError};

/// Repository for auth provider operations.
pub struct AuthProviderRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> AuthProviderRepo<'a> {
    /// Create a new auth provider repository.
    #[must_use]
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Create a new auth provider.
    pub async fn create(
        &self,
        org_id: OrganizationId,
        input: &CreateAuthProvider,
        client_secret_ref: &str,
    ) -> Result<AuthProvider> {
        let id = AuthProviderId::new();
        let now = Utc::now();

        let provider = sqlx::query_as::<_, AuthProvider>(
            r#"
            INSERT INTO auth_providers (id, org_id, provider_type, name, client_id, client_secret_ref, issuer_url, enabled, config, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, false, $8, $9, $9)
            RETURNING id, org_id, provider_type, name, client_id, client_secret_ref, issuer_url, enabled, config, created_at, updated_at
            "#,
        )
        .bind(id.as_uuid())
        .bind(org_id.as_uuid())
        .bind(&input.provider_type)
        .bind(&input.name)
        .bind(&input.client_id)
        .bind(client_secret_ref)
        .bind(&input.issuer_url)
        .bind(&input.config)
        .bind(now)
        .fetch_one(self.pool)
        .await?;

        Ok(provider)
    }

    /// Get a provider by ID.
    pub async fn get(&self, id: AuthProviderId) -> Result<AuthProvider> {
        sqlx::query_as::<_, AuthProvider>(
            r#"
            SELECT id, org_id, provider_type, name, client_id, client_secret_ref, issuer_url, enabled, config, created_at, updated_at
            FROM auth_providers
            WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("auth_provider", id))
    }

    /// Get the enabled provider of a specific type for an org.
    pub async fn get_enabled_by_type(
        &self,
        org_id: OrganizationId,
        provider_type: &str,
    ) -> Result<Option<AuthProvider>> {
        let provider = sqlx::query_as::<_, AuthProvider>(
            r#"
            SELECT id, org_id, provider_type, name, client_id, client_secret_ref, issuer_url, enabled, config, created_at, updated_at
            FROM auth_providers
            WHERE org_id = $1 AND provider_type = $2 AND enabled = true
            "#,
        )
        .bind(org_id.as_uuid())
        .bind(provider_type)
        .fetch_optional(self.pool)
        .await?;

        Ok(provider)
    }

    /// List providers for an organization.
    pub async fn list(&self, org_id: OrganizationId) -> Result<Vec<AuthProvider>> {
        let providers = sqlx::query_as::<_, AuthProvider>(
            r#"
            SELECT id, org_id, provider_type, name, client_id, client_secret_ref, issuer_url, enabled, config, created_at, updated_at
            FROM auth_providers
            WHERE org_id = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(org_id.as_uuid())
        .fetch_all(self.pool)
        .await?;

        Ok(providers)
    }

    /// Update a provider.
    pub async fn update(
        &self,
        id: AuthProviderId,
        input: &UpdateAuthProvider,
        client_secret_ref: Option<&str>,
    ) -> Result<AuthProvider> {
        let existing = self.get(id).await?;

        let name = input.name.as_ref().unwrap_or(&existing.name);
        let client_id = input.client_id.as_ref().unwrap_or(&existing.client_id);
        let secret_ref = client_secret_ref.unwrap_or(&existing.client_secret_ref);
        let issuer_url = input.issuer_url.as_ref().or(existing.issuer_url.as_ref());
        let config = input.config.as_ref().unwrap_or(&existing.config);

        let provider = sqlx::query_as::<_, AuthProvider>(
            r#"
            UPDATE auth_providers
            SET name = $2, client_id = $3, client_secret_ref = $4, issuer_url = $5, config = $6, updated_at = NOW()
            WHERE id = $1
            RETURNING id, org_id, provider_type, name, client_id, client_secret_ref, issuer_url, enabled, config, created_at, updated_at
            "#,
        )
        .bind(id.as_uuid())
        .bind(name)
        .bind(client_id)
        .bind(secret_ref)
        .bind(issuer_url)
        .bind(config)
        .fetch_one(self.pool)
        .await?;

        Ok(provider)
    }

    /// Enable a provider (disables others of the same type in the org).
    pub async fn enable(&self, id: AuthProviderId) -> Result<AuthProvider> {
        let provider = self.get(id).await?;

        // Disable all other providers of the same type
        sqlx::query(
            r#"
            UPDATE auth_providers
            SET enabled = false, updated_at = NOW()
            WHERE org_id = $1 AND provider_type = $2 AND id != $3 AND enabled = true
            "#,
        )
        .bind(provider.org_id.as_uuid())
        .bind(&provider.provider_type)
        .bind(id.as_uuid())
        .execute(self.pool)
        .await?;

        // Enable this provider
        let updated = sqlx::query_as::<_, AuthProvider>(
            r#"
            UPDATE auth_providers
            SET enabled = true, updated_at = NOW()
            WHERE id = $1
            RETURNING id, org_id, provider_type, name, client_id, client_secret_ref, issuer_url, enabled, config, created_at, updated_at
            "#,
        )
        .bind(id.as_uuid())
        .fetch_one(self.pool)
        .await?;

        Ok(updated)
    }

    /// Disable a provider.
    pub async fn disable(&self, id: AuthProviderId) -> Result<AuthProvider> {
        let provider = sqlx::query_as::<_, AuthProvider>(
            r#"
            UPDATE auth_providers
            SET enabled = false, updated_at = NOW()
            WHERE id = $1
            RETURNING id, org_id, provider_type, name, client_id, client_secret_ref, issuer_url, enabled, config, created_at, updated_at
            "#,
        )
        .bind(id.as_uuid())
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("auth_provider", id))?;

        Ok(provider)
    }

    /// Delete a provider.
    pub async fn delete(&self, id: AuthProviderId) -> Result<()> {
        let result = sqlx::query(
            r#"
            DELETE FROM auth_providers WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(StoreError::not_found("auth_provider", id));
        }

        Ok(())
    }

    /// Create an OIDC group mapping.
    pub async fn create_group_mapping(
        &self,
        provider_id: AuthProviderId,
        input: &CreateOidcGroupMapping,
    ) -> Result<OidcGroupMapping> {
        let id = OidcGroupMappingId::new();
        let now = Utc::now();

        let mapping = sqlx::query_as::<_, OidcGroupMapping>(
            r#"
            INSERT INTO oidc_group_mappings (id, provider_id, oidc_group_claim, meticulous_group_id, role, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, provider_id, oidc_group_claim, meticulous_group_id, role, created_at
            "#,
        )
        .bind(id.as_uuid())
        .bind(provider_id.as_uuid())
        .bind(&input.oidc_group_claim)
        .bind(input.meticulous_group_id.as_uuid())
        .bind(input.role)
        .bind(now)
        .fetch_one(self.pool)
        .await?;

        Ok(mapping)
    }

    /// Get a group mapping by ID.
    pub async fn get_group_mapping(&self, id: OidcGroupMappingId) -> Result<OidcGroupMapping> {
        sqlx::query_as::<_, OidcGroupMapping>(
            r#"
            SELECT id, provider_id, oidc_group_claim, meticulous_group_id, role, created_at
            FROM oidc_group_mappings
            WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| StoreError::not_found("oidc_group_mapping", id))
    }

    /// List group mappings for a provider.
    pub async fn list_group_mappings(
        &self,
        provider_id: AuthProviderId,
    ) -> Result<Vec<OidcGroupMapping>> {
        let mappings = sqlx::query_as::<_, OidcGroupMapping>(
            r#"
            SELECT id, provider_id, oidc_group_claim, meticulous_group_id, role, created_at
            FROM oidc_group_mappings
            WHERE provider_id = $1
            ORDER BY oidc_group_claim ASC
            "#,
        )
        .bind(provider_id.as_uuid())
        .fetch_all(self.pool)
        .await?;

        Ok(mappings)
    }

    /// Delete a group mapping.
    pub async fn delete_group_mapping(&self, id: OidcGroupMappingId) -> Result<()> {
        let result = sqlx::query(
            r#"
            DELETE FROM oidc_group_mappings WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(StoreError::not_found("oidc_group_mapping", id));
        }

        Ok(())
    }

    /// Find mappings for a list of OIDC group claims.
    pub async fn find_mappings_for_claims(
        &self,
        provider_id: AuthProviderId,
        claims: &[String],
    ) -> Result<Vec<OidcGroupMapping>> {
        let mappings = sqlx::query_as::<_, OidcGroupMapping>(
            r#"
            SELECT id, provider_id, oidc_group_claim, meticulous_group_id, role, created_at
            FROM oidc_group_mappings
            WHERE provider_id = $1 AND oidc_group_claim = ANY($2)
            "#,
        )
        .bind(provider_id.as_uuid())
        .bind(claims)
        .fetch_all(self.pool)
        .await?;

        Ok(mappings)
    }
}
