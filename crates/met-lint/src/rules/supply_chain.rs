//! Supply-chain integrity rules (ADR-009, ADR-015).

use crate::Diagnostic;
use met_parser::{RawJob, RawWorkflowInvocation};

/// SC-004: Environment image must be pinned to a digest (`@sha256:`).
fn check_digest_pin(image: &str) -> Option<Diagnostic> {
    if !image.contains("@sha256:") {
        return Some(Diagnostic::error(
            "SC-004",
            format!(
                "environment image `{image}` is not pinned to a digest; \
                 use `image@sha256:<hash>` for reproducibility"
            ),
        ));
    }
    None
}

/// SC-006: No signature verification configured.
fn check_verify_present(verify: Option<&str>) -> Option<Diagnostic> {
    match verify {
        None | Some("none") | Some("") => Some(Diagnostic::warning(
            "SC-006",
            "no signature verification configured for environment image; \
             consider `verify: cosign`"
                .to_string(),
        )),
        _ => None,
    }
}

/// Check a single job's `environment:` block for SC-004 and SC-006.
pub fn check_job_environment(job: &RawJob, diags: &mut Vec<Diagnostic>) {
    let Some(env) = &job.environment else {
        return;
    };

    if let Some(mut d) = check_digest_pin(&env.image) {
        d.message = format!("job `{}`: {}", job.id, d.message);
        diags.push(d);
    }

    if let Some(mut d) = check_verify_present(env.verify.as_deref()) {
        d.message = format!("job `{}`: {}", job.id, d.message);
        diags.push(d);
    }
}

/// Check workflow invocation for pinning (SC-005 placeholder — future rule for
/// broad registry credential patterns).
pub fn check_workflow_invocation(
    _wf: &RawWorkflowInvocation,
    _diags: &mut Vec<Diagnostic>,
) {
    // Workflow-level supply chain checks will be added as rules mature.
}

#[cfg(test)]
mod tests {
    use super::*;
    use met_parser::{RawEnvironment, RawJob};

    fn make_job(id: &str, image: &str, verify: Option<&str>) -> RawJob {
        RawJob {
            name: id.to_string(),
            id: id.to_string(),
            runs_on: None,
            environment: Some(RawEnvironment {
                image: image.to_string(),
                verify: verify.map(String::from),
                credentials: None,
                pull_policy: None,
            }),
            steps: Vec::new(),
            services: Vec::new(),
            depends_on: Vec::new(),
            condition: None,
            timeout: None,
            retry: None,
        }
    }

    #[test]
    fn sc004_unpinned_image() {
        let job = make_job("build", "ghcr.io/acme/build:latest", None);
        let mut diags = Vec::new();
        check_job_environment(&job, &mut diags);
        assert!(diags.iter().any(|d| d.rule_id == "SC-004"));
    }

    #[test]
    fn sc004_pinned_image_ok() {
        let job = make_job(
            "build",
            "ghcr.io/acme/build@sha256:abcdef1234567890",
            Some("cosign"),
        );
        let mut diags = Vec::new();
        check_job_environment(&job, &mut diags);
        assert!(!diags.iter().any(|d| d.rule_id == "SC-004"));
    }

    #[test]
    fn sc006_no_verify() {
        let job = make_job(
            "build",
            "ghcr.io/acme/build@sha256:abcdef1234567890",
            None,
        );
        let mut diags = Vec::new();
        check_job_environment(&job, &mut diags);
        assert!(diags.iter().any(|d| d.rule_id == "SC-006"));
    }

    #[test]
    fn sc006_cosign_ok() {
        let job = make_job(
            "build",
            "ghcr.io/acme/build@sha256:abcdef1234567890",
            Some("cosign"),
        );
        let mut diags = Vec::new();
        check_job_environment(&job, &mut diags);
        assert!(!diags.iter().any(|d| d.rule_id == "SC-006"));
    }

    #[test]
    fn no_environment_no_diags() {
        let job = RawJob {
            name: "plain".to_string(),
            id: "plain".to_string(),
            runs_on: None,
            environment: None,
            steps: Vec::new(),
            services: Vec::new(),
            depends_on: Vec::new(),
            condition: None,
            timeout: None,
            retry: None,
        };
        let mut diags = Vec::new();
        check_job_environment(&job, &mut diags);
        assert!(diags.is_empty());
    }
}
