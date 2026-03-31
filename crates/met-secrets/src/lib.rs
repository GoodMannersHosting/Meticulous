//! Security core for Meticulous CI/CD.
//!
//! This crate provides the security infrastructure for Meticulous:
//!
//! - **Secrets Management**: Pluggable providers for HashiCorp Vault, AWS Secrets Manager,
//!   Kubernetes secrets, and built-in encrypted storage
//! - **OIDC Validation**: JWT token validation with multi-issuer support and JWKS caching
//! - **RBAC**: Role-based access control with hierarchical permissions
//! - **Audit Logging**: Comprehensive audit trail for security-relevant events
//!
//! # Quick Start
//!
//! ## Secrets
//!
//! ```ignore
//! use met_secrets::{providers::VaultProvider, SecretsProvider, ProviderType};
//!
//! // Create a Vault provider
//! let provider = VaultProvider::new(VaultConfig {
//!     address: "https://vault.example.com:8200".into(),
//!     token: Some("s.mytoken".into()),
//!     ..Default::default()
//! }).await?;
//!
//! // Fetch a secret
//! let secret = provider.get_secret("secret/myapp/api-key").await?;
//! println!("Got secret: {} bytes", secret.len());
//! ```
//!
//! ## OIDC Validation
//!
//! ```ignore
//! use met_secrets::oidc::{OidcValidator, OidcValidatorBuilder};
//!
//! let validator = OidcValidatorBuilder::new()
//!     .with_simple_issuer("https://auth.example.com", "meticulous-api")
//!     .build()
//!     .await?;
//!
//! let claims = validator.validate_token(bearer_token).await?;
//! println!("Authenticated user: {}", claims.subject);
//! ```
//!
//! ## RBAC
//!
//! ```ignore
//! use met_secrets::rbac::{RbacPolicy, Actor, Resource, Role, Permission};
//!
//! let policy = RbacPolicy::new();
//! let actor = Actor::user(user_id, Role::Developer).in_org(org_id);
//! let resource = Resource::pipeline("pipe_123", project_id);
//!
//! if policy.check(&actor, Permission::RunTrigger, &resource) {
//!     // User can trigger the pipeline
//! }
//! ```
//!
//! ## Audit Logging
//!
//! ```ignore
//! use met_secrets::audit::{AuditEvent, AuditAction, TracingAuditLogger, AuditLogger};
//!
//! let logger = TracingAuditLogger;
//!
//! let event = AuditEvent::new(AuditAction::SecretAccess)
//!     .with_actor("user:usr_123")
//!     .with_resource("secret", "sec_456")
//!     .success();
//!
//! logger.log(event).await?;
//! ```
//!
//! # Architecture
//!
//! ## Secrets Providers
//!
//! The secrets system uses a trait-based design allowing multiple backends:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │                    SecretsBroker                        │
//! │  (routes requests to appropriate provider)              │
//! └────────────────────────┬────────────────────────────────┘
//!                          │
//!          ┌───────────────┼───────────────┐
//!          ▼               ▼               ▼
//!    ┌──────────┐   ┌──────────┐   ┌──────────┐
//!    │  Vault   │   │   AWS    │   │   K8s    │
//!    │ Provider │   │ Provider │   │ Provider │
//!    └──────────┘   └──────────┘   └──────────┘
//! ```
//!
//! ## Security Model
//!
//! - **Zero Trust**: Secrets are never stored locally; always fetched at runtime
//! - **Secure Memory**: Secret values use `zeroize` for automatic memory clearing
//! - **Audit Trail**: All secret access is logged for compliance
//! - **RBAC**: Fine-grained permissions control who can access what

pub mod audit;
pub mod error;
pub mod oidc;
pub mod providers;
pub mod rbac;
pub mod traits;
pub mod types;

// Re-export commonly used types at crate root
pub use error::{OidcError, RbacError, SecretsError};
pub use traits::{ProviderConfig, SecretRef, SecretsBroker, SecretsProvider, SecretsWriter};
pub use types::{ProviderType, SecretBytes, SecretMetadata, SecretPath, SecretValue};

// Re-export providers for convenience
pub use providers::{
    AwsSecretsProvider, BuiltinSecretsProvider, KubernetesSecretsProvider, MultiProviderBroker,
    VaultProvider,
};

// Re-export RBAC types
pub use rbac::{Actor, ActorId, Permission, RbacPolicy, Resource, ResourceType, Role};

// Re-export audit types
pub use audit::{AuditAction, AuditEvent, AuditFilter, AuditLogger, Outcome, Severity};

// Re-export OIDC types
pub use oidc::{OidcConfig, OidcValidator, OidcValidatorBuilder, ValidatedClaims};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crate_exports() {
        // Verify key types are accessible at crate root
        let _: ProviderType = ProviderType::Vault;
        let _: Role = Role::Developer;
        let _: Permission = Permission::ViewRuns;
        let _: AuditAction = AuditAction::Login;
        let _: Outcome = Outcome::Success;
    }

    #[test]
    fn test_secret_value_basics() {
        let secret = SecretValue::new("my-secret-value");
        assert_eq!(secret.expose_secret(), "my-secret-value");
        assert_eq!(secret.len(), 15);
        assert!(!secret.is_empty());

        // Debug output should be redacted
        let debug = format!("{:?}", secret);
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("my-secret-value"));
    }

    #[test]
    fn test_provider_type_roundtrip() {
        let types = [
            ProviderType::Vault,
            ProviderType::AwsSecretsManager,
            ProviderType::Kubernetes,
            ProviderType::Builtin,
        ];

        for pt in types {
            let s = pt.as_str();
            let parsed: ProviderType = s.parse().unwrap();
            assert_eq!(pt, parsed);
        }
    }
}
