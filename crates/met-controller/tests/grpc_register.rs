//! Agent registration against a real Postgres + NATS.
//!
//! ```text
//! docker compose up -d postgres nats
//! just db-migrate
//! NATS_URL=nats://127.0.0.1:4222 cargo test -p met-controller --test grpc_register -- --ignored
//! ```

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use met_controller::config::ControllerConfig;
use met_controller::grpc::AgentServiceImpl;
use met_controller::nats::NatsDispatcher;
use met_controller::registry::AgentRegistry;
use met_core::hash_join_token;
use met_core::ids::{JoinTokenId, UserId};
use met_core::models::{JoinToken, JoinTokenScope};
use met_proto::agent::v1::agent_service_server::AgentService;
use met_proto::agent::v1::{AgentCapabilities, RegisterRequest, SecurityBundle};
use met_store::repos::JoinTokenRepo;
use met_store::PgPool;
use tonic::Request;
use uuid::Uuid;

fn test_controller_config() -> ControllerConfig {
    let mut c = ControllerConfig::with_jwt_secret("a".repeat(40));
    c.require_ntp_sync = false;
    c.jwt_validity = Duration::from_secs(3600);
    c
}

async fn connect_nats() -> Option<NatsDispatcher> {
    let url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".to_string());
    NatsDispatcher::connect(&url, None).await.ok()
}

fn sample_register_request(join_token: impl Into<String>) -> RegisterRequest {
    RegisterRequest {
        join_token: join_token.into(),
        security_bundle: Some(SecurityBundle {
            hostname: "test-agent".to_string(),
            os: "linux".to_string(),
            arch: "amd64".to_string(),
            kernel_version: String::new(),
            public_ips: vec![],
            private_ips: vec![],
            ntp_synchronized: true,
            container_runtime: String::new(),
            container_runtime_version: String::new(),
            environment_type: met_proto::agent::v1::EnvironmentType::Virtual.into(),
            agent_x509_public_key: vec![],
            machine_id: String::new(),
            logical_cpus: 0,
            memory_total_bytes: 0,
            egress_public_ip: String::new(),
        }),
        capabilities: Some(AgentCapabilities {
            os: "linux".to_string(),
            arch: "amd64".to_string(),
            labels: vec!["from-agent".to_string()],
            pool_tags: vec!["docker".to_string()],
        }),
    }
}

async fn seed_org_and_user(pool: &PgPool) -> (Uuid, Uuid) {
    let org_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO organizations (id, name, slug, created_at, updated_at) VALUES ($1, 'test-org', $2, NOW(), NOW())",
    )
    .bind(org_id)
    .bind(format!("slug-{}", org_id.as_simple()))
    .execute(pool)
    .await
    .expect("insert org");

    let user_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, org_id, username, email, created_at, updated_at) VALUES ($1, $2, 'u', $3, NOW(), NOW())",
    )
    .bind(user_id)
    .bind(org_id)
    .bind(format!("u-{}@test.dev", user_id.as_simple()))
    .execute(pool)
    .await
    .expect("insert user");

    (org_id, user_id)
}

/// Run with: same as module doc (`--ignored` plus Postgres + NATS).
#[sqlx::test(migrations = "../met-store/migrations")]
#[ignore = "requires Postgres (DATABASE_URL) and NATS on NATS_URL"]
async fn register_rejects_unknown_join_token(pool: PgPool) {
    let Some(nats) = connect_nats().await else {
        eprintln!("NATS unavailable; start with: docker compose up -d nats");
        return;
    };
    let config = test_controller_config();
    let registry = AgentRegistry::new();
    let impl_ = AgentServiceImpl::new(
        config,
        Arc::new(pool),
        registry,
        nats,
        None,
    );

    let err = AgentService::register(
        &impl_,
        Request::new(sample_register_request("met_join_nonexistent_token_value")),
    )
    .await
    .expect_err("unknown token should fail");

    assert_eq!(err.code(), tonic::Code::Unauthenticated);
}

/// Run with: same as module doc (`--ignored` plus Postgres + NATS).
#[sqlx::test(migrations = "../met-store/migrations")]
#[ignore = "requires Postgres (DATABASE_URL) and NATS on NATS_URL"]
async fn register_succeeds_with_valid_tenant_token(pool: PgPool) {
    let Some(nats) = connect_nats().await else {
        eprintln!("NATS unavailable; start with: docker compose up -d nats");
        return;
    };

    let (org_id, user_id) = seed_org_and_user(&pool).await;

    let plain = format!("met_join_{}", Uuid::new_v4().simple());
    let token_hash = hash_join_token(&plain);
    let now = Utc::now();
    let token = JoinToken {
        id: JoinTokenId::new(),
        token_hash,
        scope: JoinTokenScope::Tenant,
        scope_id: Some(org_id),
        description: "test token".to_string(),
        org_id: Some(met_core::ids::OrganizationId::from_uuid(org_id)),
        max_uses: 1,
        current_uses: 0,
        labels: vec!["tok-label".to_string()],
        pool_tags: vec!["from-token".to_string()],
        expires_at: None,
        revoked: false,
        created_by: UserId::from_uuid(user_id),
        created_at: now,
        updated_at: now,
        consumed_by_agent_id: None,
        consumed_at: None,
    };
    JoinTokenRepo::new(&pool)
        .create(&token)
        .await
        .expect("create join token");

    let config = test_controller_config();
    let registry = AgentRegistry::new();
    let impl_ = AgentServiceImpl::new(
        config,
        Arc::new(pool),
        registry,
        nats,
        None,
    );

    let resp = AgentService::register(&impl_, Request::new(sample_register_request(&plain)))
        .await
        .expect("register should succeed")
        .into_inner();

    assert!(!resp.agent_id.is_empty());
    assert!(!resp.jwt_token.is_empty());
    assert!(
        resp.nats_subjects
            .iter()
            .any(|s| s.contains(&org_id.to_string())),
        "subjects should include org id, got {:?}",
        resp.nats_subjects
    );
    let _ = resp.agent_id.parse::<met_core::ids::AgentId>().expect("agent id");
}
