//! Process watcher for tracking child process spawns and computing binary checksums.
//!
//! This module provides functionality to:
//! - Track all child processes spawned during step execution
//! - Compute SHA256 checksums of executed binaries
//! - Report execution metadata for security auditing

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::sync::RwLock;
use tracing::{debug, trace};

use crate::error::{AgentError, Result};

/// Information about an executed binary.
#[derive(Debug, Clone)]
pub struct ExecutedBinary {
    /// Absolute path to the binary.
    pub path: PathBuf,
    /// SHA-256 checksum of the binary (hex-encoded).
    pub sha256: String,
    /// Process ID.
    pub pid: u32,
    /// Parent process ID.
    pub parent_pid: u32,
    /// When the process started.
    pub started_at: DateTime<Utc>,
    /// When the process ended (if finished).
    pub ended_at: Option<DateTime<Utc>>,
    /// Exit code (if finished).
    pub exit_code: Option<i32>,
}

/// Aggregated record for a unique binary across multiple executions.
#[derive(Debug, Clone)]
pub struct ExecutedBinaryRecord {
    /// Absolute path to the binary.
    pub path: String,
    /// SHA-256 checksum of the binary (hex-encoded).
    pub sha256: String,
    /// Number of times this binary was executed.
    pub execution_count: u32,
    /// When first executed.
    pub first_executed_at: DateTime<Utc>,
    /// When last executed.
    pub last_executed_at: DateTime<Utc>,
    /// Step IDs where this binary was executed.
    pub step_ids: Vec<String>,
}

/// Job execution metadata summary.
#[derive(Debug, Clone, Default)]
pub struct JobExecutionMetadata {
    /// All executed binaries (aggregated by path+sha256).
    pub executed_binaries: Vec<ExecutedBinaryRecord>,
    /// Total number of processes spawned.
    pub total_processes_spawned: u64,
    /// Maximum depth of the process tree.
    pub execution_tree_depth: u32,
}

/// Cache key for binary checksums.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct BinaryCacheKey {
    path: PathBuf,
    size: u64,
    modified: SystemTime,
}

/// A tracked process during execution.
#[derive(Debug)]
struct TrackedProcess {
    pid: u32,
    parent_pid: u32,
    exe_path: PathBuf,
    exe_sha256: String,
    started_at: DateTime<Utc>,
    depth: u32,
}

/// Process watcher for tracking child processes during step execution.
pub struct ProcessWatcher {
    /// Root process ID to watch.
    root_pid: Option<u32>,
    /// All tracked processes.
    tracked_processes: Arc<RwLock<Vec<TrackedProcess>>>,
    /// Cache of binary checksums: (path, size, mtime) -> sha256.
    checksum_cache: Arc<RwLock<HashMap<BinaryCacheKey, String>>>,
    /// Current step ID.
    current_step_id: Arc<RwLock<Option<String>>>,
    /// Whether watching is active.
    active: Arc<RwLock<bool>>,
    /// Polling interval (reserved for future eBPF-based implementation).
    #[allow(dead_code)]
    poll_interval: Duration,
}

impl ProcessWatcher {
    /// Create a new process watcher.
    pub fn new() -> Self {
        Self {
            root_pid: None,
            tracked_processes: Arc::new(RwLock::new(Vec::new())),
            checksum_cache: Arc::new(RwLock::new(HashMap::new())),
            current_step_id: Arc::new(RwLock::new(None)),
            active: Arc::new(RwLock::new(false)),
            poll_interval: Duration::from_millis(100),
        }
    }

    /// Start watching a process and its children.
    pub async fn start_watching(&mut self, pid: u32, step_id: &str) -> Result<()> {
        self.root_pid = Some(pid);
        *self.current_step_id.write().await = Some(step_id.to_string());
        *self.active.write().await = true;

        debug!(pid, step_id, "started process watching");

        Ok(())
    }

    /// Stop watching and return any remaining untracked processes.
    pub async fn stop_watching(&mut self) {
        *self.active.write().await = false;
        *self.current_step_id.write().await = None;
        self.root_pid = None;
        debug!("stopped process watching");
    }

    /// Poll for new child processes and track them.
    /// Call this periodically while the step is running.
    pub async fn poll(&self) -> Result<Vec<ExecutedBinary>> {
        let active = *self.active.read().await;
        if !active {
            return Ok(Vec::new());
        }

        let root_pid = match self.root_pid {
            Some(pid) => pid,
            None => return Ok(Vec::new()),
        };

        let mut new_binaries = Vec::new();

        // Get all descendant processes
        let descendants = self.get_descendant_pids(root_pid).await;
        let mut tracked = self.tracked_processes.write().await;
        let tracked_pids: std::collections::HashSet<u32> =
            tracked.iter().map(|p| p.pid).collect();

        for (pid, parent_pid, depth) in descendants {
            if tracked_pids.contains(&pid) {
                continue;
            }

            // Get executable path for this process
            if let Some(exe_path) = self.get_process_exe(pid).await {
                // Compute or get cached SHA256
                let sha256 = match self.compute_or_get_cached_sha256(&exe_path).await {
                    Ok(hash) => hash,
                    Err(e) => {
                        trace!(pid, path = %exe_path.display(), error = %e, "failed to compute sha256");
                        continue;
                    }
                };

                let now = Utc::now();
                let binary = ExecutedBinary {
                    path: exe_path.clone(),
                    sha256: sha256.clone(),
                    pid,
                    parent_pid,
                    started_at: now,
                    ended_at: None,
                    exit_code: None,
                };

                new_binaries.push(binary);

                tracked.push(TrackedProcess {
                    pid,
                    parent_pid,
                    exe_path,
                    exe_sha256: sha256,
                    started_at: now,
                    depth,
                });
            }
        }

        Ok(new_binaries)
    }

    /// Get all executed binaries tracked so far.
    pub async fn get_executed_binaries(&self) -> Vec<ExecutedBinary> {
        let tracked = self.tracked_processes.read().await;
        tracked
            .iter()
            .map(|p| ExecutedBinary {
                path: p.exe_path.clone(),
                sha256: p.exe_sha256.clone(),
                pid: p.pid,
                parent_pid: p.parent_pid,
                started_at: p.started_at,
                ended_at: None,
                exit_code: None,
            })
            .collect()
    }

    /// Aggregate execution metadata for a job.
    pub async fn aggregate_metadata(&self, step_id: &str) -> JobExecutionMetadata {
        let tracked = self.tracked_processes.read().await;

        // Group by (path, sha256)
        let mut by_binary: HashMap<(String, String), ExecutedBinaryRecord> = HashMap::new();
        let mut max_depth = 0u32;

        for process in tracked.iter() {
            max_depth = max_depth.max(process.depth);

            let key = (
                process.exe_path.to_string_lossy().to_string(),
                process.exe_sha256.clone(),
            );

            by_binary
                .entry(key)
                .and_modify(|record| {
                    record.execution_count += 1;
                    if process.started_at < record.first_executed_at {
                        record.first_executed_at = process.started_at;
                    }
                    if process.started_at > record.last_executed_at {
                        record.last_executed_at = process.started_at;
                    }
                    if !record.step_ids.contains(&step_id.to_string()) {
                        record.step_ids.push(step_id.to_string());
                    }
                })
                .or_insert_with(|| ExecutedBinaryRecord {
                    path: process.exe_path.to_string_lossy().to_string(),
                    sha256: process.exe_sha256.clone(),
                    execution_count: 1,
                    first_executed_at: process.started_at,
                    last_executed_at: process.started_at,
                    step_ids: vec![step_id.to_string()],
                });
        }

        JobExecutionMetadata {
            executed_binaries: by_binary.into_values().collect(),
            total_processes_spawned: tracked.len() as u64,
            execution_tree_depth: max_depth,
        }
    }

    /// Clear all tracked processes (call between jobs).
    pub async fn clear(&mut self) {
        self.tracked_processes.write().await.clear();
        self.root_pid = None;
        *self.active.write().await = false;
        *self.current_step_id.write().await = None;
    }

    /// Compute SHA-256 checksum, using cache if available.
    async fn compute_or_get_cached_sha256(&self, path: &Path) -> Result<String> {
        // Get file metadata for cache key
        let metadata = tokio::fs::metadata(path).await.map_err(|e| {
            AgentError::Internal(format!("failed to get metadata for {}: {}", path.display(), e))
        })?;

        let cache_key = BinaryCacheKey {
            path: path.to_path_buf(),
            size: metadata.len(),
            modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        };

        // Check cache
        {
            let cache = self.checksum_cache.read().await;
            if let Some(cached) = cache.get(&cache_key) {
                trace!(path = %path.display(), "using cached sha256");
                return Ok(cached.clone());
            }
        }

        // Compute SHA256
        let sha256 = compute_file_sha256(path).await?;

        // Store in cache
        {
            let mut cache = self.checksum_cache.write().await;
            cache.insert(cache_key, sha256.clone());
        }

        debug!(path = %path.display(), sha256 = %sha256, "computed binary sha256");

        Ok(sha256)
    }

    /// Get all descendant PIDs of a process.
    #[cfg(target_os = "linux")]
    async fn get_descendant_pids(&self, root_pid: u32) -> Vec<(u32, u32, u32)> {
        let mut result = Vec::new();
        let mut to_visit = vec![(root_pid, 0u32)]; // (pid, depth)

        while let Some((pid, depth)) = to_visit.pop() {
            // Read /proc/{pid}/task/{tid}/children for each thread
            let task_path = format!("/proc/{}/task", pid);
            if let Ok(mut entries) = tokio::fs::read_dir(&task_path).await {
                while let Ok(Some(entry)) = entries.next_entry().await {
                    let children_path = entry.path().join("children");
                    if let Ok(children_str) = tokio::fs::read_to_string(&children_path).await {
                        for child_pid_str in children_str.split_whitespace() {
                            if let Ok(child_pid) = child_pid_str.parse::<u32>() {
                                result.push((child_pid, pid, depth + 1));
                                to_visit.push((child_pid, depth + 1));
                            }
                        }
                    }
                }
            }
        }

        result
    }

    #[cfg(not(target_os = "linux"))]
    async fn get_descendant_pids(&self, _root_pid: u32) -> Vec<(u32, u32, u32)> {
        // On non-Linux platforms, we can't easily enumerate child processes
        // without platform-specific APIs. Return empty for now.
        // TODO: Implement for macOS (sysctl) and Windows (CreateToolhelp32Snapshot)
        Vec::new()
    }

    /// Get the executable path for a process.
    #[cfg(target_os = "linux")]
    async fn get_process_exe(&self, pid: u32) -> Option<PathBuf> {
        let exe_link = format!("/proc/{}/exe", pid);
        match tokio::fs::read_link(&exe_link).await {
            Ok(path) => {
                // Filter out kernel threads and special entries
                if path.to_string_lossy().contains(" (deleted)")
                    || !path.exists()
                {
                    None
                } else {
                    Some(path)
                }
            }
            Err(_) => None,
        }
    }

    #[cfg(not(target_os = "linux"))]
    async fn get_process_exe(&self, _pid: u32) -> Option<PathBuf> {
        // TODO: Implement for macOS and Windows
        None
    }
}

impl Default for ProcessWatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute SHA-256 checksum of a file.
pub async fn compute_file_sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path).await.map_err(|e| {
        AgentError::Internal(format!("failed to open file {}: {}", path.display(), e))
    })?;

    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536]; // 64KB buffer

    loop {
        let n = file.read(&mut buffer).await.map_err(|e| {
            AgentError::Internal(format!("failed to read file {}: {}", path.display(), e))
        })?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    Ok(hex::encode(hasher.finalize()))
}

/// Merge execution metadata from multiple steps into a single job-level summary.
pub fn merge_execution_metadata(
    step_metadata: Vec<(String, JobExecutionMetadata)>,
) -> JobExecutionMetadata {
    let mut by_binary: HashMap<(String, String), ExecutedBinaryRecord> = HashMap::new();
    let mut total_processes = 0u64;
    let mut max_depth = 0u32;

    for (step_id, metadata) in step_metadata {
        total_processes += metadata.total_processes_spawned;
        max_depth = max_depth.max(metadata.execution_tree_depth);

        for binary in metadata.executed_binaries {
            let key = (binary.path.clone(), binary.sha256.clone());

            by_binary
                .entry(key)
                .and_modify(|record| {
                    record.execution_count += binary.execution_count;
                    if binary.first_executed_at < record.first_executed_at {
                        record.first_executed_at = binary.first_executed_at;
                    }
                    if binary.last_executed_at > record.last_executed_at {
                        record.last_executed_at = binary.last_executed_at;
                    }
                    for sid in &binary.step_ids {
                        if !record.step_ids.contains(sid) {
                            record.step_ids.push(sid.clone());
                        }
                    }
                    if !record.step_ids.contains(&step_id) {
                        record.step_ids.push(step_id.clone());
                    }
                })
                .or_insert(binary);
        }
    }

    JobExecutionMetadata {
        executed_binaries: by_binary.into_values().collect(),
        total_processes_spawned: total_processes,
        execution_tree_depth: max_depth,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_compute_file_sha256() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"hello world").unwrap();
        file.flush().unwrap();

        let hash = compute_file_sha256(file.path()).await.unwrap();
        // SHA256 of "hello world"
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[tokio::test]
    async fn test_process_watcher_creation() {
        let watcher = ProcessWatcher::new();
        assert!(watcher.root_pid.is_none());
    }

    #[tokio::test]
    async fn test_merge_execution_metadata() {
        let step1 = JobExecutionMetadata {
            executed_binaries: vec![ExecutedBinaryRecord {
                path: "/bin/sh".to_string(),
                sha256: "abc123".to_string(),
                execution_count: 1,
                first_executed_at: Utc::now(),
                last_executed_at: Utc::now(),
                step_ids: vec!["step1".to_string()],
            }],
            total_processes_spawned: 1,
            execution_tree_depth: 1,
        };

        let step2 = JobExecutionMetadata {
            executed_binaries: vec![
                ExecutedBinaryRecord {
                    path: "/bin/sh".to_string(),
                    sha256: "abc123".to_string(),
                    execution_count: 2,
                    first_executed_at: Utc::now(),
                    last_executed_at: Utc::now(),
                    step_ids: vec!["step2".to_string()],
                },
                ExecutedBinaryRecord {
                    path: "/bin/ls".to_string(),
                    sha256: "def456".to_string(),
                    execution_count: 1,
                    first_executed_at: Utc::now(),
                    last_executed_at: Utc::now(),
                    step_ids: vec!["step2".to_string()],
                },
            ],
            total_processes_spawned: 3,
            execution_tree_depth: 2,
        };

        let merged = merge_execution_metadata(vec![
            ("step1".to_string(), step1),
            ("step2".to_string(), step2),
        ]);

        assert_eq!(merged.total_processes_spawned, 4);
        assert_eq!(merged.execution_tree_depth, 2);
        assert_eq!(merged.executed_binaries.len(), 2);

        // Find the /bin/sh entry
        let sh_entry = merged
            .executed_binaries
            .iter()
            .find(|b| b.path == "/bin/sh")
            .unwrap();
        assert_eq!(sh_entry.execution_count, 3);
        assert!(sh_entry.step_ids.contains(&"step1".to_string()));
        assert!(sh_entry.step_ids.contains(&"step2".to_string()));
    }
}
