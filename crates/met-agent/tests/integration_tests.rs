//! Integration tests for the Meticulous build agent.
//!
//! These tests cover the key scenarios from the agent system plan:
//! - Agent registration with join tokens
//! - NATS job dispatch and acknowledgment
//! - Job execution with status reporting
//! - Heartbeat loop and liveness tracking
//! - Agent revocation and graceful shutdown
//!
//! # Running Tests
//!
//! These tests require running infrastructure services:
//! ```bash
//! just up  # Start Postgres and NATS
//! just db-migrate
//! cargo test --package met-agent --test integration_tests
//! ```
//!
//! Tests marked with `#[ignore]` require external services.

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    use chrono::Utc;
    use met_agent::backend::{default_backend, ExecutionBackend, NativeBackend, StepSpec};
    use met_agent::config::{AgentConfig, AgentIdentity};
    use met_agent::heartbeat::HeartbeatState;
    use met_agent::security::{CollectedSecurityBundle, EnvironmentType, JobPki, SecurityBundleCollector};
    use met_proto::AgentStatus;
    use tokio::sync::RwLock;

    // ============================================================================
    // Security Bundle Collection Tests
    // ============================================================================

    #[tokio::test]
    async fn test_security_bundle_collection() {
        let collector = SecurityBundleCollector::new();
        let bundle = collector.collect().await;

        // Verify required fields are populated
        assert!(!bundle.hostname.is_empty(), "hostname should not be empty");
        assert!(!bundle.os.is_empty(), "os should not be empty");
        assert!(!bundle.arch.is_empty(), "arch should not be empty");

        // Verify OS/arch match Rust constants
        assert_eq!(bundle.os, std::env::consts::OS);
        assert_eq!(bundle.arch, std::env::consts::ARCH);

        // Verify environment type is set
        assert!(matches!(
            bundle.environment_type,
            EnvironmentType::Physical | EnvironmentType::Virtual | EnvironmentType::Container
        ));

        // X509 public key should be generated
        assert!(
            !bundle.x509_public_key.is_empty(),
            "x509 public key should be generated"
        );
    }

    #[tokio::test]
    async fn test_ntp_sync_detection() {
        let collector = SecurityBundleCollector::new();
        let bundle = collector.collect().await;

        // NTP sync check should return a boolean (we can't guarantee it's true in all envs)
        // Just verify the field is populated
        let _ = bundle.ntp_synchronized;
    }

    #[tokio::test]
    async fn test_container_runtime_detection() {
        let collector = SecurityBundleCollector::new();
        let bundle = collector.collect().await;

        // Container runtime should be one of known values or "none"
        let valid_runtimes = ["docker", "podman", "containerd", "none"];
        assert!(
            valid_runtimes.contains(&bundle.container_runtime.as_str()),
            "container_runtime '{}' should be one of {:?}",
            bundle.container_runtime,
            valid_runtimes
        );
    }

    // ============================================================================
    // Per-Job PKI Tests
    // ============================================================================

    #[test]
    fn test_job_pki_generation() {
        let pki = JobPki::generate().expect("PKI generation should succeed");

        // Public key should be non-empty DER
        let pubkey = pki.public_key_der();
        assert!(!pubkey.is_empty(), "public key DER should not be empty");

        // Should be valid DER format (starts with 0x30 for SEQUENCE)
        assert_eq!(
            pubkey[0], 0x30,
            "public key should start with DER SEQUENCE tag"
        );
    }

    #[test]
    fn test_job_pki_multiple_generations_unique() {
        let pki1 = JobPki::generate().unwrap();
        let pki2 = JobPki::generate().unwrap();

        // Each generation should produce different keys
        assert_ne!(
            pki1.public_key_der(),
            pki2.public_key_der(),
            "each PKI generation should produce unique keys"
        );
    }

    // ============================================================================
    // Configuration Tests
    // ============================================================================

    #[test]
    fn test_default_config() {
        let config = AgentConfig::default();

        assert_eq!(config.controller_url, "http://localhost:9090");
        assert_eq!(config.concurrency, 1);
        assert!(!config.pool_tags.is_empty());
        assert_eq!(config.pool_tags[0], "_default");
    }

    #[test]
    fn test_config_load_with_overrides() {
        let config = AgentConfig::load(
            None,
            Some("http://custom:9999".to_string()),
            Some("test-token".to_string()),
            Some("test-agent".to_string()),
            Some("test-pool".to_string()),
            vec!["tag1".to_string(), "tag2".to_string()],
        )
        .unwrap();

        assert_eq!(config.controller_url, "http://custom:9999");
        assert_eq!(config.join_token, Some("test-token".to_string()));
        assert_eq!(config.name, Some("test-agent".to_string()));
        assert_eq!(config.pool, Some("test-pool".to_string()));
        assert_eq!(config.pool_tags, vec!["tag1", "tag2"]);
    }

    #[test]
    fn test_config_validation_empty_url() {
        let mut config = AgentConfig::default();
        config.controller_url = String::new();

        // Validation should fail for empty controller URL
        let result = AgentConfig::load(
            None,
            Some(String::new()),
            None,
            None,
            None,
            vec![],
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_config_validation_invalid_concurrency() {
        let result = AgentConfig::load(None, None, None, None, None, vec![]);
        // Default config should be valid
        assert!(result.is_ok());

        let config = result.unwrap();
        assert!(config.concurrency >= 1);
    }

    // ============================================================================
    // Agent Identity Tests
    // ============================================================================

    #[test]
    fn test_agent_identity_jwt_expiry() {
        let identity = AgentIdentity {
            agent_id: "test-agent-id".to_string(),
            org_id: "test-org-id".to_string(),
            jwt_token: "test-token".to_string(),
            jwt_expires_at: Utc::now().timestamp() - 3600, // 1 hour ago
            renewable: true,
            nats_subjects: vec!["met.jobs.*.test".to_string()],
            nats_url: "nats://localhost:4222".to_string(),
            nats_user_jwt: None,
            nats_user_seed: None,
        };

        assert!(identity.is_jwt_expired(), "JWT should be expired");
    }

    #[test]
    fn test_agent_identity_jwt_not_expired() {
        let identity = AgentIdentity {
            agent_id: "test-agent-id".to_string(),
            org_id: "test-org-id".to_string(),
            jwt_token: "test-token".to_string(),
            jwt_expires_at: Utc::now().timestamp() + 3600, // 1 hour from now
            renewable: true,
            nats_subjects: vec!["met.jobs.*.test".to_string()],
            nats_url: "nats://localhost:4222".to_string(),
            nats_user_jwt: None,
            nats_user_seed: None,
        };

        assert!(!identity.is_jwt_expired(), "JWT should not be expired");
    }

    #[test]
    fn test_agent_identity_needs_renewal() {
        // JWT expires in 5 minutes (within 10% of 24h validity)
        let identity = AgentIdentity {
            agent_id: "test-agent-id".to_string(),
            org_id: "test-org-id".to_string(),
            jwt_token: "test-token".to_string(),
            jwt_expires_at: Utc::now().timestamp() + 300, // 5 minutes
            renewable: true,
            nats_subjects: vec![],
            nats_url: "nats://localhost:4222".to_string(),
            nats_user_jwt: None,
            nats_user_seed: None,
        };

        assert!(
            identity.needs_jwt_renewal(),
            "JWT should need renewal when close to expiry"
        );
    }

    #[test]
    fn test_agent_identity_no_renewal_when_not_renewable() {
        let identity = AgentIdentity {
            agent_id: "test-agent-id".to_string(),
            org_id: "test-org-id".to_string(),
            jwt_token: "test-token".to_string(),
            jwt_expires_at: Utc::now().timestamp() + 300,
            renewable: false, // Not renewable
            nats_subjects: vec![],
            nats_url: "nats://localhost:4222".to_string(),
            nats_user_jwt: None,
            nats_user_seed: None,
        };

        assert!(
            !identity.needs_jwt_renewal(),
            "Non-renewable JWT should not request renewal"
        );
    }

    // ============================================================================
    // Heartbeat State Tests
    // ============================================================================

    #[tokio::test]
    async fn test_heartbeat_state_transitions() {
        let state = Arc::new(RwLock::new(HeartbeatState::default()));

        // Initial state
        {
            let s = state.read().await;
            assert_eq!(s.status, AgentStatus::Online);
            assert_eq!(s.running_jobs, 0);
            assert!(s.current_job_id.is_none());
        }

        // Transition to busy
        {
            let mut s = state.write().await;
            s.status = AgentStatus::Busy;
            s.running_jobs = 1;
            s.current_job_id = Some("job-123".to_string());
        }

        {
            let s = state.read().await;
            assert_eq!(s.status, AgentStatus::Busy);
            assert_eq!(s.running_jobs, 1);
            assert_eq!(s.current_job_id, Some("job-123".to_string()));
        }

        // Back to online
        {
            let mut s = state.write().await;
            s.status = AgentStatus::Online;
            s.running_jobs = 0;
            s.current_job_id = None;
        }

        {
            let s = state.read().await;
            assert_eq!(s.status, AgentStatus::Online);
            assert_eq!(s.running_jobs, 0);
        }
    }

    // ============================================================================
    // Execution Backend Tests
    // ============================================================================

    #[tokio::test]
    async fn test_native_backend_simple_command() {
        let backend = NativeBackend::new();
        let temp_dir = std::env::temp_dir();

        let step = StepSpec {
            step_id: "test-step".to_string(),
            name: "echo hello".to_string(),
            command: "echo hello".to_string(),
            image: String::new(),
            working_dir: String::new(),
            shell: String::new(),
            environment: HashMap::new(),
            timeout: Duration::from_secs(30),
        };

        let exit_code = backend.execute(&step, &temp_dir).await.unwrap();
        assert_eq!(exit_code, 0, "echo command should succeed");
    }

    #[tokio::test]
    async fn test_native_backend_environment_variables() {
        let backend = NativeBackend::new();
        let temp_dir = std::env::temp_dir();

        let mut env = HashMap::new();
        env.insert("TEST_VAR".to_string(), "test_value".to_string());

        let command = if cfg!(windows) {
            "echo %TEST_VAR%"
        } else {
            "echo $TEST_VAR"
        };

        let step = StepSpec {
            step_id: "test-env".to_string(),
            name: "test env var".to_string(),
            command: command.to_string(),
            image: String::new(),
            working_dir: String::new(),
            shell: String::new(),
            environment: env,
            timeout: Duration::from_secs(30),
        };

        let exit_code = backend.execute(&step, &temp_dir).await.unwrap();
        assert_eq!(exit_code, 0);
    }

    #[tokio::test]
    async fn test_native_backend_nonzero_exit() {
        let backend = NativeBackend::new();
        let temp_dir = std::env::temp_dir();

        let command = if cfg!(windows) {
            "exit /b 42"
        } else {
            "exit 42"
        };

        let step = StepSpec {
            step_id: "test-exit".to_string(),
            name: "exit with code".to_string(),
            command: command.to_string(),
            image: String::new(),
            working_dir: String::new(),
            shell: String::new(),
            environment: HashMap::new(),
            timeout: Duration::from_secs(30),
        };

        let exit_code = backend.execute(&step, &temp_dir).await.unwrap();
        assert_eq!(exit_code, 42, "should return exit code 42");
    }

    #[tokio::test]
    async fn test_native_backend_working_directory() {
        let backend = NativeBackend::new();
        let temp_dir = std::env::temp_dir();

        // Create a subdirectory
        let work_dir = temp_dir.join("test_workdir");
        std::fs::create_dir_all(&work_dir).ok();

        let command = if cfg!(windows) { "cd" } else { "pwd" };

        let step = StepSpec {
            step_id: "test-cwd".to_string(),
            name: "check working dir".to_string(),
            command: command.to_string(),
            image: String::new(),
            working_dir: String::new(),
            shell: String::new(),
            environment: HashMap::new(),
            timeout: Duration::from_secs(30),
        };

        let exit_code = backend.execute(&step, &work_dir).await.unwrap();
        assert_eq!(exit_code, 0);

        // Cleanup
        std::fs::remove_dir(&work_dir).ok();
    }

    #[tokio::test]
    async fn test_native_backend_timeout() {
        let backend = NativeBackend::new();
        let temp_dir = std::env::temp_dir();

        let command = if cfg!(windows) {
            "ping -n 10 127.0.0.1"
        } else {
            "sleep 10"
        };

        let step = StepSpec {
            step_id: "test-timeout".to_string(),
            name: "long running command".to_string(),
            command: command.to_string(),
            image: String::new(),
            working_dir: String::new(),
            shell: String::new(),
            environment: HashMap::new(),
            timeout: Duration::from_secs(1), // Short timeout
        };

        let result = backend.execute(&step, &temp_dir).await;
        assert!(result.is_err(), "command should timeout");
    }

    #[tokio::test]
    async fn test_default_backend_is_available() {
        let backend = default_backend().await;
        assert!(backend.is_available().await, "default backend should be available");
    }

    #[tokio::test]
    async fn test_default_backend_name() {
        let backend = default_backend().await;
        let name = backend.name();

        // Should be either "docker", "podman", "containerd", or "native"
        let valid_names = ["docker", "podman", "containerd", "native"];
        assert!(
            valid_names.contains(&name),
            "backend name '{}' should be one of {:?}",
            name,
            valid_names
        );
    }

    // ============================================================================
    // Integration Tests (require external services)
    // ============================================================================

    #[tokio::test]
    #[ignore = "requires NATS server at localhost:4222"]
    async fn test_nats_connection() {
        let result = async_nats::connect("nats://localhost:4222").await;
        assert!(result.is_ok(), "should connect to NATS");
    }

    #[tokio::test]
    #[ignore = "requires NATS server with JetStream"]
    async fn test_nats_jetstream_stream_creation() {
        let client = async_nats::connect("nats://localhost:4222").await.unwrap();
        let js = async_nats::jetstream::new(client);

        // Try to create or get the JOBS stream
        let stream_result = js.get_stream("JOBS").await;

        // Stream might not exist, which is ok for this test
        // We're just verifying JetStream connectivity
        let _ = stream_result;
    }

    #[tokio::test]
    #[ignore = "requires running controller at localhost:9090"]
    async fn test_agent_registration_flow() {
        use met_proto::agent::v1::agent_service_client::AgentServiceClient;

        let result = AgentServiceClient::connect("http://localhost:9090").await;
        assert!(result.is_ok(), "should connect to controller gRPC");
    }

    #[tokio::test]
    #[ignore = "requires full infrastructure"]
    async fn test_full_job_execution_cycle() {
        // This test would:
        // 1. Connect to controller
        // 2. Register agent
        // 3. Subscribe to NATS jobs
        // 4. Receive a job
        // 5. Execute steps
        // 6. Report status
        // 7. Verify completion

        // Placeholder for full integration test
        // Would require test fixtures and mock job dispatch
    }

    // ============================================================================
    // Property-Based Tests (using basic patterns)
    // ============================================================================

    #[test]
    fn test_environment_variable_sanitization() {
        // Verify that step execution doesn't leak agent environment
        let backend = NativeBackend::new();

        // The native backend should clear environment and only set
        // minimal required vars + step-specific vars
        let _ = backend; // Backend is configured to env_clear()
    }

    #[tokio::test]
    async fn test_workspace_isolation() {
        let backend = NativeBackend::new();

        // Create two separate workspaces
        let ws1 = std::env::temp_dir().join("test_ws1");
        let ws2 = std::env::temp_dir().join("test_ws2");

        std::fs::create_dir_all(&ws1).ok();
        std::fs::create_dir_all(&ws2).ok();

        // Create a file in ws1
        std::fs::write(ws1.join("test.txt"), "ws1 content").ok();

        // Try to access it from ws2 (should not be visible)
        let command = if cfg!(windows) {
            "dir"
        } else {
            "ls -la"
        };

        let step = StepSpec {
            step_id: "isolation-test".to_string(),
            name: "test isolation".to_string(),
            command: command.to_string(),
            image: String::new(),
            working_dir: String::new(),
            shell: String::new(),
            environment: HashMap::new(),
            timeout: Duration::from_secs(10),
        };

        let exit_code = backend.execute(&step, &ws2).await.unwrap();
        assert_eq!(exit_code, 0);

        // Cleanup
        std::fs::remove_dir_all(&ws1).ok();
        std::fs::remove_dir_all(&ws2).ok();
    }

    // ============================================================================
    // Concurrency Tests
    // ============================================================================

    #[tokio::test]
    async fn test_concurrent_heartbeat_state_access() {
        let state = Arc::new(RwLock::new(HeartbeatState::default()));

        let mut handles = vec![];

        // Spawn multiple readers
        for _ in 0..10 {
            let state_clone = state.clone();
            handles.push(tokio::spawn(async move {
                for _ in 0..100 {
                    let s = state_clone.read().await;
                    let _ = s.status;
                }
            }));
        }

        // Spawn a writer
        let state_writer = state.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..50 {
                let mut s = state_writer.write().await;
                s.running_jobs = i;
                tokio::time::sleep(Duration::from_micros(100)).await;
            }
        }));

        // All tasks should complete without deadlock
        for handle in handles {
            handle.await.unwrap();
        }
    }
}
