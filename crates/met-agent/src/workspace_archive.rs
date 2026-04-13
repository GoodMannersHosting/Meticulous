//! Passive workspace snapshot: gitignore-aware `tar.zstd`, presigned GET/PUT, safe extract.

use std::path::{Component, Path};

use met_proto::controller::v1::{WorkspaceSnapshot, WorkspaceSnapshotUploadSpec};
use reqwest::Client;
use sha2::{Digest, Sha256};
use tracing::{debug, warn};

use crate::error::{AgentError, Result};

const DEFAULT_HTTP_TIMEOUT_SECS: u64 = 3600;

fn http_client() -> Result<Client> {
    Client::builder()
        .timeout(std::time::Duration::from_secs(DEFAULT_HTTP_TIMEOUT_SECS))
        .build()
        .map_err(|e| AgentError::Workspace(format!("HTTP client: {e}")))
}

/// Download and extract a workspace snapshot into `workspace` (empty or partial tree).
pub async fn restore_workspace(workspace: &Path, snap: &WorkspaceSnapshot) -> Result<()> {
    if snap.snapshot_download_url.is_empty() {
        return Ok(());
    }

    let client = http_client()?;
    let resp = client
        .get(&snap.snapshot_download_url)
        .send()
        .await
        .map_err(|e| AgentError::Workspace(format!("snapshot download: {e}")))?;
    if !resp.status().is_success() {
        return Err(AgentError::Workspace(format!(
            "snapshot download HTTP {}",
            resp.status()
        )));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AgentError::Workspace(format!("snapshot read body: {e}")))?;

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let digest = hex::encode(hasher.finalize());
    let expected = snap.expected_sha256.trim();
    if !expected.is_empty() && digest != expected {
        return Err(AgentError::Workspace(format!(
            "snapshot digest mismatch: got {digest} expected {expected}"
        )));
    }

    let workspace = workspace.to_path_buf();
    let data = bytes.to_vec();
    tokio::task::spawn_blocking(move || unpack_tar_zst(&workspace, &data))
        .await
        .map_err(|e| AgentError::Workspace(format!("unpack join: {e}")))?
}

fn normalized_include_prefixes(paths: &[String]) -> Vec<String> {
    paths
        .iter()
        .map(|p| {
            p.trim()
                .trim_start_matches("./")
                .replace('\\', "/")
                .trim_end_matches('/')
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

fn rel_matches_any_prefix(rel: &str, prefixes: &[String]) -> bool {
    let rel = rel.replace('\\', "/");
    prefixes
        .iter()
        .any(|p| rel == *p || rel.starts_with(&format!("{p}/")))
}

fn unpack_tar_zst(workspace: &Path, compressed: &[u8]) -> Result<()> {
    let dec = zstd::decode_all(compressed)
        .map_err(|e| AgentError::Workspace(format!("zstd decompress: {e}")))?;
    let mut archive = tar::Archive::new(std::io::Cursor::new(dec));
    for entry in archive
        .entries()
        .map_err(|e| AgentError::Workspace(e.to_string()))?
    {
        let mut entry = entry.map_err(|e| AgentError::Workspace(e.to_string()))?;
        let path = entry
            .path()
            .map_err(|e| AgentError::Workspace(e.to_string()))?
            .to_path_buf();
        if path.is_absolute() || path.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(AgentError::Workspace(format!(
                "unsafe path in archive: {}",
                path.display()
            )));
        }
        entry
            .unpack_in(workspace)
            .map_err(|e| AgentError::Workspace(format!("unpack_in: {e}")))?;
    }
    Ok(())
}

fn pack_blocking(
    workspace: &Path,
    max_uncompressed: u64,
    include_paths: &[String],
) -> Result<Vec<u8>> {
    let prefixes = normalized_include_prefixes(include_paths);
    let filter_by_prefix = !prefixes.is_empty();

    let mut builder = ignore::WalkBuilder::new(workspace);
    builder.standard_filters(true);
    builder.hidden(false);
    let walk = builder.build();

    let buf = Vec::new();
    let enc = zstd::stream::Encoder::new(buf, 3)
        .map_err(|e| AgentError::Workspace(format!("zstd encoder: {e}")))?;
    let mut tar = tar::Builder::new(enc);
    tar.follow_symlinks(false);

    let mut total: u64 = 0;

    for entry in walk {
        let entry = entry.map_err(|e| AgentError::Workspace(e.to_string()))?;
        let path = entry.path();
        let rel = path
            .strip_prefix(workspace)
            .map_err(|e| AgentError::Workspace(format!("strip_prefix {}: {e}", path.display())))?;

        if rel.as_os_str().is_empty() {
            continue;
        }

        let rel_str = rel.to_string_lossy();
        if filter_by_prefix && !rel_matches_any_prefix(&rel_str, &prefixes) {
            continue;
        }
        if rel_str == ".git" || rel_str.starts_with(".git/") {
            continue;
        }

        let meta = entry
            .metadata()
            .map_err(|e| AgentError::Workspace(e.to_string()))?;
        if meta.is_dir() {
            continue;
        }
        if !meta.is_file() {
            debug!(path = %path.display(), "skipping non-file workspace entry for snapshot");
            continue;
        }

        let size = meta.len();
        total = total.saturating_add(size);
        if max_uncompressed < u64::MAX && total > max_uncompressed {
            return Err(AgentError::Workspace(format!(
                "workspace snapshot exceeds uncompressed cap ({max_uncompressed} bytes)"
            )));
        }

        tar.append_path_with_name(path, rel)
            .map_err(|e| AgentError::Workspace(format!("tar append {}: {e}", rel.display())))?;
    }

    tar.into_inner()
        .map_err(|e| AgentError::Workspace(format!("tar into_inner: {e}")))?
        .finish()
        .map_err(|e| AgentError::Workspace(format!("zstd finish: {e}")))
}

/// Upload snapshot bytes via presigned PUT.
pub async fn put_snapshot(url: &str, body: &[u8]) -> Result<()> {
    let client = http_client()?;
    let resp = client
        .put(url)
        .header(reqwest::header::CONTENT_TYPE, "application/zstd")
        .body(body.to_vec())
        .send()
        .await
        .map_err(|e| AgentError::Workspace(format!("snapshot upload: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        warn!(%status, body = %text, "snapshot PUT failed");
        return Err(AgentError::Workspace(format!(
            "snapshot upload HTTP {status}"
        )));
    }
    Ok(())
}

/// Pack, optionally enforce max size on compressed output, upload, return protobuf result.
pub async fn snapshot_and_upload(
    workspace: &Path,
    spec: &WorkspaceSnapshotUploadSpec,
) -> Result<met_proto::controller::v1::WorkspaceSnapshotUploadResult> {
    if spec.snapshot_upload_url.is_empty() || spec.object_key.is_empty() {
        return Ok(met_proto::controller::v1::WorkspaceSnapshotUploadResult {
            skipped: true,
            uploaded: false,
            ..Default::default()
        });
    }

    let include_paths: Vec<String> = spec.include_paths.iter().cloned().collect();

    let (compressed, sha256) = tokio::task::spawn_blocking({
        let root = workspace.to_path_buf();
        let max = spec.max_bytes;
        move || {
            let max_u = if max <= 0 { u64::MAX } else { max as u64 };
            let raw = pack_blocking(&root, max_u, &include_paths)?;
            let mut hasher = Sha256::new();
            hasher.update(&raw);
            let digest = hex::encode(hasher.finalize());
            Ok::<_, AgentError>((raw, digest))
        }
    })
    .await
    .map_err(|e| AgentError::Workspace(format!("pack join: {e}")))??;

    if spec.max_bytes > 0 && compressed.len() as i64 > spec.max_bytes {
        return Ok(met_proto::controller::v1::WorkspaceSnapshotUploadResult {
            uploaded: false,
            error_message: format!(
                "compressed snapshot {} bytes exceeds max_bytes {}",
                compressed.len(),
                spec.max_bytes
            ),
            object_key: spec.object_key.clone(),
            ..Default::default()
        });
    }

    put_snapshot(&spec.snapshot_upload_url, &compressed).await?;

    Ok(met_proto::controller::v1::WorkspaceSnapshotUploadResult {
        skipped: false,
        uploaded: true,
        sha256,
        size_bytes: compressed.len() as i64,
        object_key: spec.object_key.clone(),
        ..Default::default()
    })
}
