//! Core validation and resolution logic.

use std::str::FromStr;

use indexmap::IndexMap;
use met_core::ids::{JobRunId, OrganizationId, PipelineId, ProjectId};
use met_parser::{PipelineParser, SecretRef};
use met_parser::WorkflowProvider;
use met_secrets::{
    parse_github_app_credentials, installation_access_token, BuiltinStoredCrypto,
};
use met_store::repos::{BuiltinSecretsRepo, JobRunPipelineContext, JobRunRepo};
use met_store::PgPool;

use crate::hints::SecretResolutionHints;

use crate::error::{parse_errors, ResolveError};

/// Map DB `kind` to agent materialization (matches `agent.v1.SecretMaterialKind` protobuf).
#[must_use]
pub fn materialization_for_kind(kind: &str) -> i32 {
    match kind {
        "github_app" => 1, // installation token is always env-inline at job time
        "ssh_private_key" | "x509_bundle" => 2, // WORKSPACE_FILE_PATH
        _ => 1, // ENV_INLINE
    }
}

fn definition_to_yaml(def: &serde_json::Value) -> Result<String, ResolveError> {
    if let Some(s) = def.as_str() {
        return Ok(s.to_string());
    }
    serde_yaml::to_string(def).map_err(|e| ResolveError::Parse(e.to_string()))
}

/// Load pipeline IR secret refs from stored `pipelines.definition` JSON.
pub async fn load_secret_refs_from_definition(
    def: &serde_json::Value,
    workflows: &dyn WorkflowProvider,
) -> Result<IndexMap<String, SecretRef>, ResolveError> {
    let yaml = definition_to_yaml(def)?;
    let mut parser = PipelineParser::new(workflows);
    let ir = parser
        .parse(&yaml)
        .await
        .map_err(parse_errors)?;
    Ok(ir.secret_refs)
}

/// Ensure every `SecretRef` in the map is satisfiable.
pub async fn validate_secret_refs(
    pool: &PgPool,
    org_id: OrganizationId,
    project_id: Option<ProjectId>,
    pipeline_id: PipelineId,
    refs: &IndexMap<String, SecretRef>,
) -> Result<(), ResolveError> {
    let Some(project_id) = project_id else {
        return Err(ResolveError::MissingProjectId);
    };

    for (env, r) in refs {
        if matches!(r, SecretRef::Aws { .. } | SecretRef::Vault { .. }) {
            return Err(ResolveError::ExternalNotConfigured(env.clone()));
        }
    }

    let repo = BuiltinSecretsRepo::new(pool);

    let mut missing = Vec::new();
    for (env_name, pref) in refs {
        match pref {
            SecretRef::Stored { name } | SecretRef::Builtin { name } => {
                if !repo
                    .exists_resolvable(org_id, project_id, pipeline_id, name)
                    .await?
                {
                    missing.push(env_name.clone());
                }
            }
            SecretRef::Aws { .. } | SecretRef::Vault { .. } => {}
        }
    }

    if missing.is_empty() {
        Ok(())
    } else {
        Err(ResolveError::MissingSecrets(missing))
    }
}

/// Resolve stored/builtin secrets to plaintext maps keyed by pipeline env name.
/// External (AWS/Vault) refs return [`ResolveError::ExternalNotConfigured`].
pub async fn resolve_stored_secret_map(
    pool: &PgPool,
    crypto: &BuiltinStoredCrypto,
    org_id: OrganizationId,
    project_id: Option<ProjectId>,
    pipeline_id: PipelineId,
    refs: &IndexMap<String, SecretRef>,
) -> Result<IndexMap<String, (String, String, i32)>, ResolveError> {
    let Some(project_id) = project_id else {
        return Err(ResolveError::MissingProjectId);
    };

    let repo = BuiltinSecretsRepo::new(pool);
    let mut out = IndexMap::new();

    for (env_name, pref) in refs {
        match pref {
            SecretRef::Stored { name } | SecretRef::Builtin { name } => {
                let row = repo
                    .get_current_cipher_row(org_id, project_id, pipeline_id, name)
                    .await?
                    .ok_or_else(|| ResolveError::MissingSecrets(vec![env_name.clone()]))?;

                let nonce: [u8; 12] = row
                    .nonce
                    .as_slice()
                    .try_into()
                    .map_err(|_| ResolveError::BadNonce)?;

                let pt = crypto
                    .decrypt(&row.encrypted_value, &nonce)
                    .map_err(|e| ResolveError::Crypto(e.to_string()))?;
                let s = String::from_utf8(pt.to_vec())
                    .map_err(|e| ResolveError::Crypto(format!("utf8: {e}")))?;

                if row.kind == "github_app" {
                    let creds = parse_github_app_credentials(&s).map_err(|e| {
                        ResolveError::Crypto(e.to_string())
                    })?;
                    let token = installation_access_token(&creds)
                        .await
                        .map_err(|e| ResolveError::Crypto(e.to_string()))?;
                    out.insert(
                        env_name.clone(),
                        (token, "github_app".to_string(), 1),
                    );
                } else {
                    let mat = materialization_for_kind(&row.kind);
                    out.insert(env_name.clone(), (s, row.kind, mat));
                }
            }
            SecretRef::Aws { .. } | SecretRef::Vault { .. } => {
                return Err(ResolveError::ExternalNotConfigured(env_name.clone()));
            }
        }
    }

    Ok(out)
}

/// Resolve secrets for a job run using DB pipeline definition (controller).
pub async fn resolve_for_job_run_context(
    pool: &PgPool,
    crypto: &BuiltinStoredCrypto,
    ctx: &JobRunPipelineContext,
    workflows: &dyn WorkflowProvider,
) -> Result<IndexMap<String, (String, String, i32)>, ResolveError> {
    let refs = load_secret_refs_from_definition(&ctx.definition, workflows).await?;
    let org_id = OrganizationId::from_uuid(ctx.org_id);
    let project_id = ProjectId::from_uuid(ctx.project_id);
    let pipeline_id = PipelineId::from_uuid(ctx.pipeline_id);
    resolve_stored_secret_map(
        pool,
        crypto,
        org_id,
        Some(project_id),
        pipeline_id,
        &refs,
    )
    .await
}

/// Resolve plaintext secrets for `ExchangeJobKeys`: try DB `job_run` join first, else hints + ids.
pub async fn resolve_job_secrets_for_exchange(
    pool: &PgPool,
    crypto: &BuiltinStoredCrypto,
    job_run_id: &str,
    org_id: &str,
    project_id: &str,
    pipeline_id: &str,
    hints_json: &str,
) -> Result<Vec<(String, String, i32)>, ResolveError> {
    if let Ok(jrid) = JobRunId::from_str(job_run_id.trim()) {
        if let Some(ctx) = JobRunRepo::new(pool).get_pipeline_context(jrid).await? {
            let wp = met_parser::MockWorkflowProvider::default();
            let map = resolve_for_job_run_context(pool, crypto, &ctx, &wp).await?;
            return Ok(map
                .into_iter()
                .map(|(k, (v, _kind, mat))| (k, v, mat))
                .collect());
        }
    }

    if hints_json.trim().is_empty() {
        return Ok(Vec::new());
    }

    let org_id = OrganizationId::from_str(org_id.trim()).map_err(|e| ResolveError::Parse(e.to_string()))?;
    let project_id =
        ProjectId::from_str(project_id.trim()).map_err(|e| ResolveError::Parse(e.to_string()))?;
    let pipeline_id =
        PipelineId::from_str(pipeline_id.trim()).map_err(|e| ResolveError::Parse(e.to_string()))?;

    let hints: SecretResolutionHints =
        serde_json::from_str(hints_json.trim()).map_err(|e| ResolveError::Parse(e.to_string()))?;

    let mut m = IndexMap::new();
    for h in hints.refs {
        m.insert(h.env_name, SecretRef::Stored { name: h.path });
    }

    let map = resolve_stored_secret_map(
        pool,
        crypto,
        org_id,
        Some(project_id),
        pipeline_id,
        &m,
    )
    .await?;
    Ok(map
        .into_iter()
        .map(|(k, (v, _kind, mat))| (k, v, mat))
        .collect())
}
