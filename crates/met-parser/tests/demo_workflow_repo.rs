//! Optional: parses demo YAML from sibling `meticulous-ci-workflows` checkout (skipped if missing).

use std::path::Path;

use met_parser::{GitWorkflowProvider, PipelineParser, TagValue};

#[tokio::test]
async fn demo_cross_platform_pipeline_parses() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../meticulous-ci-workflows");
    let yaml_path = repo.join(".stable/demo-cross-platform.yaml");
    if !yaml_path.is_file() {
        eprintln!(
            "skip demo_cross_platform_pipeline_parses: {} not found",
            yaml_path.display()
        );
        return;
    }
    let yaml = std::fs::read_to_string(&yaml_path).expect("read demo yaml");
    let provider = GitWorkflowProvider::new(&repo, None);
    let mut parser = PipelineParser::new(&provider);
    let ir = parser
        .parse(&yaml)
        .await
        .unwrap_or_else(|e| panic!("parse demo-cross-platform: {e:?}"));
    assert!(
        ir.jobs.len() >= 2,
        "expected at least two jobs (linux + macos), got {}",
        ir.jobs.len()
    );
    let linux_job = ir.jobs.iter().find(|j| {
        matches!(j.pool_selector.required_tags.get("os"), Some(TagValue::String(s)) if s == "linux")
    });
    let mac_job = ir.jobs.iter().find(|j| {
        matches!(j.pool_selector.required_tags.get("os"), Some(TagValue::String(s)) if s == "macos")
    });
    assert!(
        linux_job.is_some(),
        "expected a job with runs-on tag os: linux"
    );
    assert!(
        mac_job.is_some(),
        "expected a job with runs-on tag os: macos"
    );
}

#[tokio::test]
async fn demo_git_clone_pipeline_parses() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../meticulous-ci-workflows");
    let yaml_path = repo.join(".stable/demo-git-clone.yaml");
    if !yaml_path.is_file() {
        eprintln!(
            "skip demo_git_clone_pipeline_parses: {} not found",
            yaml_path.display()
        );
        return;
    }
    let yaml = std::fs::read_to_string(&yaml_path).expect("read demo git-clone yaml");
    let provider = GitWorkflowProvider::new(&repo, None);
    let mut parser = PipelineParser::new(&provider);
    parser
        .parse(&yaml)
        .await
        .unwrap_or_else(|e| panic!("parse demo-git-clone: {e:?}"));
}
