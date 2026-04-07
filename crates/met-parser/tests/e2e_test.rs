//! End-to-end parser test: parse example pipeline YAML, verify IR structure.

use indexmap::IndexMap;
use met_parser::{
    MockWorkflowProvider, PipelineParser, RawJob, RawStep, RawWorkflowDef, WorkflowScope,
    EnvValue, StepCommand, Trigger,
};
use met_parser::schema::{RawInputDef, RawOutputDef};

fn create_full_provider() -> MockWorkflowProvider {
    let mut provider = MockWorkflowProvider::new();

    let checkout_workflow = RawWorkflowDef {
        name: "Git Checkout".to_string(),
        description: Some("Checkout code from git".to_string()),
        version: Some("2.0.0".to_string()),
        inputs: {
            let mut m = IndexMap::new();
            m.insert("ref".to_string(), met_parser::schema::RawInputDef {
                input_type: "string".to_string(),
                required: false,
                default: Some(serde_yaml::Value::String("HEAD".to_string())),
                description: None,
            });
            m
        },
        outputs: IndexMap::new(),
        jobs: vec![RawJob {
            name: "Checkout".to_string(),
            id: "checkout".to_string(),
            runs_on: None,
            steps: vec![RawStep {
                name: "Clone Repository".to_string(),
                id: Some("clone".to_string()),
                run: Some("git checkout ${ref}".to_string()),
                shell: None,
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
    };

    let build_workflow = RawWorkflowDef {
        name: "Rust Build".to_string(),
        description: Some("Build Rust project".to_string()),
        version: Some("1.0.0".to_string()),
        inputs: {
            let mut m = IndexMap::new();
            m.insert("profile".to_string(), met_parser::schema::RawInputDef {
                input_type: "string".to_string(),
                required: false,
                default: Some(serde_yaml::Value::String("release".to_string())),
                description: None,
            });
            m
        },
        outputs: IndexMap::new(),
        jobs: vec![RawJob {
            name: "Build".to_string(),
            id: "build".to_string(),
            runs_on: None,
            steps: vec![
                RawStep {
                    name: "Install Dependencies".to_string(),
                    id: Some("deps".to_string()),
                    run: Some("cargo fetch".to_string()),
                    shell: None,
                    uses: None,
                    action_inputs: IndexMap::new(),
                    env: IndexMap::new(),
                    working_directory: None,
                    timeout: None,
                    continue_on_error: false,
                    outputs: IndexMap::new(),
                },
                RawStep {
                    name: "Compile".to_string(),
                    id: Some("compile".to_string()),
                    run: Some("cargo build --profile ${profile}".to_string()),
                    shell: None,
                    uses: None,
                    action_inputs: IndexMap::new(),
                    env: IndexMap::new(),
                    working_directory: None,
                    timeout: None,
                    continue_on_error: false,
                    outputs: IndexMap::new(),
                },
            ],
            services: vec![],
            depends_on: vec![],
            condition: None,
            timeout: None,
            retry: None,
        }],
    };

    let test_workflow = RawWorkflowDef {
        name: "Rust Test".to_string(),
        description: Some("Run Rust tests".to_string()),
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
                run: Some("cargo test --all".to_string()),
                shell: None,
                uses: None,
                action_inputs: IndexMap::new(),
                env: {
                    let mut env = IndexMap::new();
                    env.insert("RUST_BACKTRACE".to_string(), "1".to_string());
                    env
                },
                working_directory: None,
                timeout: None,
                continue_on_error: false,
                outputs: IndexMap::new(),
            }],
            services: vec![met_parser::schema::RawService {
                name: "postgres".to_string(),
                image: "postgres:15".to_string(),
                ports: vec![5432],
                env: {
                    let mut env = IndexMap::new();
                    env.insert("POSTGRES_PASSWORD".to_string(), "test".to_string());
                    env
                },
                command: None,
                health_check: None,
            }],
            depends_on: vec![],
            condition: None,
            timeout: None,
            retry: None,
        }],
    };

    provider.add_workflow(WorkflowScope::Global, "checkout", checkout_workflow);
    provider.add_workflow(WorkflowScope::Global, "rust-build", build_workflow);
    provider.add_workflow(WorkflowScope::Global, "rust-test", test_workflow);

    provider
}

#[tokio::test]
async fn test_e2e_full_pipeline_parsing() {
    let yaml = r#"
name: Meticulous CI Pipeline
triggers:
  manual:
    description: Manual trigger for testing
  webhook:
    events: [push, pull_request]
    branches: [main, develop, "release/*"]
    paths:
      - "crates/**"
      - "Cargo.toml"
      - "Cargo.lock"
    paths_ignore:
      - "*.md"
      - "docs/**"
  schedule:
    cron: "0 4 * * *"
    timezone: UTC

runs-on:
  tags:
    - linux: true
    - rust: true
  pool: default

vars:
  RUST_VERSION: "1.75"
  CARGO_INCREMENTAL: "0"

secrets:
  CODECOV_TOKEN:
    vault:
      path: secret/data/ci
      key: codecov_token
  DOCKER_PASSWORD:
    aws:
      arn: arn:aws:secretsmanager:us-east-1:123456789:secret:docker

workflows:
  - name: Checkout Code
    id: checkout
    workflow: global/checkout
    timeout: 5m

  - name: Build
    id: build
    workflow: global/rust-build
    inputs:
      profile: release
    depends-on: [checkout]
    timeout: 30m
    cache:
      key: cargo-${RUST_VERSION}-${hashFiles('**/Cargo.lock')}
      paths:
        - ~/.cargo/registry
        - ~/.cargo/git
        - target/
      restore-keys:
        - cargo-${RUST_VERSION}-

  - name: Unit Tests
    id: test-unit
    workflow: global/rust-test
    depends-on: [build]
    timeout: 20m
    retry:
      max_attempts: 2
      backoff: 30s

  - name: Integration Tests
    id: test-integration
    workflow: global/rust-test
    depends-on: [build]
    timeout: 30m
    condition: "trigger.event != 'pull_request' || trigger.branch == 'main'"

  - name: Release
    id: release
    workflow: global/rust-build
    inputs:
      profile: release
    depends-on: [test-unit, test-integration]
    condition: "trigger.branch == 'main' && trigger.event == 'push'"
"#;

    let provider = create_full_provider();
    let mut parser = PipelineParser::new(&provider);
    
    let result = parser.parse(yaml).await;
    assert!(result.is_ok(), "Parse failed: {:?}", result.err());
    
    let ir = result.unwrap();

    assert_eq!(ir.name, "Meticulous CI Pipeline");

    assert!(ir.triggers.iter().any(|t| matches!(t, Trigger::Manual)));
    assert!(ir.triggers.iter().any(|t| matches!(t, Trigger::Webhook(_))));
    assert!(ir.triggers.iter().any(|t| matches!(t, Trigger::Schedule(_))));

    let webhook = ir.triggers.iter().find_map(|t| {
        if let Trigger::Webhook(w) = t { Some(w) } else { None }
    }).unwrap();
    assert_eq!(webhook.branches.len(), 3);
    assert!(webhook.paths.contains(&"crates/**".to_string()));

    assert_eq!(ir.variables.len(), 2);
    assert_eq!(ir.variables.get("RUST_VERSION"), Some(&"1.75".to_string()));

    assert_eq!(ir.secret_refs.len(), 2);
    assert!(ir.secret_refs.contains_key("CODECOV_TOKEN"));
    assert!(ir.secret_refs.contains_key("DOCKER_PASSWORD"));

    assert!(ir.default_pool_selector.is_some());
    let pool = ir.default_pool_selector.as_ref().unwrap();
    assert!(pool.required_tags.contains_key("linux"));
    assert!(pool.required_tags.contains_key("rust"));
    assert_eq!(pool.pool_name, Some("default".to_string()));

    assert_eq!(ir.jobs.len(), 5);

    let checkout_job = ir.jobs.iter().find(|j| j.name == "Checkout").unwrap();
    assert!(checkout_job.depends_on.is_empty());

    let build_job = ir.jobs.iter().find(|j| j.name == "Build").unwrap();
    assert_eq!(build_job.depends_on.len(), 1);
    assert!(build_job.cache_config.is_some());
    let cache = build_job.cache_config.as_ref().unwrap();
    assert!(cache.key.contains("RUST_VERSION"));

    let test_job = ir.jobs.iter().find(|j| j.name == "Test").unwrap();
    assert!(!test_job.services.is_empty());
    let postgres_service = &test_job.services[0];
    assert_eq!(postgres_service.name, "postgres");
    assert_eq!(postgres_service.image, "postgres:15");

    let release_job = ir.jobs.iter().find(|j| j.name == "Build" && j.condition.is_some());
    assert!(release_job.is_some());
}

#[tokio::test]
async fn test_e2e_ir_structure_integrity() {
    let yaml = r#"
name: IR Structure Test
triggers:
  manual: {}
vars:
  MY_VAR: test_value
workflows:
  - name: Job One
    id: job1
    workflow: global/checkout
  - name: Job Two
    id: job2
    workflow: global/rust-build
    depends-on: [job1]
    inputs:
      profile: debug
"#;

    let provider = create_full_provider();
    let mut parser = PipelineParser::new(&provider);
    let ir = parser.parse(yaml).await.expect("Parse should succeed");

    assert!(!ir.id.as_uuid().is_nil());

    assert_eq!(ir.jobs.len(), 2);
    for job in &ir.jobs {
        assert!(!job.id.as_uuid().is_nil());
        for step in &job.steps {
            assert!(!step.id.as_uuid().is_nil());
        }
    }

    let job2 = ir.jobs.iter().find(|j| j.name == "Build").unwrap();
    assert_eq!(job2.depends_on.len(), 1);

    let job1 = ir.jobs.iter().find(|j| j.name == "Checkout").unwrap();
    assert!(job2.depends_on.iter().any(|dep| ir.jobs.iter().any(|j| &j.id == dep)));

    for job in &ir.jobs {
        for step in &job.steps {
            match &step.command {
                StepCommand::Run { script, .. } => {
                    assert!(!script.is_empty());
                }
                StepCommand::Action { name, version, .. } => {
                    assert!(!name.is_empty());
                    assert!(!version.is_empty());
                }
            }
        }
    }
}

#[tokio::test]
async fn test_e2e_topological_ordering() {
    let yaml = r#"
name: Topological Test
triggers:
  manual: {}
workflows:
  - name: D
    id: d
    workflow: global/checkout
    depends-on: [b, c]
  - name: A
    id: a
    workflow: global/checkout
  - name: B
    id: b
    workflow: global/checkout
    depends-on: [a]
  - name: C
    id: c
    workflow: global/checkout
    depends-on: [a]
"#;

    let provider = create_full_provider();
    let mut parser = PipelineParser::new(&provider);
    let ir = parser.parse(yaml).await.expect("Parse should succeed");

    let topo_order = met_engine::topological_order(&ir).expect("Should have valid ordering");

    let positions: std::collections::HashMap<_, _> = topo_order
        .iter()
        .enumerate()
        .map(|(i, id)| (*id, i))
        .collect();

    let job_a = ir.jobs.iter().find(|j| j.name == "Checkout" && j.depends_on.is_empty()).unwrap();
    let job_d = ir.jobs.iter().find(|j| j.name == "Checkout" && j.depends_on.len() == 2).unwrap();
    
    if let (Some(&pos_a), Some(&pos_d)) = (positions.get(&job_a.id), positions.get(&job_d.id)) {
        assert!(pos_a < pos_d, "A should come before D");
    }
}

#[tokio::test]
async fn test_e2e_workflow_traceability() {
    let yaml = r#"
name: Traceability Test
triggers:
  manual: {}
workflows:
  - name: My Build
    id: mybuild
    workflow: global/rust-build
    inputs:
      profile: release
"#;

    let provider = create_full_provider();
    let mut parser = PipelineParser::new(&provider);
    let ir = parser.parse(yaml).await.expect("Parse should succeed");

    let job = &ir.jobs[0];
    assert!(job.source_workflow.is_some());
    
    let source = job.source_workflow.as_ref().unwrap();
    assert_eq!(source.scope, WorkflowScope::Global);
    assert_eq!(source.name, "rust-build");
}

#[tokio::test]
async fn test_e2e_workflow_outputs_reference_in_inputs() {
    let mut provider = MockWorkflowProvider::new();

    let mut build_outputs = IndexMap::new();
    build_outputs.insert(
        "image_uri".to_string(),
        RawOutputDef {
            value: None,
            description: Some("OCI reference".to_string()),
            secret: false,
        },
    );

    let build_wf = RawWorkflowDef {
        name: "build".to_string(),
        description: None,
        version: Some("1.0.0".to_string()),
        inputs: IndexMap::new(),
        outputs: build_outputs,
        jobs: vec![RawJob {
            name: "Build".to_string(),
            id: "build_job".to_string(),
            runs_on: None,
            steps: vec![RawStep {
                name: "noop".to_string(),
                id: Some("noop".to_string()),
                run: Some("true".to_string()),
                shell: None,
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
    };

    let mut deploy_inputs = IndexMap::new();
    deploy_inputs.insert(
        "image".to_string(),
        RawInputDef {
            input_type: "string".to_string(),
            required: true,
            default: None,
            description: None,
        },
    );

    let deploy_wf = RawWorkflowDef {
        name: "deploy".to_string(),
        description: None,
        version: Some("1.0.0".to_string()),
        inputs: deploy_inputs,
        outputs: IndexMap::new(),
        jobs: vec![RawJob {
            name: "Deploy".to_string(),
            id: "deploy_job".to_string(),
            runs_on: None,
            steps: vec![RawStep {
                name: "apply".to_string(),
                id: Some("apply".to_string()),
                run: Some("echo \"$IMAGE\"".to_string()),
                shell: None,
                uses: None,
                action_inputs: IndexMap::new(),
                env: {
                    let mut m = IndexMap::new();
                    m.insert("IMAGE".to_string(), "${{ inputs.image }}".to_string());
                    m
                },
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
    };

    provider.add_workflow(WorkflowScope::Global, "build", build_wf);
    provider.add_workflow(WorkflowScope::Global, "deploy", deploy_wf);

    let yaml = r#"
name: workflow outputs chain
triggers:
  manual: {}
workflows:
  - name: Build
    id: build
    workflow: global/build
    version: "1.0.0"
  - name: Deploy
    id: deploy
    workflow: global/deploy
    version: "1.0.0"
    depends-on: [build]
    inputs:
      image: "${{ workflows.build.outputs.image_uri }}"
"#;

    let mut parser = PipelineParser::new(&provider);
    let ir = parser.parse(yaml).await.expect("parse");
    assert_eq!(ir.jobs.len(), 2, "build + deploy jobs");
    let build_job = ir
        .jobs
        .iter()
        .find(|j| j.workflow_invocation_id.as_deref() == Some("build"))
        .expect("build job");
    let deploy_job = ir
        .jobs
        .iter()
        .find(|j| j.workflow_invocation_id.as_deref() == Some("deploy"))
        .expect("deploy job");
    assert!(
        deploy_job.depends_on.contains(&build_job.id),
        "deploy must depend on concrete build job id"
    );
    let step = &deploy_job.steps[0];
    let env_img = step.env.get("IMAGE").expect("IMAGE env");
    let EnvValue::Expression(e) = env_img else {
        panic!("expected expression env for inputs.image");
    };
    assert!(
        e.contains("inputs.image"),
        "deploy step should reference injected input (got {e})"
    );
}
