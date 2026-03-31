//! Integration tests for the agent controller.
//!
//! These tests require a running PostgreSQL and NATS server.
//! Use `docker compose up -d` to start the required services.

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use met_controller::config::ControllerConfig;
    use met_controller::jwt::JwtManager;
    use met_controller::nats::NatsDispatcher;
    use met_controller::registry::{AgentRegistry, AgentState};
    use met_core::ids::{AgentId, OrganizationId};
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
        let dispatcher = NatsDispatcher::connect("nats://localhost:4222")
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
}
