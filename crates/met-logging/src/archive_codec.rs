//! Gzip-compressed JSONL encoding for durable log archives (SeaweedFS / S3).

use chrono::{DateTime, Utc};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use thiserror::Error;
use uuid::Uuid;

/// One line as stored in `.jsonl.gz` archives.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchivedLogLine {
    pub sequence: i64,
    pub timestamp: DateTime<Utc>,
    pub stream: String,
    pub content: String,
    pub run_id: Uuid,
    pub step_run_id: Option<Uuid>,
}

#[derive(Debug, Error)]
pub enum ArchiveCodecError {
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Build canonical object store key for a job run's log archive.
#[must_use]
pub fn job_run_archive_key(project_id: Uuid, run_id: Uuid, job_run_id: Uuid) -> String {
    format!("logs/{project_id}/{run_id}/{job_run_id}.jsonl.gz")
}

/// Serialize lines to gzip-compressed JSONL.
pub fn gzip_jsonl(lines: &[ArchivedLogLine]) -> Result<Vec<u8>, ArchiveCodecError> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    for line in lines {
        serde_json::to_writer(&mut encoder, line)?;
        encoder.write_all(b"\n")?;
    }
    Ok(encoder.finish()?)
}

/// Decode gzip JSONL payload from object storage.
pub fn gunzip_jsonl(bytes: &[u8]) -> Result<Vec<ArchivedLogLine>, ArchiveCodecError> {
    let mut decoder = GzDecoder::new(bytes);
    let mut buf = String::new();
    decoder.read_to_string(&mut buf)?;

    let mut out = Vec::new();
    for row in buf.lines() {
        if row.is_empty() {
            continue;
        }
        out.push(serde_json::from_str(row)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_archive() {
        let lines = vec![
            ArchivedLogLine {
                sequence: 1,
                timestamp: Utc::now(),
                stream: "stdout".into(),
                content: "hello".into(),
                run_id: Uuid::now_v7(),
                step_run_id: None,
            },
            ArchivedLogLine {
                sequence: 2,
                timestamp: Utc::now(),
                stream: "stderr".into(),
                content: "oops".into(),
                run_id: Uuid::now_v7(),
                step_run_id: Some(Uuid::now_v7()),
            },
        ];

        let gz = gzip_jsonl(&lines).unwrap();
        let back = gunzip_jsonl(&gz).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].content, "hello");
        assert_eq!(back[1].content, "oops");
    }
}
