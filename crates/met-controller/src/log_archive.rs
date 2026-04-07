//! Finalize job logs: gzip JSONL → object store, then drop PostgreSQL cache rows.
//! `rehydrate_job_logs_from_store` loads archived logs back into the 24h PostgreSQL cache on demand.

use std::sync::Arc;

use bytes::Bytes;
use met_core::ids::{JobRunId, RunId, StepRunId};
use met_logging::{ArchivedLogLine, gunzip_jsonl, gzip_jsonl, job_run_archive_key};
use met_objstore::{ObjectKey, ObjectStore};
use met_store::PgPool;
use met_store::repos::{LazyCacheLine, LogCacheRepo, project_run_for_job_run};

use crate::error::ControllerError;
use sha2::{Digest, Sha256};
use tracing::{info, warn};

/// Upload logs to object storage and clear `log_cache`. If `object_store` is `None`, only
/// sets `expires_at` on cache rows so they fall off after 24 hours.
pub async fn finalize_job_logs(
    pool: &PgPool,
    object_store: Option<Arc<dyn ObjectStore + Send + Sync>>,
    job_run_id: JobRunId,
) {
    let cache = LogCacheRepo::new(pool);
    let lines = match cache.get_all_for_job_run(job_run_id).await {
        Ok(l) => l,
        Err(e) => {
            warn!(error = %e, job_run_id = %job_run_id, "failed to read log cache for archival");
            return;
        }
    };

    let Some(store) = object_store else {
        if let Err(e) = cache.touch_ttl_no_store(job_run_id).await {
            warn!(error = %e, job_run_id = %job_run_id, "failed to set log cache TTL without object store");
        }
        return;
    };

    let (run_id, project_id) = match project_run_for_job_run(pool, job_run_id).await {
        Ok(x) => x,
        Err(e) => {
            warn!(error = %e, job_run_id = %job_run_id, "could not resolve run/project for log archival");
            return;
        }
    };

    if lines.is_empty() {
        if let Err(e) = cache.delete_for_job_run(job_run_id).await {
            warn!(error = %e, job_run_id = %job_run_id, "failed to clear empty log cache");
        }
        return;
    }

    let archived: Vec<ArchivedLogLine> = lines
        .iter()
        .map(|e| ArchivedLogLine {
            sequence: e.sequence,
            timestamp: e.timestamp,
            stream: e.stream.clone(),
            content: e.content.clone(),
            run_id: e.run_id,
            step_run_id: e.step_run_id,
        })
        .collect();

    let payload = match gzip_jsonl(&archived) {
        Ok(p) => p,
        Err(e) => {
            warn!(error = %e, job_run_id = %job_run_id, "gzip jsonl failed");
            return;
        }
    };

    let checksum = hex::encode(Sha256::digest(&payload));
    let key_str = job_run_archive_key(project_id.as_uuid(), run_id.as_uuid(), job_run_id.as_uuid());
    let key = ObjectKey::new(&key_str);

    if let Err(e) = store
        .put_object_with_content_type(&key, Bytes::from(payload.clone()), "application/gzip")
        .await
    {
        warn!(error = %e, key = %key_str, "object store put for job logs failed");
        return;
    }

    if let Err(e) = cache
        .insert_archive(
            job_run_id,
            run_id,
            project_id,
            &key_str,
            archived.len() as i64,
            payload.len() as i64,
            true,
            Some(checksum.as_str()),
        )
        .await
    {
        warn!(error = %e, job_run_id = %job_run_id, "insert log_archives failed");
        return;
    }

    if let Err(e) = cache.delete_for_job_run(job_run_id).await {
        warn!(error = %e, job_run_id = %job_run_id, "delete log_cache after archive failed");
        return;
    }

    info!(
        job_run_id = %job_run_id,
        key = %key_str,
        lines = archived.len(),
        "archived job logs to object storage"
    );
}

/// If `log_cache` is empty but an archive row exists, download gzip JSONL from object storage and
/// repopulate the cache with `expires_at = now + 24h`.
pub async fn rehydrate_job_logs_from_store(
    pool: &PgPool,
    store: &(dyn ObjectStore + Send + Sync),
    job_run_id: JobRunId,
) -> Result<(), ControllerError> {
    let cache = LogCacheRepo::new(pool);
    if cache.count_for_job_run(job_run_id).await? > 0 {
        return Ok(());
    }

    let Some(archive) = cache.get_archive_by_job_run(job_run_id).await? else {
        return Ok(());
    };

    let key = ObjectKey::new(archive.storage_key.as_str());
    let body = store.get_object(&key).await.map_err(|e| {
        ControllerError::Internal(format!("object store get for log rehydrate: {e}"))
    })?;

    if let Some(expected) = archive.sha256_checksum.as_deref() {
        let actual = hex::encode(Sha256::digest(&body.body));
        if actual != expected {
            return Err(ControllerError::Internal(format!(
                "log archive checksum mismatch for job_run {job_run_id}"
            )));
        }
    }

    let lines = gunzip_jsonl(&body.body)
        .map_err(|e| ControllerError::Internal(format!("log archive decode: {e}")))?;

    let lazy: Vec<LazyCacheLine> = lines
        .into_iter()
        .map(|l| LazyCacheLine {
            job_run_id,
            run_id: RunId::from_uuid(l.run_id),
            step_run_id: l.step_run_id.map(StepRunId::from_uuid),
            sequence: l.sequence,
            stream: l.stream,
            content: l.content,
            timestamp: l.timestamp,
        })
        .collect();

    cache.bulk_insert_lazy(&lazy).await?;
    Ok(())
}
