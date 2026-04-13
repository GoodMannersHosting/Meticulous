//! Best-effort TCP flow snapshots from Linux `/proc/net/tcp*`, correlated with watched PIDs.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader};

use met_core::proc_net_tcp::parse_hex_ip_port;
use met_proto::agent::v1::LogStream;
use serde_json::json;

use super::redact_path;
use crate::error::Result;
use crate::process_watcher::ProcessWatcher;
use crate::step_log::StepLogPipe;

const TCP_ESTABLISHED: &str = "01";

#[derive(Debug)]
struct ParsedRow {
    local_ip: String,
    local_port: u16,
    remote_ip: String,
    remote_port: u16,
    inode: u64,
}

fn read_proc_tcp(path: &str) -> std::io::Result<Vec<ParsedRow>> {
    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut out = Vec::new();
    for (i, line) in reader.lines().enumerate() {
        let line = line?;
        if i == 0 {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 10 {
            continue;
        }
        if parts[3] != TCP_ESTABLISHED {
            continue;
        }
        let Some((lip, lp)) = parse_hex_ip_port(parts[1]) else {
            continue;
        };
        let Some((rip, rp)) = parse_hex_ip_port(parts[2]) else {
            continue;
        };
        if rp == 0 {
            continue;
        }
        let Ok(inode) = parts[9].parse::<u64>() else {
            continue;
        };
        out.push(ParsedRow {
            local_ip: lip,
            local_port: lp,
            remote_ip: rip,
            remote_port: rp,
            inode,
        });
    }
    Ok(out)
}

fn inode_to_pid(tracked_pids: &HashSet<u32>) -> HashMap<u64, u32> {
    let mut map = HashMap::new();
    for &pid in tracked_pids {
        let fd_dir = format!("/proc/{pid}/fd");
        let Ok(entries) = fs::read_dir(&fd_dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(target) = fs::read_link(entry.path()) else {
                continue;
            };
            let t = target.to_string_lossy();
            let Some(rest) = t.strip_prefix("socket:[") else {
                continue;
            };
            let Some(num) = rest.strip_suffix(']') else {
                continue;
            };
            if let Ok(inode) = num.parse::<u64>() {
                map.entry(inode).or_insert(pid);
            }
        }
    }
    map
}

fn flow_direction(local_port: u16, remote_port: u16) -> &'static str {
    // Ephemeral local port with a well-known / registered remote service port → typical outbound client.
    if local_port >= 32768 && remote_port < 32768 {
        return "outbound";
    }
    if remote_port >= 32768 && local_port < 32768 {
        return "inbound";
    }
    "observed"
}

pub async fn emit_new_network_flows(
    pipe: &StepLogPipe,
    step_sequence: i32,
    watcher: &ProcessWatcher,
) -> Result<()> {
    if !watcher.is_watching_active().await {
        return Ok(());
    }

    let exe_by_pid = watcher.tracked_pid_exe_map().await;
    let mut tracked_pids: HashSet<u32> = exe_by_pid.keys().copied().collect();
    if let Some(r) = watcher.watch_root_pid() {
        tracked_pids.insert(r);
    }
    if tracked_pids.is_empty() {
        return Ok(());
    }

    let inode_pid = inode_to_pid(&tracked_pids);

    let mut rows = read_proc_tcp("/proc/net/tcp").unwrap_or_default();
    if let Ok(r6) = read_proc_tcp("/proc/net/tcp6") {
        rows.extend(r6);
    }

    for row in rows {
        let Some(pid) = inode_pid.get(&row.inode).copied() else {
            continue;
        };
        let dedupe_key = format!(
            "{}|{}|{}|{}|{}",
            pid, row.local_ip, row.local_port, row.remote_ip, row.remote_port
        );
        if !watcher.net_flow_key_insert_if_new(dedupe_key).await {
            continue;
        }

        let (binary_path, binary_sha256) = if let Some((p, s)) = exe_by_pid.get(&pid) {
            (Some(redact_path(&p.to_string_lossy())), Some(s.as_str()))
        } else {
            (None, None)
        };

        let direction = flow_direction(row.local_port, row.remote_port);
        let v = json!({
            "step_sequence": step_sequence,
            "pid": pid,
            "protocol": "tcp",
            "direction": direction,
            "src_ip": row.local_ip,
            "src_port": row.local_port,
            "dst_ip": row.remote_ip,
            "dst_port": row.remote_port,
            "observed_via": "proc_net_tcp",
            "binary_path": binary_path,
            "binary_sha256": binary_sha256,
        });

        pipe.send_telemetry(LogStream::NetworkFlow, &v.to_string())
            .await?;
    }

    Ok(())
}
