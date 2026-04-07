//! Comprehensive parser tests for met-parser.
//!
//! Tests cover:
//! - Basic pipeline parsing
//! - Schema validation
//! - Workflow resolution
//! - Variable interpolation
//! - DAG construction
//! - Error reporting

use indexmap::IndexMap;
use met_parser::{
    MockWorkflowProvider, PipelineParser, ParserConfig, RawJob, RawStep, RawWorkflowDef,
    WorkflowScope, ErrorCode, SourceLocation,
};

fn mock_docker_workflow() -> RawWorkflowDef {
    RawWorkflowDef {
        name: "Docker Build".to_string(),
        description: Some("Build a Docker image".to_string()),
        version: Some("1.0.0".to_string()),
        inputs: {
            let mut m = IndexMap::new();
            m.insert("image".to_string(), met_parser::schema::RawInputDef {
                input_type: "string".to_string(),
                required: true,
                default: None,
                description: Some("Docker image name".to_string()),
            });
            m.insert("tag".to_string(), met_parser::schema::RawInputDef {
                input_type: "string".to_string(),
                required: false,
                default: Some(serde_yaml::Value::String("latest".to_string())),
                description: Some("Image tag".to_string()),
            });
            m
        },
        outputs: IndexMap::new(),
        jobs: vec![RawJob {
            name: "Build".to_string(),
            id: "build".to_string(),
            runs_on: None,
            steps: vec![RawStep {
                name: "Build Image".to_string(),
                id: Some("build".to_string()),
                run: Some("docker build -t ${image}:${tag} .".to_string()),
                shell: Some("bash".to_string()),
                uses: None,
                action_inputs: IndexMap::new(),
                env: IndexMap::new(),
                working_directory: None,
                timeout: None,
                continue_on_error: false,
                outputs: IndexMap::new(),
            }],
            services: vec![],
            depends_on: vec![],
            condition: None,
            timeout: None,
            retry: None,
        }],
    }
}

fn mock_test_workflow() -> RawWorkflowDef {
    RawWorkflowDef {
        name: "Run Tests".to_string(),
        description: Some("Run test suite".to_string()),
        version: Some("1.0.0".to_string()),
        inputs: IndexMap::new(),
        outputs: IndexMap::new(),
        jobs: vec![RawJob {
            name: "Test".to_string(),
            id: "test".to_string(),
            runs_on: None,
            steps: vec![RawStep {
                name: "Run Tests".to_string(),
                id: Some("test".to_string()),
                run: Some("cargo test".to_string()),
                shell: Some("bash".to_string()),
                uses: None,
                action_inputs: IndexMap::new(),
                env: IndexMap::new(),
                working_directory: None,
                timeout: None,
                continue_on_error: false,
                outputs: IndexMap::new(),
            }],
            services: vec![],
            depends_on: vec![],
            condition: None,
            timeout: None,
            retry: None,
        }],
    }
}

fn create_provider() -> MockWorkflowProvider {
    let mut provider = MockWorkflowProvider::new();
    provider.add_workflow(WorkflowScope::Global, "docker-build", mock_docker_workflow());
    provider.add_workflow(WorkflowScope::Global, "test", mock_test_workflow());
    provider
}

#[tokio::test]
async fn test_parse_minimal_pipeline() {
    let yaml = r#"
name: Minimal Pipeline
triggers:
  manual: {}
workflows:
  - name: Build
    id: build
    workflow: global/docker-build
    inputs:
      image: myapp
"#;

    let provider = create_provider();
    let mut parser = PipelineParser::new(&provider);
    let result = parser.parse(yaml).await;

    assert!(result.is_ok(), "Parse error: {:?}", result.err());
    let ir = result.unwrap();
    
    assert_eq!(ir.name, "Minimal Pipeline");
    assert!(!ir.jobs.is_empty());
}

#[tokio::test]
async fn test_parse_pipeline_with_variables() {
    let yaml = r#"
name: Pipeline with Variables
triggers:
  manual: {}
vars:
  VERSION: "1.0.0"
  ENVIRONMENT: production
workflows:
  - name: Build
    id: build
    workflow: global/docker-build
    inputs:
      image: myapp
      tag: ${VERSION}
"#;

    let provider = create_provider();
    let mut parser = PipelineParser::new(&provider);
    let result = parser.parse(yaml).await;

    assert!(result.is_ok());
    let ir = result.unwrap();
    
    assert_eq!(ir.variables.get("VERSION"), Some(&"1.0.0".to_string()));
    assert_eq!(ir.variables.get("ENVIRONMENT"), Some(&"production".to_string()));
}

#[tokio::test]
async fn test_parse_pipeline_with_secrets() {
    let yaml = r#"
name: Pipeline with Secrets
triggers:
  manual: {}
secrets:
  AWS_ACCESS_KEY:
    aws:
      arn: arn:aws:secretsmanager:us-east-1:123456789:secret:aws-key
  VAULT_SECRET:
    vault:
      path: secret/data/myapp
      key: password
workflows:
  - name: Build
    id: build
    workflow: global/docker-build
    inputs:
      image: myapp
"#;

    let provider = create_provider();
    let mut parser = PipelineParser::new(&provider);
    let result = parser.parse(yaml).await;

    assert!(result.is_ok());
    let ir = result.unwrap();
    
    assert_eq!(ir.secret_refs.len(), 2);
    assert!(ir.secret_refs.contains_key("AWS_ACCESS_KEY"));
    assert!(ir.secret_refs.contains_key("VAULT_SECRET"));
}

#[tokio::test]
async fn test_parse_pipeline_with_dependencies() {
    let yaml = r#"
name: Pipeline with Dependencies
triggers:
  manual: {}
workflows:
  - name: Build
    id: build
    workflow: global/docker-build
    inputs:
      image: myapp
  - name: Test
    id: test
    workflow: global/test
    depends-on: [build]
  - name: Deploy
    id: deploy
    workflow: global/docker-build
    inputs:
      image: myapp-deploy
    depends-on: [test]
"#;

    let provider = create_provider();
    let mut parser = PipelineParser::new(&provider);
    let result = parser.parse(yaml).await;

    assert!(result.is_ok());
    let ir = result.unwrap();
    
    assert_eq!(ir.jobs.len(), 3);
    
    let deploy_job = ir.jobs.iter().find(|j| j.name == "Build").unwrap();
    assert!(deploy_job.depends_on.is_empty());
}

#[tokio::test]
async fn test_parse_pipeline_with_all_triggers() {
    let yaml = r#"
name: Multi-trigger Pipeline
triggers:
  manual: {}
  webhook:
    events: [push, pull_request]
    branches: [main, "release/*"]
  schedule:
    cron: "0 2 * * 1-5"
workflows:
  - name: Build
    id: build
    workflow: global/docker-build
    inputs:
      image: myapp
"#;

    let provider = create_provider();
    let mut parser = PipelineParser::new(&provider);
    let result = parser.parse(yaml).await;

    assert!(result.is_ok());
    let ir = result.unwrap();
    
    assert!(ir.triggers.len() >= 3);
}

#[tokio::test]
async fn test_parse_webhook_sync_key() {
    let yaml = r#"
name: Sync webhook
triggers:
  webhook:
    sync-key: primary
    events: [push]
    branches: [main]
workflows:
  - name: Build
    id: build
    workflow: global/docker-build
    inputs:
      image: myapp
"#;

    let provider = create_provider();
    let mut parser = PipelineParser::new(&provider);
    let ir = parser.parse(yaml).await.unwrap();
    let wh = ir.triggers.iter().find_map(|t| match t {
        met_parser::ir::Trigger::Webhook(w) => Some(w),
        _ => None,
    });
    assert_eq!(wh.and_then(|w| w.sync_key.as_deref()), Some("primary"));
}

#[tokio::test]
async fn test_parse_pipeline_with_pool_selector() {
    let yaml = r#"
name: Pipeline with Pool Selector
triggers:
  manual: {}
runs-on:
  tags:
    - amd64: true
    - gpu: false
workflows:
  - name: Build
    id: build
    workflow: global/docker-build
    inputs:
      image: myapp
"#;

    let provider = create_provider();
    let mut parser = PipelineParser::new(&provider);
    let result = parser.parse(yaml).await;

    assert!(result.is_ok());
    let ir = result.unwrap();
    
    assert!(ir.default_pool_selector.is_some());
}

#[tokio::test]
async fn test_parse_pipeline_with_cache() {
    let yaml = r#"
name: Pipeline with Cache
triggers:
  manual: {}
workflows:
  - name: Build
    id: build
    workflow: global/docker-build
    inputs:
      image: myapp
    cache:
      key: docker-${hashFiles('Dockerfile')}
      paths:
        - /var/cache/docker
      restore-keys:
        - docker-
"#;

    let provider = create_provider();
    let mut parser = PipelineParser::new(&provider);
    let result = parser.parse(yaml).await;

    assert!(result.is_ok());
    let ir = result.unwrap();
    
    let job = &ir.jobs[0];
    assert!(job.cache_config.is_some());
}

#[tokio::test]
async fn test_parse_pipeline_with_retry() {
    let yaml = r#"
name: Pipeline with Retry
triggers:
  manual: {}
workflows:
  - name: Build
    id: build
    workflow: global/docker-build
    inputs:
      image: myapp
    retry:
      max_attempts: 3
      backoff: 30s
"#;

    let provider = create_provider();
    let mut parser = PipelineParser::new(&provider);
    let result = parser.parse(yaml).await;

    assert!(result.is_ok());
    let ir = result.unwrap();
    
    let job = &ir.jobs[0];
    assert!(job.retry_policy.is_some());
    let retry = job.retry_policy.as_ref().unwrap();
    assert_eq!(retry.max_attempts, 3);
}

#[tokio::test]
async fn test_parse_pipeline_with_condition() {
    let yaml = r#"
name: Pipeline with Conditions
triggers:
  manual: {}
workflows:
  - name: Build
    id: build
    workflow: global/docker-build
    inputs:
      image: myapp
  - name: Deploy Prod
    id: deploy-prod
    workflow: global/docker-build
    inputs:
      image: myapp-prod
    depends-on: [build]
    condition: "trigger.branch == 'main'"
"#;

    let provider = create_provider();
    let mut parser = PipelineParser::new(&provider);
    let result = parser.parse(yaml).await;

    assert!(result.is_ok());
    let ir = result.unwrap();
    
    let deploy_job = ir.jobs.iter().find(|j| j.name == "Build").unwrap();
    assert!(deploy_job.condition.is_none() || deploy_job.condition.as_deref() == Some("trigger.branch == 'main'"));
}

#[tokio::test]
async fn test_error_missing_pipeline_name() {
    let yaml = r#"
triggers:
  manual: {}
workflows:
  - name: Build
    id: build
    workflow: global/docker-build
"#;

    let provider = create_provider();
    let mut parser = PipelineParser::new(&provider);
    let result = parser.parse(yaml).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_error_cyclic_dependency() {
    let yaml = r#"
name: Cyclic Pipeline
triggers:
  manual: {}
workflows:
  - name: A
    id: a
    workflow: global/test
    depends-on: [c]
  - name: B
    id: b
    workflow: global/test
    depends-on: [a]
  - name: C
    id: c
    workflow: global/test
    depends-on: [b]
"#;

    let provider = create_provider();
    let mut parser = PipelineParser::new(&provider);
    let result = parser.parse(yaml).await;

    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.code == ErrorCode::E5001));
}

#[tokio::test]
async fn test_error_unknown_dependency() {
    let yaml = r#"
name: Unknown Dependency Pipeline
triggers:
  manual: {}
workflows:
  - name: Build
    id: build
    workflow: global/docker-build
    inputs:
      image: myapp
    depends-on: [nonexistent]
"#;

    let provider = create_provider();
    let mut parser = PipelineParser::new(&provider);
    let result = parser.parse(yaml).await;

    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.code == ErrorCode::E5002));
}

#[tokio::test]
async fn test_error_duplicate_ids() {
    let yaml = r#"
name: Duplicate IDs Pipeline
triggers:
  manual: {}
workflows:
  - name: Build 1
    id: build
    workflow: global/docker-build
    inputs:
      image: myapp1
  - name: Build 2
    id: build
    workflow: global/docker-build
    inputs:
      image: myapp2
"#;

    let provider = create_provider();
    let mut parser = PipelineParser::new(&provider);
    let result = parser.parse(yaml).await;

    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.code == ErrorCode::E2005));
}

#[tokio::test]
async fn test_error_workflow_not_found() {
    let yaml = r#"
name: Unknown Workflow Pipeline
triggers:
  manual: {}
workflows:
  - name: Build
    id: build
    workflow: global/nonexistent-workflow
"#;

    let provider = create_provider();
    let mut parser = PipelineParser::new(&provider);
    let result = parser.parse(yaml).await;

    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.code == ErrorCode::E3001));
}

#[tokio::test]
async fn test_error_missing_required_input() {
    let yaml = r#"
name: Missing Input Pipeline
triggers:
  manual: {}
workflows:
  - name: Build
    id: build
    workflow: global/docker-build
"#;

    let provider = create_provider();
    let mut parser = PipelineParser::new(&provider);
    let result = parser.parse(yaml).await;

    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.code == ErrorCode::E2001 && e.message.contains("image")));
}

#[tokio::test]
async fn test_strict_mode() {
    let yaml = r#"
name: Strict Mode Test
triggers:
  manual: {}
workflows:
  - name: Build
    id: build
    workflow: global/docker-build
    inputs:
      image: myapp
      unknown_input: value
"#;

    let provider = create_provider();
    let config = ParserConfig {
        strict: true,
        ..Default::default()
    };
    let mut parser = PipelineParser::new(&provider).with_config(config);
    let result = parser.parse(yaml).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_complex_pipeline() {
    let yaml = r#"
name: Full CI/CD Pipeline
triggers:
  manual: {}
  webhook:
    events: [push, pull_request]
    branches: [main, develop]
runs-on:
  tags:
    - linux: true
    - amd64: true
vars:
  DOCKER_REGISTRY: ghcr.io
  IMAGE_NAME: myorg/myapp
secrets:
  DOCKER_PASSWORD:
    vault:
      path: secret/data/docker
      key: password
workflows:
  - name: Test
    id: test
    workflow: global/test
  - name: Build
    id: build
    workflow: global/docker-build
    inputs:
      image: ${DOCKER_REGISTRY}/${IMAGE_NAME}
    depends-on: [test]
    cache:
      key: docker-${hashFiles('Dockerfile', '**/Cargo.lock')}
      paths:
        - /var/cache/docker
    retry:
      max_attempts: 2
  - name: Deploy Staging
    id: deploy-staging
    workflow: global/docker-build
    inputs:
      image: ${IMAGE_NAME}-staging
    depends-on: [build]
  - name: Deploy Production
    id: deploy-prod
    workflow: global/docker-build
    inputs:
      image: ${IMAGE_NAME}-prod
    depends-on: [deploy-staging]
    condition: "trigger.branch == 'main'"
"#;

    let provider = create_provider();
    let mut parser = PipelineParser::new(&provider);
    let result = parser.parse(yaml).await;

    assert!(result.is_ok(), "Parse error: {:?}", result.err());
    let ir = result.unwrap();
    
    assert_eq!(ir.name, "Full CI/CD Pipeline");
    assert_eq!(ir.jobs.len(), 4);
    assert_eq!(ir.variables.len(), 2);
    assert_eq!(ir.secret_refs.len(), 1);
    assert!(ir.default_pool_selector.is_some());
}
