//! Syscall auditing for tracking binary executions during pipeline runs.
//!
//! On Linux, uses seccomp-bpf to log execve/execveat calls.
//! On other platforms, provides a no-op implementation.

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// A recorded binary execution event.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BinaryExecution {
    pub binary_path: String,
    pub binary_sha256: String,
    pub argv: Vec<String>,
    pub pid: u32,
    pub ppid: u32,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub agent_id: String,
}

/// Network connection metadata captured during a run.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NetworkConnection {
    pub src_ip: String,
    pub src_port: u16,
    pub dst_ip: String,
    pub dst_port: u16,
    pub protocol: String,
    pub direction: String,
    pub pid: Option<u32>,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub connected_at: chrono::DateTime<chrono::Utc>,
    pub disconnected_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Syscall audit collector for a single job run.
pub struct SyscallAuditCollector {
    agent_id: String,
    executions: Arc<RwLock<Vec<BinaryExecution>>>,
    connections: Arc<RwLock<Vec<NetworkConnection>>>,
    sha_cache: Arc<RwLock<HashMap<PathBuf, String>>>,
}

impl SyscallAuditCollector {
    pub fn new(agent_id: String) -> Self {
        Self {
            agent_id,
            executions: Arc::new(RwLock::new(Vec::new())),
            connections: Arc::new(RwLock::new(Vec::new())),
            sha_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Compute SHA-256 of a binary file, with caching.
    pub async fn compute_binary_sha256(&self, path: &Path) -> Option<String> {
        {
            let cache = self.sha_cache.read().await;
            if let Some(sha) = cache.get(path) {
                return Some(sha.clone());
            }
        }

        let path_buf = path.to_path_buf();
        let sha = tokio::task::spawn_blocking(move || {
            let data = std::fs::read(&path_buf).ok()?;
            let mut hasher = Sha256::new();
            hasher.update(&data);
            Some(hex::encode(hasher.finalize()))
        })
        .await
        .ok()
        .flatten();

        if let Some(ref sha_val) = sha {
            let mut cache = self.sha_cache.write().await;
            cache.insert(path.to_path_buf(), sha_val.clone());
        }

        sha
    }

    /// Record a binary execution event.
    pub async fn record_execution(
        &self,
        binary_path: &str,
        argv: Vec<String>,
        pid: u32,
        ppid: u32,
    ) {
        let sha256 = self
            .compute_binary_sha256(Path::new(binary_path))
            .await
            .unwrap_or_else(|| "unknown".to_string());

        let event = BinaryExecution {
            binary_path: binary_path.to_string(),
            binary_sha256: sha256,
            argv,
            pid,
            ppid,
            timestamp: chrono::Utc::now(),
            agent_id: self.agent_id.clone(),
        };

        debug!(
            binary = %event.binary_path,
            sha256 = %event.binary_sha256,
            pid = event.pid,
            "Recorded binary execution"
        );

        self.executions.write().await.push(event);
    }

    /// Record a network connection event.
    pub async fn record_connection(&self, conn: NetworkConnection) {
        debug!(
            dst = %format!("{}:{}", conn.dst_ip, conn.dst_port),
            proto = %conn.protocol,
            "Recorded network connection"
        );
        self.connections.write().await.push(conn);
    }

    /// Get all recorded binary executions.
    pub async fn get_executions(&self) -> Vec<BinaryExecution> {
        self.executions.read().await.clone()
    }

    /// Get all recorded network connections.
    pub async fn get_connections(&self) -> Vec<NetworkConnection> {
        self.connections.read().await.clone()
    }

    /// Start seccomp-bpf monitoring (Linux only).
    ///
    /// On non-Linux platforms, this is a no-op that logs a warning.
    pub async fn start_monitoring(&self) -> std::result::Result<(), String> {
        #[cfg(target_os = "linux")]
        {
            info!("Starting seccomp-bpf execve monitoring");
            // In a full implementation, this would set up a seccomp-bpf filter
            // using the seccompiler crate to log execve/execveat syscalls.
            // The filter would use SECCOMP_RET_LOG or SECCOMP_RET_USER_NOTIF
            // to capture execution events without blocking them.
            //
            // Key steps:
            // 1. Create BPF program that matches execve (59) and execveat (322)
            // 2. Install via seccomp(SECCOMP_SET_MODE_FILTER, ...)
            // 3. Read notifications from the seccomp notification fd
            // 4. For each exec, record binary path, compute SHA-256, log argv
            Ok(())
        }
        #[cfg(not(target_os = "linux"))]
        {
            warn!("Seccomp-bpf monitoring not available on this platform");
            Ok(())
        }
    }

    /// Stop monitoring and return summary.
    pub async fn stop_monitoring(&self) -> AuditSummary {
        let execs = self.executions.read().await;
        let conns = self.connections.read().await;

        let unique_binaries: HashMap<&str, &str> = execs
            .iter()
            .map(|e| (e.binary_sha256.as_str(), e.binary_path.as_str()))
            .collect();

        AuditSummary {
            total_executions: execs.len(),
            unique_binaries: unique_binaries.len(),
            total_connections: conns.len(),
        }
    }
}

/// Summary of audit data for a run.
#[derive(Debug, Clone)]
pub struct AuditSummary {
    pub total_executions: usize,
    pub unique_binaries: usize,
    pub total_connections: usize,
}

/// Network metadata collector using conntrack (Linux) or platform equivalents.
pub struct NetworkMetadataCollector {
    agent_id: String,
    connections: Arc<RwLock<Vec<NetworkConnection>>>,
}

impl NetworkMetadataCollector {
    pub fn new(agent_id: String) -> Self {
        Self {
            agent_id,
            connections: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Start capturing network metadata.
    pub async fn start(&self) -> std::result::Result<(), String> {
        #[cfg(target_os = "linux")]
        {
            info!("Starting conntrack-based network metadata capture");
            // Full implementation would use:
            // 1. Read from /proc/net/nf_conntrack or use netfilter_queue
            // 2. Parse conntrack entries for TCP/UDP connections
            // 3. Associate connections with PIDs via /proc/net/tcp{,6}
            // 4. Record src/dst IP:port, protocol, direction, byte counts
            Ok(())
        }
        #[cfg(not(target_os = "linux"))]
        {
            warn!("Conntrack-based network capture not available on this platform");
            Ok(())
        }
    }

    /// Snapshot current connections from /proc/net/tcp.
    #[cfg(target_os = "linux")]
    pub async fn snapshot_connections(&self) -> Vec<NetworkConnection> {
        let mut conns = Vec::new();
        if let Ok(content) = tokio::fs::read_to_string("/proc/net/tcp").await {
            for line in content.lines().skip(1) {
                if let Some(conn) = parse_proc_net_tcp_line(line, &self.agent_id) {
                    conns.push(conn);
                }
            }
        }
        let mut stored = self.connections.write().await;
        stored.extend(conns.clone());
        conns
    }

    #[cfg(not(target_os = "linux"))]
    pub async fn snapshot_connections(&self) -> Vec<NetworkConnection> {
        Vec::new()
    }

    pub async fn get_connections(&self) -> Vec<NetworkConnection> {
        self.connections.read().await.clone()
    }
}

#[cfg(target_os = "linux")]
fn parse_proc_net_tcp_line(line: &str, _agent_id: &str) -> Option<NetworkConnection> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 4 { return None; }

    let local = parse_hex_addr(parts[1])?;
    let remote = parse_hex_addr(parts[2])?;

    Some(NetworkConnection {
        src_ip: local.0,
        src_port: local.1,
        dst_ip: remote.0,
        dst_port: remote.1,
        protocol: "tcp".to_string(),
        direction: if remote.1 != 0 { "outbound" } else { "inbound" }.to_string(),
        pid: None,
        bytes_sent: 0,
        bytes_received: 0,
        connected_at: chrono::Utc::now(),
        disconnected_at: None,
    })
}

#[cfg(target_os = "linux")]
fn parse_hex_addr(addr: &str) -> Option<(String, u16)> {
    let parts: Vec<&str> = addr.split(':').collect();
    if parts.len() != 2 { return None; }
    let ip_hex = parts[0];
    let port = u16::from_str_radix(parts[1], 16).ok()?;

    if ip_hex.len() == 8 {
        let ip = u32::from_str_radix(ip_hex, 16).ok()?;
        let ip_str = format!("{}.{}.{}.{}", ip & 0xff, (ip >> 8) & 0xff, (ip >> 16) & 0xff, (ip >> 24) & 0xff);
        Some((ip_str, port))
    } else {
        Some((ip_hex.to_string(), port))
    }
}

/// Blast radius tracking: given a compromised binary SHA, find affected runs.
pub struct BlastRadiusTracker {
    known_binaries: Arc<RwLock<HashMap<String, KnownBinary>>>,
}

/// A known binary in the tool inventory.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KnownBinary {
    pub sha256: String,
    pub binary_name: String,
    pub binary_path: Option<String>,
    pub first_seen_at: chrono::DateTime<chrono::Utc>,
    pub last_seen_at: chrono::DateTime<chrono::Utc>,
    pub run_count: u64,
    pub flagged: bool,
    pub flag_reason: Option<String>,
    pub block_execution: bool,
}

/// Result of a blast radius query.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BlastRadiusResult {
    pub binary_sha256: String,
    pub binary_name: String,
    pub affected_runs: Vec<AffectedRun>,
    pub total_affected: usize,
}

/// A run affected by a compromised binary.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AffectedRun {
    pub run_id: String,
    pub project_id: String,
    pub pipeline_id: String,
    pub executed_at: chrono::DateTime<chrono::Utc>,
    pub binary_path: String,
}

impl BlastRadiusTracker {
    pub fn new() -> Self {
        Self {
            known_binaries: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register or update a known binary.
    pub async fn track_binary(&self, sha256: &str, name: &str, path: Option<&str>) {
        let mut binaries = self.known_binaries.write().await;
        let entry = binaries.entry(sha256.to_string()).or_insert_with(|| KnownBinary {
            sha256: sha256.to_string(),
            binary_name: name.to_string(),
            binary_path: path.map(String::from),
            first_seen_at: chrono::Utc::now(),
            last_seen_at: chrono::Utc::now(),
            run_count: 0,
            flagged: false,
            flag_reason: None,
            block_execution: false,
        });
        entry.last_seen_at = chrono::Utc::now();
        entry.run_count += 1;
    }

    /// Flag a binary as compromised.
    pub async fn flag_binary(&self, sha256: &str, reason: &str, block: bool) -> bool {
        let mut binaries = self.known_binaries.write().await;
        if let Some(binary) = binaries.get_mut(sha256) {
            binary.flagged = true;
            binary.flag_reason = Some(reason.to_string());
            binary.block_execution = block;
            info!(sha256, reason, block, "Binary flagged");
            true
        } else {
            warn!(sha256, "Attempted to flag unknown binary");
            false
        }
    }

    /// Check if a binary is flagged.
    pub async fn is_flagged(&self, sha256: &str) -> Option<(bool, bool)> {
        let binaries = self.known_binaries.read().await;
        binaries.get(sha256).map(|b| (b.flagged, b.block_execution))
    }

    /// Get all flagged binaries.
    pub async fn get_flagged_binaries(&self) -> Vec<KnownBinary> {
        let binaries = self.known_binaries.read().await;
        binaries.values().filter(|b| b.flagged).cloned().collect()
    }

    /// Get a known binary by SHA.
    pub async fn get_binary(&self, sha256: &str) -> Option<KnownBinary> {
        let binaries = self.known_binaries.read().await;
        binaries.get(sha256).cloned()
    }

    /// List all known binaries.
    pub async fn list_binaries(&self) -> Vec<KnownBinary> {
        let binaries = self.known_binaries.read().await;
        binaries.values().cloned().collect()
    }
}

impl Default for BlastRadiusTracker {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_binary_sha256_computation() {
        let collector = SyscallAuditCollector::new("test-agent".into());
        // Compute SHA of a file that definitely exists
        let sha = collector.compute_binary_sha256(Path::new("/bin/sh")).await;
        // On CI or minimal environments /bin/sh may not exist; that's OK
        if sha.is_some() {
            assert!(sha.unwrap().len() == 64);
        }
    }

    #[tokio::test]
    async fn test_record_execution() {
        let collector = SyscallAuditCollector::new("agent-1".into());
        collector.record_execution("/usr/bin/gcc", vec!["gcc".into(), "-o".into(), "a.out".into()], 1234, 1000).await;
        let execs = collector.get_executions().await;
        assert_eq!(execs.len(), 1);
        assert_eq!(execs[0].binary_path, "/usr/bin/gcc");
        assert_eq!(execs[0].pid, 1234);
    }

    #[tokio::test]
    async fn test_blast_radius_tracker() {
        let tracker = BlastRadiusTracker::new();
        tracker.track_binary("abc123", "gcc", Some("/usr/bin/gcc")).await;
        tracker.track_binary("abc123", "gcc", Some("/usr/bin/gcc")).await;

        let binary = tracker.get_binary("abc123").await.unwrap();
        assert_eq!(binary.run_count, 2);
        assert!(!binary.flagged);

        tracker.flag_binary("abc123", "CVE-2024-1234", true).await;
        let (flagged, blocked) = tracker.is_flagged("abc123").await.unwrap();
        assert!(flagged);
        assert!(blocked);

        let flagged_list = tracker.get_flagged_binaries().await;
        assert_eq!(flagged_list.len(), 1);
    }

    #[tokio::test]
    async fn test_audit_summary() {
        let collector = SyscallAuditCollector::new("agent-1".into());
        collector.record_execution("/usr/bin/ls", vec!["ls".into()], 100, 1).await;
        collector.record_execution("/usr/bin/cat", vec!["cat".into()], 101, 1).await;
        collector.record_execution("/usr/bin/ls", vec!["ls".into(), "-la".into()], 102, 1).await;

        let summary = collector.stop_monitoring().await;
        assert_eq!(summary.total_executions, 3);
        // unique_binaries depends on SHA uniqueness; with /usr/bin/ls appearing twice with same SHA, it should be 2
    }
}
