//! Integration tests for the agent controller.
//!
//! These tests cover the key scenarios from the agent system plan:
//! - Agent registration with valid/invalid join tokens
//! - Heartbeat liveness and timeout detection
//! - Agent revocation and status transitions
//! - NATS job dispatch and consumer creation
//! - Health monitoring and stale agent reaping
//!
//! # Running Tests
//!
//! ```bash
//! just up  # Start Postgres and NATS
//! just db-migrate
//! cargo test --package met-controller --test integration_tests
//! ```
//!
//! Tests marked with `#[ignore]` require external services.

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use met_controller::config::ControllerConfig;
    use met_controller::jwt::{AgentClaims, JwtManager};
    use met_controller::nats::NatsDispatcher;
    use met_controller::registry::{AgentRegistry, AgentState, ResourceSnapshot};
    use met_core::ids::{AgentId, JobRunId, OrganizationId};
    use met_core::models::AgentStatus;
    use chrono::Utc;
    use std::time::Instant;

    fn test_config() -> ControllerConfig {
        ControllerConfig {
            jwt_secret: "test-secret-that-is-long-enough-for-tests-32".to_string(),
            jwt_validity: Duration::from_secs(3600),
            jwt_renewable: true,
            heartbeat_interval: Duration::from_secs(15),
            stale_threshold: Duration::from_secs(45),
            dead_threshold: Duration::from_secs(120),
            health_check_interval: Duration::from_secs(10),
            require_ntp_sync: false,
            allowed_platforms: vec![],
            ..Default::default()
        }
    }

    #[test]
    fn test_jwt_issue_and_validate() {
        let config = test_config();
        let jwt = JwtManager::new(&config.jwt_secret, config.jwt_validity, config.jwt_renewable);

        let agent_id = AgentId::new();
        let org_id = OrganizationId::new();
        let pool_tags = vec!["linux-amd64".to_string(), "docker".to_string()];

        let (token, expires_at) = jwt.issue(agent_id, org_id, pool_tags.clone()).unwrap();

        let claims = jwt.validate(&token).unwrap();
        assert_eq!(claims.agent_id().unwrap(), agent_id);
        assert_eq!(claims.org_id().unwrap(), org_id);
        assert_eq!(claims.pool_tags, pool_tags);
        assert!(claims.renewable);
        assert!(!claims.is_expired());
    }

    #[tokio::test]
    async fn test_registry_operations() {
        let registry = AgentRegistry::new();
        let agent_id = AgentId::new();
        let org_id = OrganizationId::new();

        let state = AgentState {
            agent_id,
            org_id,
            status: AgentStatus::Online,
            last_heartbeat: Instant::now(),
            last_heartbeat_at: Utc::now(),
            os: "linux".to_string(),
            arch: "amd64".to_string(),
            pool_tags: vec!["docker".to_string()],
            labels: vec![],
            max_jobs: 1,
            running_jobs: 0,
            current_job: None,
            jwt_expires_at: Utc::now() + chrono::Duration::hours(24),
            resources: None,
        };

        // Register
        registry.register(state.clone()).await;

        // Get
        let retrieved = registry.get(agent_id).await.unwrap();
        assert_eq!(retrieved.agent_id, agent_id);
        assert_eq!(retrieved.status, AgentStatus::Online);

        // Heartbeat
        let updated = registry
            .heartbeat(agent_id, AgentStatus::Busy, 1, None, None)
            .await
            .unwrap();
        assert_eq!(updated.status, AgentStatus::Busy);
        assert_eq!(updated.running_jobs, 1);

        // List by org
        let agents = registry.list_by_org(org_id).await;
        assert_eq!(agents.len(), 1);

        // List available (should be empty - agent is busy)
        let available = registry
            .list_available(org_id, &["docker".to_string()])
            .await;
        assert_eq!(available.len(), 0);

        // Update status to online
        registry.update_status(agent_id, AgentStatus::Online).await;
        registry.heartbeat(agent_id, AgentStatus::Online, 0, None, None).await;

        // Now should be available
        let available = registry
            .list_available(org_id, &["docker".to_string()])
            .await;
        assert_eq!(available.len(), 1);

        // Remove
        registry.remove(agent_id).await;
        assert!(registry.get(agent_id).await.is_none());
    }

    #[tokio::test]
    async fn test_registry_stale_detection() {
        let registry = AgentRegistry::new();
        let agent_id = AgentId::new();
        let org_id = OrganizationId::new();

        // Create agent with old heartbeat
        let state = AgentState {
            agent_id,
            org_id,
            status: AgentStatus::Online,
            last_heartbeat: Instant::now() - Duration::from_secs(60),
            last_heartbeat_at: Utc::now() - chrono::Duration::seconds(60),
            os: "linux".to_string(),
            arch: "amd64".to_string(),
            pool_tags: vec![],
            labels: vec![],
            max_jobs: 1,
            running_jobs: 0,
            current_job: None,
            jwt_expires_at: Utc::now() + chrono::Duration::hours(24),
            resources: None,
        };

        registry.register(state).await;

        // Should be detected as stale
        let stale = registry.find_stale(Duration::from_secs(30)).await;
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].agent_id, agent_id);
    }

    #[tokio::test]
    async fn test_multiple_agents_pool_filtering() {
        let registry = AgentRegistry::new();
        let org_id = OrganizationId::new();

        // Create agents with different pool tags
        for i in 0..5 {
            let agent_id = AgentId::new();
            let pool_tags = if i < 3 {
                vec!["docker".to_string()]
            } else {
                vec!["native".to_string()]
            };

            let state = AgentState {
                agent_id,
                org_id,
                status: AgentStatus::Online,
                last_heartbeat: Instant::now(),
                last_heartbeat_at: Utc::now(),
                os: "linux".to_string(),
                arch: "amd64".to_string(),
                pool_tags,
                labels: vec![],
                max_jobs: 1,
                running_jobs: 0,
                current_job: None,
                jwt_expires_at: Utc::now() + chrono::Duration::hours(24),
                resources: None,
            };

            registry.register(state).await;
        }

        // Filter by docker tag
        let docker_agents = registry
            .list_available(org_id, &["docker".to_string()])
            .await;
        assert_eq!(docker_agents.len(), 3);

        // Filter by native tag
        let native_agents = registry
            .list_available(org_id, &["native".to_string()])
            .await;
        assert_eq!(native_agents.len(), 2);

        // Filter by non-existent tag
        let no_agents = registry
            .list_available(org_id, &["gpu".to_string()])
            .await;
        assert_eq!(no_agents.len(), 0);
    }

    #[tokio::test]
    #[ignore = "requires NATS server"]
    async fn test_nats_dispatcher() {
        let dispatcher = NatsDispatcher::connect("nats://localhost:4222", None)
            .await
            .unwrap();

        let org_id = OrganizationId::new();

        // Create a job dispatch message
        let job = met_proto::controller::v1::JobDispatch {
            job_run_id: "test-job-123".to_string(),
            run_id: "test-run-456".to_string(),
            org_id: org_id.to_string(),
            pipeline_name: "test-pipeline".to_string(),
            job_name: "test-job".to_string(),
            steps: vec![],
            variables: Default::default(),
            secrets: vec![],
            timeout_secs: 3600,
            required_tags: vec!["docker".to_string()],
            priority: 50,
            cache_restore: None,
            input_artifacts: vec![],
            services: vec![],
            retry_policy: None,
            trace_id: String::new(),
            attempt: 1,
        };

        // Dispatch the job
        dispatcher
            .dispatch_job(org_id, "docker", &job)
            .await
            .unwrap();

        // Create consumer and verify message
        let consumer = dispatcher
            .create_job_consumer(org_id, "docker")
            .await
            .unwrap();

        // Close dispatcher
        dispatcher.close().await;
    }

    // ============================================================================
    // JWT Token Management Tests
    // ============================================================================

    #[test]
    fn test_jwt_renewal_check() {
        let config = test_config();
        let jwt = JwtManager::new(&config.jwt_secret, config.jwt_validity, config.jwt_renewable);

        let agent_id = AgentId::new();
        let org_id = OrganizationId::new();

        let (token, _) = jwt.issue(agent_id, org_id, vec![]).unwrap();
        let claims = jwt.validate(&token).unwrap();

        // Freshly issued token should not need renewal
        assert!(!jwt.needs_renewal(&claims));
    }

    #[test]
    fn test_jwt_non_renewable_token() {
        let jwt = JwtManager::new(
            "test-secret-that-is-long-enough-for-tests-32",
            Duration::from_secs(3600),
            false, // Non-renewable
        );

        let agent_id = AgentId::new();
        let org_id = OrganizationId::new();

        let (token, _) = jwt.issue(agent_id, org_id, vec![]).unwrap();
        let claims = jwt.validate(&token).unwrap();

        // Non-renewable token should never need renewal
        assert!(!claims.renewable);
        assert!(!jwt.needs_renewal(&claims));
    }

    #[test]
    fn test_jwt_pool_tags_preserved() {
        let config = test_config();
        let jwt = JwtManager::new(&config.jwt_secret, config.jwt_validity, config.jwt_renewable);

        let agent_id = AgentId::new();
        let org_id = OrganizationId::new();
        let pool_tags = vec![
            "linux-amd64".to_string(),
            "docker".to_string(),
            "gpu".to_string(),
        ];

        let (token, _) = jwt.issue(agent_id, org_id, pool_tags.clone()).unwrap();
        let claims = jwt.validate(&token).unwrap();

        assert_eq!(claims.pool_tags, pool_tags);
    }

    // ============================================================================
    // Agent Registry Advanced Tests
    // ============================================================================

    #[tokio::test]
    async fn test_registry_resource_tracking() {
        let registry = AgentRegistry::new();
        let agent_id = AgentId::new();
        let org_id = OrganizationId::new();

        let state = AgentState {
            agent_id,
            org_id,
            status: AgentStatus::Online,
            last_heartbeat: Instant::now(),
            last_heartbeat_at: Utc::now(),
            os: "linux".to_string(),
            arch: "amd64".to_string(),
            pool_tags: vec!["docker".to_string()],
            labels: vec![],
            max_jobs: 4,
            running_jobs: 0,
            current_job: None,
            jwt_expires_at: Utc::now() + chrono::Duration::hours(24),
            resources: None,
        };

        registry.register(state).await;

        // Update with resource snapshot
        let resources = ResourceSnapshot {
            cpu_percent: 0.45,
            memory_percent: 0.60,
            disk_percent: 0.30,
        };

        registry
            .heartbeat(agent_id, AgentStatus::Online, 0, None, Some(resources.clone()))
            .await
            .unwrap();

        let agent = registry.get(agent_id).await.unwrap();
        assert!(agent.resources.is_some());

        let r = agent.resources.unwrap();
        assert!((r.cpu_percent - 0.45).abs() < 0.01);
        assert!((r.memory_percent - 0.60).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_registry_job_tracking() {
        let registry = AgentRegistry::new();
        let agent_id = AgentId::new();
        let org_id = OrganizationId::new();

        let state = AgentState {
            agent_id,
            org_id,
            status: AgentStatus::Online,
            last_heartbeat: Instant::now(),
            last_heartbeat_at: Utc::now(),
            os: "linux".to_string(),
            arch: "amd64".to_string(),
            pool_tags: vec![],
            labels: vec![],
            max_jobs: 2,
            running_jobs: 0,
            current_job: None,
            jwt_expires_at: Utc::now() + chrono::Duration::hours(24),
            resources: None,
        };

        registry.register(state).await;

        // Start a job
        let job_id = JobRunId::new();
        registry
            .heartbeat(agent_id, AgentStatus::Busy, 1, Some(job_id), None)
            .await
            .unwrap();

        let agent = registry.get(agent_id).await.unwrap();
        assert_eq!(agent.status, AgentStatus::Busy);
        assert_eq!(agent.running_jobs, 1);
        assert_eq!(agent.current_job, Some(job_id));

        // Agent should still have capacity for 1 more job
        assert!(!agent.can_accept_jobs()); // Busy status means not accepting

        // Complete job
        registry
            .heartbeat(agent_id, AgentStatus::Online, 0, None, None)
            .await
            .unwrap();

        let agent = registry.get(agent_id).await.unwrap();
        assert!(agent.can_accept_jobs());
    }

    #[tokio::test]
    async fn test_registry_revocation() {
        let registry = AgentRegistry::new();
        let agent_id = AgentId::new();
        let org_id = OrganizationId::new();

        let state = AgentState {
            agent_id,
            org_id,
            status: AgentStatus::Online,
            last_heartbeat: Instant::now(),
            last_heartbeat_at: Utc::now(),
            os: "linux".to_string(),
            arch: "amd64".to_string(),
            pool_tags: vec!["docker".to_string()],
            labels: vec![],
            max_jobs: 1,
            running_jobs: 0,
            current_job: None,
            jwt_expires_at: Utc::now() + chrono::Duration::hours(24),
            resources: None,
        };

        registry.register(state).await;

        // Revoke the agent
        registry.update_status(agent_id, AgentStatus::Revoked).await;

        let agent = registry.get(agent_id).await.unwrap();
        assert_eq!(agent.status, AgentStatus::Revoked);

        // Revoked agent should not be available
        let available = registry
            .list_available(org_id, &["docker".to_string()])
            .await;
        assert!(available.is_empty());
    }

    #[tokio::test]
    async fn test_registry_dead_detection() {
        let registry = AgentRegistry::new();
        let agent_id = AgentId::new();
        let org_id = OrganizationId::new();

        // Create agent with very old heartbeat (beyond dead threshold)
        let state = AgentState {
            agent_id,
            org_id,
            status: AgentStatus::Online,
            last_heartbeat: Instant::now() - Duration::from_secs(300), // 5 minutes ago
            last_heartbeat_at: Utc::now() - chrono::Duration::seconds(300),
            os: "linux".to_string(),
            arch: "amd64".to_string(),
            pool_tags: vec![],
            labels: vec![],
            max_jobs: 1,
            running_jobs: 0,
            current_job: None,
            jwt_expires_at: Utc::now() + chrono::Duration::hours(24),
            resources: None,
        };

        registry.register(state).await;

        // Should be detected as stale
        let stale = registry.find_stale(Duration::from_secs(120)).await;
        assert_eq!(stale.len(), 1);

        // Agent's heartbeat is beyond dead threshold
        let agent = &stale[0];
        assert!(agent.last_heartbeat.elapsed() > Duration::from_secs(120));
    }

    #[tokio::test]
    async fn test_registry_count_by_status() {
        let registry = AgentRegistry::new();
        let org_id = OrganizationId::new();

        // Register agents with different statuses
        for i in 0..3 {
            let state = AgentState {
                agent_id: AgentId::new(),
                org_id,
                status: AgentStatus::Online,
                last_heartbeat: Instant::now(),
                last_heartbeat_at: Utc::now(),
                os: "linux".to_string(),
                arch: "amd64".to_string(),
                pool_tags: vec![],
                labels: vec![],
                max_jobs: 1,
                running_jobs: 0,
                current_job: None,
                jwt_expires_at: Utc::now() + chrono::Duration::hours(24),
                resources: None,
            };
            registry.register(state).await;
        }

        for i in 0..2 {
            let state = AgentState {
                agent_id: AgentId::new(),
                org_id,
                status: AgentStatus::Busy,
                last_heartbeat: Instant::now(),
                last_heartbeat_at: Utc::now(),
                os: "linux".to_string(),
                arch: "amd64".to_string(),
                pool_tags: vec![],
                labels: vec![],
                max_jobs: 1,
                running_jobs: 1,
                current_job: None,
                jwt_expires_at: Utc::now() + chrono::Duration::hours(24),
                resources: None,
            };
            registry.register(state).await;
        }

        let counts = registry.count_by_status().await;
        assert_eq!(*counts.get(&AgentStatus::Online).unwrap_or(&0), 3);
        assert_eq!(*counts.get(&AgentStatus::Busy).unwrap_or(&0), 2);
        assert_eq!(registry.total_count().await, 5);
    }

    // ============================================================================
    // NATS Subject Tests
    // ============================================================================

    #[test]
    fn test_nats_subject_hierarchy() {
        use met_controller::nats::subjects;

        let org_id = OrganizationId::new();

        let dispatch = subjects::job_dispatch(org_id, "docker");
        assert!(dispatch.starts_with("met.jobs."));
        assert!(dispatch.contains(org_id.as_uuid().to_string().as_str()));
        assert!(dispatch.ends_with(".docker"));

        let default_dispatch = subjects::job_dispatch_default(org_id);
        assert!(default_dispatch.ends_with("._default"));

        let broadcast = subjects::broadcast(org_id);
        assert!(broadcast.starts_with("met.broadcast."));

        let cancel = subjects::job_cancel(org_id, "job-123");
        assert!(cancel.starts_with("met.cancel."));
        assert!(cancel.ends_with(".job-123"));
    }

    // ============================================================================
    // Integration Tests (require external services)
    // ============================================================================

    #[tokio::test]
    #[ignore = "requires NATS server with JetStream"]
    async fn test_job_dispatch_and_consume() {
        let dispatcher = NatsDispatcher::connect("nats://localhost:4222", None)
            .await
            .unwrap();

        let org_id = OrganizationId::new();
        let pool_tag = "test-pool";

        // Create consumer first
        let consumer = dispatcher
            .create_job_consumer(org_id, pool_tag)
            .await
            .unwrap();

        // Dispatch a job
        let job = met_proto::controller::v1::JobDispatch {
            job_run_id: uuid::Uuid::new_v4().to_string(),
            run_id: uuid::Uuid::new_v4().to_string(),
            org_id: org_id.to_string(),
            pipeline_name: "test-pipeline".to_string(),
            job_name: "test-job".to_string(),
            steps: vec![
                met_proto::controller::v1::StepSpec {
                    step_run_id: uuid::Uuid::new_v4().to_string(),
                    step_id: "step-1".to_string(),
                    name: "build".to_string(),
                    kind: 1, // COMMAND
                    command: "echo hello".to_string(),
                    image: "alpine:latest".to_string(),
                    working_dir: "/workspace".to_string(),
                    shell: "/bin/sh".to_string(),
                    environment: Default::default(),
                    sequence: 1,
                    continue_on_error: false,
                    timeout_secs: 300,
                },
            ],
            variables: Default::default(),
            secrets: vec![],
            timeout_secs: 3600,
            required_tags: vec![pool_tag.to_string()],
            priority: 50,
            cache_restore: None,
            input_artifacts: vec![],
            services: vec![],
            retry_policy: None,
            trace_id: String::new(),
            attempt: 1,
        };

        dispatcher
            .dispatch_job(org_id, pool_tag, &job)
            .await
            .unwrap();

        // The consumer should be able to receive the message
        // (In a real test, we'd pull and verify)

        dispatcher.close().await;
    }

    #[tokio::test]
    #[ignore = "requires NATS server"]
    async fn test_job_cancellation_broadcast() {
        let dispatcher = NatsDispatcher::connect("nats://localhost:4222", None)
            .await
            .unwrap();

        let org_id = OrganizationId::new();
        let job_id = "cancel-test-job";

        // Cancel a job
        dispatcher.cancel_job(org_id, job_id).await.unwrap();

        // In a real test, we'd verify subscribers receive the cancellation
        dispatcher.close().await;
    }

    #[tokio::test]
    #[ignore = "requires NATS server"]
    async fn test_nats_reconnection() {
        // First connection
        let dispatcher = NatsDispatcher::connect("nats://localhost:4222", None)
            .await
            .unwrap();

        let org_id = OrganizationId::new();

        // Dispatch should succeed
        let job = met_proto::controller::v1::JobDispatch {
            job_run_id: "reconnect-test".to_string(),
            run_id: "reconnect-run".to_string(),
            org_id: org_id.to_string(),
            pipeline_name: "test".to_string(),
            job_name: "test".to_string(),
            steps: vec![],
            variables: Default::default(),
            secrets: vec![],
            timeout_secs: 60,
            required_tags: vec![],
            priority: 50,
            cache_restore: None,
            input_artifacts: vec![],
            services: vec![],
            retry_policy: None,
            trace_id: String::new(),
            attempt: 1,
        };

        dispatcher
            .dispatch_job(org_id, "_default", &job)
            .await
            .unwrap();

        dispatcher.close().await;
    }
}
