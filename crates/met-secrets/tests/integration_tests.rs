//! Integration tests for the met-secrets security crate.
//!
//! Tests cover: PKI flow, hybrid encryption, masking, RBAC, blast radius, and audit types.

use met_secrets::{
    // PKI
    pki::{
        ca::{CaConfig, CertificateAuthority},
        encryption::{EncryptedEnvelope, HybridDecryption, HybridEncryption},
        ephemeral::EphemeralKeypair,
    },
    // Masking
    masking::{ControlPlaneMaskingFilter, SecretMaskingFilter},
    // RBAC
    rbac::{Actor, Permission, RbacPolicy, Resource, ResourceType, Role},
    // Audit
    audit::{AuditAction, AuditActor, AuditEvent, AuditLogger, Outcome, TracingAuditLogger},
    // Syscall / Blast radius
    syscall_audit::{BlastRadiusTracker, NetworkConnection, SyscallAuditCollector},
    // Types
    ProviderType, SecretValue,
};

use aes_gcm::aead::OsRng;
use met_core::{OrganizationId, ProjectId, UserId};
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};

// ──────────────────────────────────────────────────
// PKI: full flow (CA → ephemeral keypair → sign CSR)
// ──────────────────────────────────────────────────

#[tokio::test]
async fn pki_full_flow() {
    let ca = CertificateAuthority::new(CaConfig {
        organization: "TestOrg".to_string(),
        common_name: "Test Intermediate CA".to_string(),
        max_job_cert_duration: std::time::Duration::from_secs(300),
        serial_seed: 1,
    })
    .expect("CA creation must succeed");

    assert!(ca.ca_certificate_pem().contains("BEGIN CERTIFICATE"));

    let ephemeral = EphemeralKeypair::generate().expect("keygen must succeed");
    let kp_der = ephemeral.key_pair_der();
    assert!(!kp_der.is_empty());

    let signed_cert = ca
        .sign_csr("agent-integration", "job-42", kp_der)
        .await
        .expect("CSR signing must succeed");

    assert!(signed_cert.certificate_pem.contains("BEGIN CERTIFICATE"));
    assert_eq!(
        signed_cert.subject_cn,
        "agent:agent-integration/job:job-42"
    );
    assert!(signed_cert.not_after > signed_cert.not_before);
    assert_eq!(signed_cert.public_key_fingerprint.len(), 64); // SHA-256 hex

    let cert2 = ca
        .sign_csr("agent-integration", "job-43", kp_der)
        .await
        .unwrap();
    assert_ne!(signed_cert.serial_number, cert2.serial_number);
}

#[tokio::test]
async fn pki_multiple_agents_distinct_serials() {
    let ca = CertificateAuthority::new(CaConfig::default()).unwrap();

    let mut serials = std::collections::HashSet::new();
    for i in 0..10 {
        let kp = EphemeralKeypair::generate().unwrap();
        let cert = ca
            .sign_csr(&format!("agent-{i}"), &format!("job-{i}"), kp.key_pair_der())
            .await
            .unwrap();
        assert!(
            serials.insert(cert.serial_number.clone()),
            "serial must be unique, got duplicate: {}",
            cert.serial_number
        );
    }
}

// ──────────────────────────────────────────────────
// Hybrid encryption: encrypt/decrypt roundtrip
// ──────────────────────────────────────────────────

#[test]
fn hybrid_encryption_roundtrip() {
    let recipient_secret = StaticSecret::random_from_rng(OsRng);
    let recipient_public = X25519PublicKey::from(&recipient_secret);
    let hmac_key = b"integration-test-hmac-key-value!";

    let plaintext = b"database-password=hunter2";

    let envelope = HybridEncryption::encrypt(
        &recipient_public.to_bytes(),
        plaintext,
        hmac_key,
    )
    .expect("encryption must succeed");

    // Serialize → deserialize the envelope (simulates network transit)
    let bytes = envelope.to_bytes();
    let restored = EncryptedEnvelope::from_bytes(&bytes).expect("deserialization must succeed");

    let decrypted =
        HybridDecryption::decrypt(&recipient_secret.to_bytes(), &restored, hmac_key)
            .expect("decryption must succeed");

    assert_eq!(&*decrypted, plaintext);
}

#[test]
fn hybrid_encryption_wrong_key_rejected() {
    let real_secret = StaticSecret::random_from_rng(OsRng);
    let real_public = X25519PublicKey::from(&real_secret);
    let wrong_secret = StaticSecret::random_from_rng(OsRng);
    let hmac_key = b"integration-test-hmac-key-value!";

    let envelope =
        HybridEncryption::encrypt(&real_public.to_bytes(), b"secret-data", hmac_key).unwrap();

    let result =
        HybridDecryption::decrypt(&wrong_secret.to_bytes(), &envelope, hmac_key);
    assert!(result.is_err(), "decryption with wrong key must fail");
}

#[test]
fn hybrid_encryption_tampered_hmac_rejected() {
    let secret = StaticSecret::random_from_rng(OsRng);
    let public = X25519PublicKey::from(&secret);
    let hmac_key = b"integration-test-hmac-key-value!";

    let mut envelope =
        HybridEncryption::encrypt(&public.to_bytes(), b"secret", hmac_key).unwrap();

    // Tamper with the HMAC
    envelope.plaintext_hmac[0] ^= 0xff;

    let result = HybridDecryption::decrypt(&secret.to_bytes(), &envelope, hmac_key);
    assert!(result.is_err(), "tampered HMAC must be rejected");
}

#[test]
fn hybrid_encryption_wrong_hmac_key_rejected() {
    let secret = StaticSecret::random_from_rng(OsRng);
    let public = X25519PublicKey::from(&secret);
    let hmac_key = b"correct-hmac-key-for-encrypt!!!!";
    let wrong_hmac = b"wrong---hmac-key-for-decrypt!!!!";

    let envelope =
        HybridEncryption::encrypt(&public.to_bytes(), b"payload", hmac_key).unwrap();

    let result = HybridDecryption::decrypt(&secret.to_bytes(), &envelope, wrong_hmac);
    assert!(result.is_err(), "wrong HMAC key must be rejected");
}

#[test]
fn hybrid_encryption_envelope_too_short() {
    let result = EncryptedEnvelope::from_bytes(&[0u8; 10]);
    assert!(result.is_err());
}

#[test]
fn hybrid_encryption_large_payload() {
    let secret = StaticSecret::random_from_rng(OsRng);
    let public = X25519PublicKey::from(&secret);
    let hmac_key = b"integration-test-hmac-key-value!";

    let plaintext = vec![0xAB_u8; 1024 * 1024]; // 1 MiB

    let envelope =
        HybridEncryption::encrypt(&public.to_bytes(), &plaintext, hmac_key).unwrap();
    let decrypted =
        HybridDecryption::decrypt(&secret.to_bytes(), &envelope, hmac_key).unwrap();

    assert_eq!(&*decrypted, &plaintext);
}

// ──────────────────────────────────────────────────
// Masking: agent-side and control-plane filters
// ──────────────────────────────────────────────────

#[test]
fn masking_raw_secret() {
    let filter = SecretMaskingFilter::new();
    filter.add_secret("my-database-password-12345");

    let masked = filter.mask("Connecting with password my-database-password-12345 to db");
    assert!(
        !masked.contains("my-database-password-12345"),
        "raw secret must be masked"
    );
    assert!(masked.contains("***"));
}

#[test]
fn masking_base64_encoded_secret() {
    let filter = SecretMaskingFilter::new();
    let secret = "api-key-very-secret-value";
    filter.add_secret(secret);

    let b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        secret.as_bytes(),
    );
    let masked = filter.mask(&format!("Authorization: Basic {b64}"));
    assert!(!masked.contains(&b64), "base64 variant must be masked");
}

#[test]
fn masking_github_token_pattern() {
    let filter = SecretMaskingFilter::new();
    let input = "export GITHUB_TOKEN=ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijkl";
    let masked = filter.mask(input);
    assert!(!masked.contains("ghp_"), "GitHub token pattern must be masked");
}

#[test]
fn masking_aws_key_pattern() {
    let filter = SecretMaskingFilter::new();
    let input = "AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE";
    let masked = filter.mask(input);
    assert!(!masked.contains("AKIA"), "AWS key pattern must be masked");
}

#[test]
fn masking_jwt_pattern() {
    let filter = SecretMaskingFilter::new();
    let jwt = "eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiJ1c2VyIn0.signature-part-here";
    let masked = filter.mask(&format!("Bearer {jwt}"));
    assert!(!masked.contains("eyJ"), "JWT pattern must be masked");
}

#[test]
fn masking_private_key_pattern() {
    let filter = SecretMaskingFilter::new();
    let input = "key data: -----BEGIN RSA PRIVATE KEY----- MIIE...";
    let masked = filter.mask(input);
    assert!(
        !masked.contains("-----BEGIN RSA PRIVATE KEY-----"),
        "private key marker must be masked"
    );
}

#[test]
fn masking_multiple_secrets_in_one_line() {
    let filter = SecretMaskingFilter::new();
    filter.add_secret("secret-alpha-value");
    filter.add_secret("secret-beta-value-long");

    let masked =
        filter.mask("DB=secret-alpha-value API=secret-beta-value-long done");
    assert!(!masked.contains("secret-alpha"));
    assert!(!masked.contains("secret-beta"));
}

#[test]
fn masking_short_secrets_ignored() {
    let filter = SecretMaskingFilter::new();
    filter.add_secret("ab");
    assert_eq!(filter.secret_count(), 0);
}

#[test]
fn masking_contains_secret_detection() {
    let filter = SecretMaskingFilter::new();
    filter.add_secret("detect-this-secret");
    assert!(filter.contains_secret("log line with detect-this-secret in it"));
    assert!(!filter.contains_secret("nothing sensitive here"));
}

#[test]
fn masking_control_plane_defense_in_depth() {
    let cp = ControlPlaneMaskingFilter::new();
    cp.add_secret("escaped-secret-value");

    let masked = cp.mask("log: escaped-secret-value leaked");
    assert!(!masked.contains("escaped-secret-value"));
}

// ──────────────────────────────────────────────────
// RBAC: role hierarchy, permissions, scoping
// ──────────────────────────────────────────────────

#[test]
fn rbac_role_hierarchy() {
    assert!(Role::PlatformAdmin.has_at_least(Role::OrgAdmin));
    assert!(Role::PlatformAdmin.has_at_least(Role::Viewer));
    assert!(Role::Developer.has_at_least(Role::Viewer));
    assert!(!Role::Viewer.has_at_least(Role::Developer));
    assert!(!Role::Developer.has_at_least(Role::ProjectAdmin));
}

#[test]
fn rbac_permission_minimum_role() {
    assert_eq!(
        Permission::CreateOrganization.minimum_role(),
        Role::PlatformAdmin
    );
    assert_eq!(Permission::ManageSecrets.minimum_role(), Role::ProjectAdmin);
    assert_eq!(Permission::RunTrigger.minimum_role(), Role::Developer);
    assert_eq!(Permission::ViewRuns.minimum_role(), Role::Viewer);
}

#[test]
fn rbac_developer_can_trigger_but_not_manage_secrets() {
    let policy = RbacPolicy::new();
    let org_id = OrganizationId::new();
    let project_id = ProjectId::new();

    let developer = Actor::user(UserId::new(), Role::Developer)
        .in_org(org_id)
        .in_project(project_id);
    let resource = Resource::pipeline("pipe_1", project_id).in_org(org_id);

    assert!(policy.check(&developer, Permission::RunTrigger, &resource));
    assert!(policy.check(&developer, Permission::ViewRuns, &resource));
    assert!(!policy.check(&developer, Permission::ManageSecrets, &resource));
    assert!(!policy.check(&developer, Permission::CreateOrganization, &resource));
}

#[test]
fn rbac_viewer_read_only() {
    let policy = RbacPolicy::new();
    let viewer = Actor::user(UserId::new(), Role::Viewer);
    let resource = Resource::new(ResourceType::Pipeline);

    assert!(policy.check(&viewer, Permission::ViewRuns, &resource));
    assert!(policy.check(&viewer, Permission::ViewPipeline, &resource));
    assert!(!policy.check(&viewer, Permission::RunTrigger, &resource));
    assert!(!policy.check(&viewer, Permission::ManagePipeline, &resource));
}

#[test]
fn rbac_platform_admin_global_access() {
    let policy = RbacPolicy::new();
    let admin = Actor::user(UserId::new(), Role::PlatformAdmin);
    let resource = Resource::new(ResourceType::Organization);

    assert!(policy.check(&admin, Permission::CreateOrganization, &resource));
    assert!(policy.check(&admin, Permission::DeleteOrganization, &resource));
    assert!(policy.check(&admin, Permission::ViewRuns, &resource));
}

#[test]
fn rbac_cross_org_denied() {
    let policy = RbacPolicy::new();
    let org1 = OrganizationId::new();
    let org2 = OrganizationId::new();
    let project = ProjectId::new();

    let dev = Actor::user(UserId::new(), Role::Developer)
        .in_org(org1)
        .in_project(project);
    let resource_other_org = Resource::pipeline("pipe_1", project).in_org(org2);

    assert!(
        !policy.check(&dev, Permission::RunTrigger, &resource_other_org),
        "cross-org access must be denied"
    );
}

#[test]
fn rbac_explicit_grant_overrides_role() {
    let mut policy = RbacPolicy::new();
    let user_id = UserId::new();
    let actor_id = met_secrets::rbac::ActorId::User(user_id);

    policy.grant(&actor_id, Permission::ManageSecrets);

    let viewer = Actor::user(user_id, Role::Viewer);
    let resource = Resource::new(ResourceType::Secret);

    assert!(
        policy.check(&viewer, Permission::ManageSecrets, &resource),
        "explicit grant must override insufficient role"
    );
}

#[test]
fn rbac_explicit_denial_overrides_role() {
    let mut policy = RbacPolicy::new();
    let user_id = UserId::new();
    let actor_id = met_secrets::rbac::ActorId::User(user_id);

    policy.deny(&actor_id, Permission::RunTrigger);

    let dev = Actor::user(user_id, Role::Developer);
    let resource = Resource::new(ResourceType::Pipeline);

    assert!(
        !policy.check(&dev, Permission::RunTrigger, &resource),
        "explicit denial must override role"
    );
    assert!(
        policy.check(&dev, Permission::ViewRuns, &resource),
        "non-denied permissions must still work"
    );
}

#[test]
fn rbac_system_actor_has_full_access() {
    let policy = RbacPolicy::new();
    let system = Actor::system();
    let resource = Resource::new(ResourceType::Organization);

    assert!(policy.check(&system, Permission::CreateOrganization, &resource));
    assert!(policy.check(&system, Permission::DeleteOrganization, &resource));
}

#[test]
fn rbac_permissions_for_role_are_correct() {
    let viewer_perms = RbacPolicy::permissions_for_role(Role::Viewer);
    assert!(viewer_perms.contains(&Permission::ViewRuns));
    assert!(!viewer_perms.contains(&Permission::RunTrigger));

    let admin_perms = RbacPolicy::permissions_for_role(Role::PlatformAdmin);
    assert!(admin_perms.contains(&Permission::CreateOrganization));
    assert!(admin_perms.contains(&Permission::ViewRuns));
}

#[test]
fn rbac_role_from_str() {
    assert_eq!("developer".parse::<Role>().unwrap(), Role::Developer);
    assert_eq!("org_admin".parse::<Role>().unwrap(), Role::OrgAdmin);
    assert_eq!("platform_admin".parse::<Role>().unwrap(), Role::PlatformAdmin);
    assert!("nonexistent_role".parse::<Role>().is_err());
}

// ──────────────────────────────────────────────────
// Audit logging: event construction and tracing logger
// ──────────────────────────────────────────────────

#[tokio::test]
async fn audit_event_construction() {
    let event = AuditEvent::new(AuditAction::SecretAccess)
        .with_actor("user:test-user-123")
        .with_resource("secret", "sec_456")
        .success();

    assert_eq!(event.action, AuditAction::SecretAccess);
    assert_eq!(event.outcome, Outcome::Success);
    match &event.actor {
        AuditActor::User { id, .. } => assert_eq!(id, "test-user-123"),
        other => panic!("expected User actor, got {other:?}"),
    }
    let resource = event.resource.as_ref().unwrap();
    assert_eq!(resource.resource_type, "secret");
    assert_eq!(resource.resource_id, "sec_456");
}

#[tokio::test]
async fn audit_tracing_logger_does_not_panic() {
    let logger = TracingAuditLogger;
    let event = AuditEvent::new(AuditAction::Login)
        .with_actor("user:u1")
        .success();

    // Should complete without panicking
    logger.log(event).await.expect("tracing logger must not fail");
}

#[tokio::test]
async fn audit_failure_event() {
    let event = AuditEvent::new(AuditAction::SecretAccess)
        .with_actor("user:bad-actor")
        .with_resource("secret", "sec_789")
        .failure("access denied by RBAC");

    assert_eq!(event.outcome, Outcome::Failure);
    assert!(event.error.is_some());
    assert!(event.error.as_ref().unwrap().contains("access denied"));
}

// ──────────────────────────────────────────────────
// Blast radius tracking
// ──────────────────────────────────────────────────

#[tokio::test]
async fn blast_radius_track_and_flag() {
    let tracker = BlastRadiusTracker::new();

    tracker
        .track_binary("sha_aaa111", "node", Some("/usr/bin/node"))
        .await;
    tracker
        .track_binary("sha_aaa111", "node", Some("/usr/bin/node"))
        .await;
    tracker
        .track_binary("sha_bbb222", "python3", Some("/usr/bin/python3"))
        .await;

    let node = tracker.get_binary("sha_aaa111").await.unwrap();
    assert_eq!(node.run_count, 2);
    assert!(!node.flagged);

    let python = tracker.get_binary("sha_bbb222").await.unwrap();
    assert_eq!(python.run_count, 1);

    // Flag the node binary
    let flagged = tracker
        .flag_binary("sha_aaa111", "CVE-2025-0001", true)
        .await;
    assert!(flagged);

    let (is_flagged, blocked) = tracker.is_flagged("sha_aaa111").await.unwrap();
    assert!(is_flagged);
    assert!(blocked);

    let flagged_list = tracker.get_flagged_binaries().await;
    assert_eq!(flagged_list.len(), 1);
    assert_eq!(flagged_list[0].sha256, "sha_aaa111");
}

#[tokio::test]
async fn blast_radius_flag_unknown_binary() {
    let tracker = BlastRadiusTracker::new();
    let result = tracker.flag_binary("nonexistent", "test", false).await;
    assert!(!result, "flagging unknown binary must return false");
}

#[tokio::test]
async fn blast_radius_list_all_binaries() {
    let tracker = BlastRadiusTracker::new();
    tracker.track_binary("sha_1", "a", None).await;
    tracker.track_binary("sha_2", "b", None).await;
    tracker.track_binary("sha_3", "c", None).await;

    let all = tracker.list_binaries().await;
    assert_eq!(all.len(), 3);
}

// ──────────────────────────────────────────────────
// Syscall audit collector
// ──────────────────────────────────────────────────

#[tokio::test]
async fn syscall_audit_record_and_summary() {
    let collector = SyscallAuditCollector::new("agent-test".into());

    collector
        .record_execution("/usr/bin/git", vec!["git".into(), "status".into()], 1000, 1)
        .await;
    collector
        .record_execution("/usr/bin/make", vec!["make".into(), "build".into()], 1001, 1)
        .await;
    collector
        .record_execution("/usr/bin/git", vec!["git".into(), "push".into()], 1002, 1)
        .await;

    let execs = collector.get_executions().await;
    assert_eq!(execs.len(), 3);
    assert_eq!(execs[0].binary_path, "/usr/bin/git");
    assert_eq!(execs[1].binary_path, "/usr/bin/make");
    assert_eq!(execs[2].pid, 1002);

    let summary = collector.stop_monitoring().await;
    assert_eq!(summary.total_executions, 3);
}

#[tokio::test]
async fn syscall_audit_record_network() {
    let collector = SyscallAuditCollector::new("agent-net".into());

    collector
        .record_connection(NetworkConnection {
            src_ip: "10.0.0.1".into(),
            src_port: 54321,
            dst_ip: "93.184.216.34".into(),
            dst_port: 443,
            protocol: "tcp".into(),
            direction: "outbound".into(),
            pid: Some(2000),
            bytes_sent: 1024,
            bytes_received: 4096,
            connected_at: chrono::Utc::now(),
            disconnected_at: None,
        })
        .await;

    let conns = collector.get_connections().await;
    assert_eq!(conns.len(), 1);
    assert_eq!(conns[0].dst_port, 443);
}

#[tokio::test]
async fn syscall_audit_start_monitoring() {
    let collector = SyscallAuditCollector::new("agent-mon".into());
    let result = collector.start_monitoring().await;
    assert!(result.is_ok());
}

// ──────────────────────────────────────────────────
// Provider types roundtrip
// ──────────────────────────────────────────────────

#[test]
fn provider_type_roundtrip() {
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

#[test]
fn secret_value_redacted_debug() {
    let sv = SecretValue::new("super-secret-password");
    let debug = format!("{:?}", sv);
    assert!(debug.contains("[REDACTED]"));
    assert!(!debug.contains("super-secret-password"));
    assert_eq!(sv.expose_secret(), "super-secret-password");
}

// ──────────────────────────────────────────────────
// Combined flow: encrypt secret, mask log output
// ──────────────────────────────────────────────────

#[test]
fn combined_encrypt_then_mask() {
    let secret_value = "production-db-password-XyZ789!@#";
    let hmac_key = b"combined-test-hmac-key-32-bytes!";

    let recipient = StaticSecret::random_from_rng(OsRng);
    let recipient_pub = X25519PublicKey::from(&recipient);

    let envelope = HybridEncryption::encrypt(
        &recipient_pub.to_bytes(),
        secret_value.as_bytes(),
        hmac_key,
    )
    .unwrap();

    let decrypted =
        HybridDecryption::decrypt(&recipient.to_bytes(), &envelope, hmac_key).unwrap();
    let decrypted_str = std::str::from_utf8(&decrypted).unwrap();
    assert_eq!(decrypted_str, secret_value);

    let filter = SecretMaskingFilter::new();
    filter.add_secret(decrypted_str);

    let log_line = format!("Connecting to db with password {decrypted_str}");
    let masked = filter.mask(&log_line);
    assert!(
        !masked.contains(secret_value),
        "decrypted secret must be masked in logs"
    );
    assert!(masked.contains("***"));
}
