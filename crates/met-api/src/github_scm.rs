//! GitHub SCM helpers: repository parsing, file + tarball fetch, extraction.

use std::io::Read;
use std::path::{Path, PathBuf};

use base64::Engine;
use flate2::read::GzDecoder;
use met_core::ids::{OrganizationId, PipelineId, ProjectId};
use met_parser::{
    CompositeWorkflowProvider, DatabaseWorkflowProvider, GitWorkflowProvider, PipelineIR,
    PipelineParser,
};
use met_secrets::{BuiltinStoredCrypto, installation_access_token, parse_github_app_credentials};
use met_store::StoreError;
use met_store::PgPool;
use met_store::repos::BuiltinSecretCipherRow;
use met_store::repos::{BuiltinSecretsRepo, CreateWorkflow, StoredSecretKind, WorkflowRepo};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use tracing::instrument;

use crate::error::{ApiError, ApiResult};

/// Owner + repo name for `api.github.com/repos/{owner}/{repo}/...`.
#[derive(Debug, Clone)]
pub struct RepoSlug {
    pub owner: String,
    pub name: String,
}

/// Parse `owner/repo`, `https://github.com/o/r`, or `https://github.com/o/r.git`.
pub fn parse_github_repository(input: &str) -> Result<RepoSlug, ApiError> {
    let s = input.trim();
    if s.is_empty() {
        return Err(ApiError::bad_request("repository is required"));
    }
    if let Some(rest) = s
        .strip_prefix("https://github.com/")
        .or_else(|| s.strip_prefix("http://github.com/"))
    {
        let rest = rest.trim_end_matches(".git").trim_end_matches('/');
        let mut parts = rest.splitn(2, '/');
        let owner = parts
            .next()
            .filter(|o| !o.is_empty())
            .ok_or_else(|| ApiError::bad_request("invalid GitHub repository URL"))?;
        let name = parts
            .next()
            .filter(|n| !n.is_empty())
            .ok_or_else(|| ApiError::bad_request("invalid GitHub repository URL"))?;
        return Ok(RepoSlug {
            owner: owner.to_string(),
            name: name.to_string(),
        });
    }

    let mut parts = s.splitn(2, '/');
    let owner = parts
        .next()
        .filter(|o| !o.is_empty())
        .ok_or_else(|| ApiError::bad_request("repository must be owner/name"))?;
    let name = parts
        .next()
        .filter(|n| !n.is_empty())
        .ok_or_else(|| ApiError::bad_request("repository must be owner/name"))?;
    Ok(RepoSlug {
        owner: owner.to_string(),
        name: name.to_string(),
    })
}

fn api_base_from_env_inline_json(creds_json: &str) -> String {
    parse_github_app_credentials(creds_json)
        .map(|c| {
            c.github_api_base
                .as_deref()
                .unwrap_or("https://api.github.com")
                .trim_end_matches('/')
                .to_string()
        })
        .unwrap_or_else(|_| "https://api.github.com".to_string())
}

/// When `catalog_scm` is true, org-wide secrets that do not propagate to projects can be resolved
/// (global workflow catalog Git import).
#[instrument(skip(pool, crypto), fields(org_id = %org_id, project_id = %project_id))]
pub async fn github_app_installation_token_for_project_secret(
    pool: &PgPool,
    crypto: &BuiltinStoredCrypto,
    org_id: OrganizationId,
    project_id: ProjectId,
    credentials_path: &str,
    catalog_scm: bool,
) -> ApiResult<String> {
    let repo = BuiltinSecretsRepo::new(pool);
    let nil_pipe = PipelineId::from_uuid(uuid::Uuid::nil());
    let row = if catalog_scm {
        repo
            .get_current_cipher_row_for_catalog_scm(org_id, project_id, nil_pipe, credentials_path)
            .await
    } else {
        repo
            .get_current_cipher_row(org_id, project_id, nil_pipe, credentials_path)
            .await
    }
    .map_err(met_store::StoreError::from)?
        .ok_or_else(|| {
            ApiError::bad_request(format!(
                "stored secret '{credentials_path}' not found for this project"
            ))
        })?;

    let kind =
        StoredSecretKind::parse(&row.kind).map_err(|e| ApiError::bad_request(e.to_string()))?;
    if kind != StoredSecretKind::GithubApp {
        return Err(ApiError::bad_request(format!(
            "secret '{}' must be kind github_app",
            credentials_path
        )));
    }

    let nonce: [u8; 12] = row
        .nonce
        .as_slice()
        .try_into()
        .map_err(|_| ApiError::bad_request("invalid secret nonce"))?;

    let pt = crypto
        .decrypt(&row.encrypted_value, &nonce)
        .map_err(|e| ApiError::internal(format!("decrypt stored secret: {e}")))?;
    let plaintext = String::from_utf8(pt.to_vec())
        .map_err(|e| ApiError::internal(format!("stored secret utf8: {e}")))?;

    let creds = parse_github_app_credentials(&plaintext)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;

    installation_access_token(&creds)
        .await
        .map_err(|e| ApiError::bad_request(format!("GitHub App token exchange: {e}")))
}

/// Resolve a ref (branch / tag / SHA) to the commit SHA via the Commits API.
#[instrument(skip(token), fields(owner = %owner, repo = %repo))]
pub async fn resolve_github_commit_sha(
    token: &str,
    owner: &str,
    repo: &str,
    git_ref: &str,
    api_base: &str,
) -> ApiResult<String> {
    #[derive(Deserialize)]
    struct CommitResp {
        sha: String,
    }

    let base = api_base.trim_end_matches('/');
    let url = format!(
        "{base}/repos/{owner}/{repo}/commits/{}",
        urlencoding::encode(git_ref)
    );
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| ApiError::internal(e.to_string()))?;

    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header(reqwest::header::USER_AGENT, "meticulous-control-plane")
        .send()
        .await
        .map_err(|e| ApiError::bad_request(format!("GitHub commits request: {e}")))?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(ApiError::bad_request(format!(
            "GitHub commits API {}: {}",
            status.as_u16(),
            body.chars().take(400).collect::<String>()
        )));
    }

    let parsed: CommitResp = serde_json::from_str(&body)
        .map_err(|e| ApiError::bad_request(format!("GitHub JSON: {e}")))?;
    Ok(parsed.sha)
}

/// List branches (`name` + tip SHA), first page only (`per_page` max 100).
#[instrument(skip(token), fields(owner = %owner, repo = %repo))]
pub async fn list_github_branches(
    token: &str,
    owner: &str,
    repo: &str,
    api_base: &str,
    per_page: u32,
) -> ApiResult<Vec<(String, String)>> {
    #[derive(Deserialize)]
    struct Row {
        name: String,
        commit: Tip,
    }
    #[derive(Deserialize)]
    struct Tip {
        sha: String,
    }

    let base = api_base.trim_end_matches('/');
    let cap = per_page.clamp(1, 100);
    let url = format!(
        "{base}/repos/{owner}/{repo}/branches?per_page={cap}"
    );
    github_get_json_array::<Row>(token, &url).await.map(|rows| {
        rows.into_iter()
            .map(|r| (r.name, r.commit.sha))
            .collect()
    })
}

/// List tags (`name` + object SHA), first page only (`per_page` max 100).
#[instrument(skip(token), fields(owner = %owner, repo = %repo))]
pub async fn list_github_tags(
    token: &str,
    owner: &str,
    repo: &str,
    api_base: &str,
    per_page: u32,
) -> ApiResult<Vec<(String, String)>> {
    #[derive(Deserialize)]
    struct Row {
        name: String,
        commit: Tip,
    }
    #[derive(Deserialize)]
    struct Tip {
        sha: String,
    }

    let base = api_base.trim_end_matches('/');
    let cap = per_page.clamp(1, 100);
    let url = format!("{base}/repos/{owner}/{repo}/tags?per_page={cap}");
    github_get_json_array::<Row>(token, &url).await.map(|rows| {
        rows.into_iter()
            .map(|r| (r.name, r.commit.sha))
            .collect()
    })
}

/// Recent commits on `git_ref` (branch, tag, or SHA), first page only.
#[instrument(skip(token), fields(owner = %owner, repo = %repo))]
pub async fn list_github_commits(
    token: &str,
    owner: &str,
    repo: &str,
    git_ref: &str,
    api_base: &str,
    per_page: u32,
) -> ApiResult<Vec<(String, String, Option<String>)>> {
    #[derive(Deserialize)]
    struct Row {
        sha: String,
        commit: CommitBody,
    }
    #[derive(Deserialize)]
    struct CommitBody {
        message: Option<String>,
        author: Option<Author>,
    }
    #[derive(Deserialize)]
    struct Author {
        date: Option<String>,
    }

    let base = api_base.trim_end_matches('/');
    let cap = per_page.clamp(1, 100);
    let url = format!(
        "{base}/repos/{owner}/{repo}/commits?sha={}&per_page={cap}",
        urlencoding::encode(git_ref)
    );
    let rows = github_get_json_array::<Row>(token, &url).await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let title = r
                .commit
                .message
                .as_deref()
                .unwrap_or("")
                .lines()
                .next()
                .unwrap_or("")
                .to_string();
            let at = r.commit.author.and_then(|a| a.date);
            (r.sha, title, at)
        })
        .collect())
}

async fn github_get_json_array<T: DeserializeOwned>(
    token: &str,
    url: &str,
) -> ApiResult<Vec<T>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| ApiError::internal(e.to_string()))?;

    let resp = client
        .get(url)
        .header("Authorization", format!("Bearer {token}"))
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header(reqwest::header::USER_AGENT, "meticulous-control-plane")
        .send()
        .await
        .map_err(|e| ApiError::bad_request(format!("GitHub API request: {e}")))?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(ApiError::bad_request(format!(
            "GitHub API {}: {}",
            status.as_u16(),
            body.chars().take(400).collect::<String>()
        )));
    }

    serde_json::from_str::<Vec<T>>(&body)
        .map_err(|e| ApiError::bad_request(format!("GitHub JSON: {e}")))
}

/// Fetch a repository file via the GitHub JSON Contents API (includes blob SHA).
#[instrument(skip(token), fields(owner = %owner, repo = %repo, path = %path))]
pub async fn fetch_github_text_file(
    token: &str,
    owner: &str,
    repo: &str,
    path: &str,
    git_ref: &str,
    api_base: &str,
) -> ApiResult<(String, String)> {
    #[derive(Deserialize)]
    struct ContentResp {
        encoding: Option<String>,
        content: Option<String>,
        sha: String,
    }

    let base = api_base.trim_end_matches('/');
    let url = format!(
        "{base}/repos/{owner}/{repo}/contents/{}?ref={}",
        path.trim_start_matches('/'),
        urlencoding::encode(git_ref)
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| ApiError::internal(e.to_string()))?;

    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header(reqwest::header::USER_AGENT, "meticulous-control-plane")
        .send()
        .await
        .map_err(|e| ApiError::bad_request(format!("GitHub contents request: {e}")))?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(ApiError::bad_request(format!(
            "GitHub contents API {}: {}",
            status.as_u16(),
            body.chars().take(400).collect::<String>()
        )));
    }

    let parsed: ContentResp = serde_json::from_str(&body)
        .map_err(|e| ApiError::bad_request(format!("GitHub JSON: {e}")))?;

    let encoding = parsed.encoding.as_deref().unwrap_or("");
    if encoding != "base64" {
        return Err(ApiError::bad_request(
            "unexpected GitHub contents encoding (expected base64 file)",
        ));
    }

    let b64 = parsed
        .content
        .ok_or_else(|| ApiError::bad_request("GitHub contents missing base64 body"))?
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.as_bytes())
        .map_err(|e| ApiError::bad_request(format!("base64 decode: {e}")))?;

    let text = String::from_utf8(bytes)
        .map_err(|e| ApiError::bad_request(format!("file is not valid UTF-8: {e}")))?;

    Ok((text, parsed.sha))
}

/// Download `tarball` for a ref and unpack to `dest`. Returns the single top-level directory.
pub async fn fetch_and_extract_tarball(
    token: &str,
    owner: &str,
    repo: &str,
    git_ref: &str,
    api_base: &str,
    dest: &Path,
) -> ApiResult<PathBuf> {
    let base = api_base.trim_end_matches('/');
    let url = format!(
        "{base}/repos/{owner}/{repo}/tarball/{}",
        urlencoding::encode(git_ref)
    );
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| ApiError::internal(e.to_string()))?;

    let resp = client
        .get(url)
        .header("Authorization", format!("Bearer {token}"))
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header(reqwest::header::USER_AGENT, "meticulous-control-plane")
        .send()
        .await
        .map_err(|e| ApiError::bad_request(format!("GitHub tarball request: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(ApiError::bad_request(format!(
            "GitHub tarball {}: {}",
            status.as_u16(),
            body.chars().take(400).collect::<String>()
        )));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| ApiError::bad_request(format!("read tarball: {e}")))?;

    std::fs::create_dir_all(dest).map_err(|e| ApiError::internal(e.to_string()))?;

    let decoder = GzDecoder::new(bytes.as_ref());
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(dest)
        .map_err(|e| ApiError::bad_request(format!("unpack tarball: {e}")))?;

    let mut read = std::fs::read_dir(dest).map_err(|e| ApiError::internal(e.to_string()))?;
    let first = read
        .next()
        .transpose()
        .map_err(|e| ApiError::internal(e.to_string()))?
        .ok_or_else(|| ApiError::bad_request("empty tarball"))?;

    Ok(first.path())
}

/// Load GitHub API base from encrypted credential row (for fetch before full token exchange).
pub fn github_api_base_hint_from_encrypted_row(
    crypto: &BuiltinStoredCrypto,
    row: &BuiltinSecretCipherRow,
) -> ApiResult<String> {
    let nonce: [u8; 12] = row
        .nonce
        .as_slice()
        .try_into()
        .map_err(|_| ApiError::bad_request("invalid secret nonce"))?;
    let pt = crypto
        .decrypt(&row.encrypted_value, &nonce)
        .map_err(|e| ApiError::internal(format!("decrypt: {e}")))?;
    let plaintext =
        String::from_utf8(pt.to_vec()).map_err(|e| ApiError::internal(format!("utf8: {e}")))?;
    Ok(api_base_from_env_inline_json(&plaintext))
}

/// Resolve API base for a project `github_app` secret without minting a token.
pub async fn github_api_base_for_credentials_path(
    pool: &PgPool,
    crypto: &BuiltinStoredCrypto,
    org_id: OrganizationId,
    project_id: ProjectId,
    credentials_path: &str,
    catalog_scm: bool,
) -> ApiResult<String> {
    let repo = BuiltinSecretsRepo::new(pool);
    let nil_pipe = PipelineId::from_uuid(uuid::Uuid::nil());
    let row = if catalog_scm {
        repo
            .get_current_cipher_row_for_catalog_scm(org_id, project_id, nil_pipe, credentials_path)
            .await
    } else {
        repo
            .get_current_cipher_row(org_id, project_id, nil_pipe, credentials_path)
            .await
    }
    .map_err(met_store::StoreError::from)?
        .ok_or_else(|| {
            ApiError::bad_request(format!(
                "stored secret '{credentials_path}' not found for this project"
            ))
        })?;
    github_api_base_hint_from_encrypted_row(crypto, &row)
}

pub fn yaml_file_to_json_value(yaml: &str) -> ApiResult<serde_json::Value> {
    let v: serde_yaml::Value =
        serde_yaml::from_str(yaml).map_err(|e| ApiError::bad_request(format!("YAML: {e}")))?;
    serde_json::to_value(&v).map_err(|e| ApiError::bad_request(format!("JSON: {e}")))
}

pub fn json_workflow_version(v: &serde_json::Value) -> Option<String> {
    if let Some(s) = v.as_str() {
        let t = s.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    v.as_u64().map(|n| n.to_string())
        .or_else(|| v.as_f64().map(|n| n.to_string()))
}

/// Copy `.stable/workflows/*.{yaml,yml}` from a Git checkout into `reusable_workflows` (project scope).
///
/// Trigger / import / sync-from-git already resolve `project/...` via [`GitWorkflowProvider`] on disk.
/// Scheduling hints and other DB-only parses use [`crate::scheduling_hints::try_parse_pipeline_ir`],
/// which only sees the database—this keeps both paths aligned.
pub async fn sync_project_workflows_from_stable_dir(
    pool: &PgPool,
    org_id: OrganizationId,
    project_id: ProjectId,
    repo_root: &Path,
) -> Result<(), StoreError> {
    let dir = repo_root.join(".stable/workflows");
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };

    let repo = WorkflowRepo::new(pool);
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "yaml" && ext != "yml" {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("workflow");
        let yaml_text = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "skipping unreadable workflow file");
                continue;
            }
        };
        let def = match yaml_file_to_json_value(&yaml_text) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "skipping invalid workflow YAML");
                continue;
            }
        };
        let name = def
            .get("name")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(stem)
            .to_string();
        let version = def
            .get("version")
            .and_then(json_workflow_version)
            .unwrap_or_else(|| "0.0.0".to_string());
        let description = def
            .get("description")
            .and_then(|v| v.as_str())
            .map(String::from);

        repo.upsert_project(
            org_id,
            project_id,
            &CreateWorkflow {
                name,
                version,
                definition: def,
                description,
                tags: Vec::new(),
            },
        )
        .await?;
    }

    Ok(())
}

/// Load pipeline YAML text from a GitHub tarball checkout (same resolution as full parse).
pub async fn fetch_pipeline_yaml_from_github_checkout(
    pool: &PgPool,
    crypto: &BuiltinStoredCrypto,
    org_id: OrganizationId,
    project_id: ProjectId,
    repository: &str,
    git_ref: &str,
    scm_path: &str,
    credentials_path: &str,
) -> ApiResult<String> {
    let slug = parse_github_repository(repository)?;
    let api_base =
        github_api_base_for_credentials_path(pool, crypto, org_id, project_id, credentials_path, false)
            .await?;
    let token = github_app_installation_token_for_project_secret(
        pool,
        crypto,
        org_id,
        project_id,
        credentials_path,
        false,
    )
    .await?;

    let tmp = tempfile::tempdir().map_err(|e| ApiError::internal(e.to_string()))?;
    let unpack_root = tmp.path().join("t");
    let repo_root = fetch_and_extract_tarball(
        &token,
        &slug.owner,
        &slug.name,
        git_ref,
        &api_base,
        &unpack_root,
    )
    .await?;

    read_pipeline_yaml_from_checkout(&repo_root, scm_path)
}

/// Parse pipeline IR from a GitHub tarball (global DB workflows + `project/` files on disk).
pub async fn parse_pipeline_from_github_checkout(
    pool: &PgPool,
    crypto: &BuiltinStoredCrypto,
    org_id: OrganizationId,
    project_id: ProjectId,
    repository: &str,
    git_ref: &str,
    scm_path: &str,
    credentials_path: &str,
) -> ApiResult<(PipelineIR, String, serde_json::Value)> {
    let slug = parse_github_repository(repository)?;
    let api_base =
        github_api_base_for_credentials_path(pool, crypto, org_id, project_id, credentials_path, false)
            .await?;
    let token = github_app_installation_token_for_project_secret(
        pool,
        crypto,
        org_id,
        project_id,
        credentials_path,
        false,
    )
    .await?;
    let commit_sha =
        resolve_github_commit_sha(&token, &slug.owner, &slug.name, git_ref, &api_base).await?;

    let tmp = tempfile::tempdir().map_err(|e| ApiError::internal(e.to_string()))?;
    let unpack_root = tmp.path().join("t");
    let repo_root = fetch_and_extract_tarball(
        &token,
        &slug.owner,
        &slug.name,
        git_ref,
        &api_base,
        &unpack_root,
    )
    .await?;

    let yaml_text = read_pipeline_yaml_from_checkout(&repo_root, scm_path)?;
    let def = yaml_file_to_json_value(&yaml_text)?;

    let db = DatabaseWorkflowProvider::new(pool.clone(), org_id.as_uuid());
    let git = GitWorkflowProvider::new(&repo_root, None);
    let composite = CompositeWorkflowProvider::new()
        .with_database(db)
        .with_git(git);
    let mut parser = PipelineParser::new(&composite);
    let ir = parser.parse(&yaml_text).await.map_err(|diags| {
        ApiError::bad_request(format!(
            "invalid pipeline definition: {}",
            diags
                .iter()
                .map(|d| d.message.clone())
                .collect::<Vec<_>>()
                .join("; ")
        ))
    })?;

    if let Err(e) =
        sync_project_workflows_from_stable_dir(pool, org_id, project_id, &repo_root).await
    {
        tracing::warn!(
            error = %e,
            "failed to sync .stable/workflows to reusable_workflows (run sync-from-git or trigger again after fix)"
        );
    }

    Ok((ir, commit_sha, def))
}

/// Read YAML from an on-disk checkout (e.g. tarball root).
///
/// If `path` has no `.yaml` / `.yml` suffix, also tries appending those extensions so
/// `.stable/demo-git-clone` resolves to `.stable/demo-git-clone.yaml`.
pub fn read_pipeline_yaml_from_checkout(root: &Path, path: &str) -> ApiResult<String> {
    let rel = path.trim().trim_start_matches('/');
    if rel.is_empty() {
        return Err(ApiError::bad_request(
            "pipeline path (scm_path) is required",
        ));
    }
    let mut candidates: Vec<PathBuf> = vec![root.join(rel)];
    if !rel.ends_with(".yaml") && !rel.ends_with(".yml") {
        candidates.push(root.join(format!("{rel}.yaml")));
        candidates.push(root.join(format!("{rel}.yml")));
    }
    let mut last_err: Option<std::io::Error> = None;
    for p in &candidates {
        match std::fs::File::open(p) {
            Ok(mut f) => {
                let mut s = String::new();
                f.read_to_string(&mut s).map_err(|e| {
                    ApiError::bad_request(format!("read pipeline file {}: {e}", p.display()))
                })?;
                return Ok(s);
            }
            Err(e) => last_err = Some(e),
        }
    }
    let tried = candidates
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Err(ApiError::bad_request(format!(
        "pipeline file not found (tried: {tried}): {}",
        last_err
            .map(|e| e.to_string())
            .unwrap_or_else(|| "unknown".into())
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_github_repository_owner_slash_name() {
        let s = parse_github_repository("org/workflow-demos").unwrap();
        assert_eq!(s.owner, "org");
        assert_eq!(s.name, "workflow-demos");
    }

    #[test]
    fn parse_github_repository_https_dot_git() {
        let s = parse_github_repository("https://github.com/acme/CI.git").unwrap();
        assert_eq!(s.owner, "acme");
        assert_eq!(s.name, "CI");
    }

    #[test]
    fn read_pipeline_yaml_from_nested_stable_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let nested = tmp.path().join(".stable");
        std::fs::create_dir_all(&nested).unwrap();
        let p = nested.join("demo.yaml");
        std::fs::write(&p, "name: x\ntriggers:\n  manual: {}\n").unwrap();
        let text = read_pipeline_yaml_from_checkout(tmp.path(), ".stable/demo.yaml").unwrap();
        assert!(text.contains("name: x"));
    }

    #[test]
    fn read_pipeline_yaml_infers_yaml_extension() {
        let tmp = tempfile::tempdir().unwrap();
        let nested = tmp.path().join(".stable");
        std::fs::create_dir_all(&nested).unwrap();
        let p = nested.join("demo-git-clone.yaml");
        std::fs::write(&p, "name: clone\ntriggers:\n  manual: {}\n").unwrap();
        let text = read_pipeline_yaml_from_checkout(tmp.path(), ".stable/demo-git-clone").unwrap();
        assert!(text.contains("name: clone"));
    }
}
