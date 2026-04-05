//! Security bundle collection and per-job PKI.

use std::process::Command;
use std::time::Duration;

use if_addrs::IfAddr;
use met_secrets::pki::{EncryptedEnvelope, HybridDecryption};
use rand::rngs::OsRng;
use rcgen::KeyPair;
use sysinfo::System;
use tracing::{debug, warn};
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret as X25519StaticSecret};
use zeroize::Zeroizing;

/// Map Rust target arch names to common CI labels (`amd64` / `arm64`).
#[must_use]
pub fn normalize_arch(arch: &str) -> String {
    match arch {
        "x86_64" => "amd64".to_string(),
        "aarch64" => "arm64".to_string(),
        a => a.to_string(),
    }
}

/// Collected security bundle for registration.
#[derive(Debug)]
pub struct CollectedSecurityBundle {
    pub hostname: String,
    pub os: String,
    pub arch: String,
    pub kernel_version: String,
    pub public_ips: Vec<String>,
    pub private_ips: Vec<String>,
    pub ntp_synchronized: bool,
    pub container_runtime: String,
    pub container_runtime_version: String,
    pub environment_type: EnvironmentType,
    pub x509_public_key: Vec<u8>,
    /// Stable machine identifier where available (e.g. Linux machine-id, macOS serial).
    pub machine_id: String,
    pub logical_cpus: u32,
    pub memory_total_bytes: u64,
    /// Outbound public IP (egress), when discoverable.
    pub egress_public_ip: String,
}

/// Environment type.
#[derive(Debug, Clone, Copy)]
#[repr(i32)]
pub enum EnvironmentType {
    Unspecified = 0,
    Physical = 1,
    Virtual = 2,
    Container = 3,
}

/// Collector for security bundle information.
pub struct SecurityBundleCollector {
    key_pair: Option<KeyPair>,
}

impl SecurityBundleCollector {
    /// Create a new collector.
    pub fn new() -> Self {
        Self { key_pair: None }
    }

    /// Collect the security bundle.
    pub async fn collect(&self) -> CollectedSecurityBundle {
        let hostname = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "unknown".to_string());

        let os = std::env::consts::OS.to_string();
        let arch = normalize_arch(std::env::consts::ARCH);
        let machine_id = read_machine_id();
        let kernel_version = self.get_kernel_version();
        let (logical_cpus, memory_total_bytes) = self.sysinfo_resources();
        let (public_from_iface, mut private_ips) = collect_interface_ips();
        let egress_public_ip = fetch_egress_public_ip().await;

        let mut public_ips = Vec::new();
        if !egress_public_ip.is_empty() {
            public_ips.push(egress_public_ip.clone());
        }
        for p in public_from_iface {
            if !public_ips.contains(&p) {
                public_ips.push(p);
            }
        }

        private_ips.sort();
        private_ips.dedup();

        let ntp_synchronized = self.check_ntp_sync();
        let (container_runtime, container_runtime_version) = self.detect_container_runtime();
        let environment_type = self.detect_environment_type();

        // Generate long-term identity keypair
        let key_pair = KeyPair::generate().expect("failed to generate key pair");
        let x509_public_key = key_pair.public_key_der().to_vec();

        CollectedSecurityBundle {
            hostname,
            os,
            arch,
            kernel_version,
            public_ips,
            private_ips,
            ntp_synchronized,
            container_runtime,
            container_runtime_version,
            environment_type,
            x509_public_key,
            machine_id,
            logical_cpus,
            memory_total_bytes,
            egress_public_ip,
        }
    }

    fn sysinfo_resources(&self) -> (u32, u64) {
        let mut sys = System::new();
        sys.refresh_memory();
        let memory_total_bytes = sys.total_memory();
        sys.refresh_cpu_all();
        let logical_cpus = sys.cpus().len() as u32;
        (logical_cpus, memory_total_bytes)
    }

    /// Get the kernel version.
    fn get_kernel_version(&self) -> String {
        #[cfg(unix)]
        {
            Command::new("uname")
                .arg("-r")
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
                .unwrap_or_default()
        }
        #[cfg(windows)]
        {
            Command::new("ver")
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
                .unwrap_or_default()
        }
    }

    /// Check if NTP is synchronized.
    fn check_ntp_sync(&self) -> bool {
        #[cfg(target_os = "linux")]
        {
            // Check timedatectl
            if let Ok(output) = Command::new("timedatectl").arg("status").output() {
                if let Ok(stdout) = String::from_utf8(output.stdout) {
                    return stdout.contains("synchronized: yes")
                        || stdout.contains("NTP synchronized: yes");
                }
            }
            // Assume synced if we can't check
            true
        }
        #[cfg(not(target_os = "linux"))]
        {
            // Assume synced on other platforms
            true
        }
    }

    /// Detect container runtime.
    fn detect_container_runtime(&self) -> (String, String) {
        // Check for Docker
        if let Ok(output) = Command::new("docker").arg("--version").output() {
            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout);
                let version = version
                    .split_whitespace()
                    .nth(2)
                    .unwrap_or("unknown")
                    .trim_end_matches(',')
                    .to_string();
                return ("docker".to_string(), version);
            }
        }

        // Check for Podman
        if let Ok(output) = Command::new("podman").arg("--version").output() {
            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout);
                let version = version
                    .split_whitespace()
                    .nth(2)
                    .unwrap_or("unknown")
                    .to_string();
                return ("podman".to_string(), version);
            }
        }

        // Check for containerd
        if let Ok(output) = Command::new("ctr").arg("--version").output() {
            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout);
                let version = version
                    .split_whitespace()
                    .nth(2)
                    .unwrap_or("unknown")
                    .to_string();
                return ("containerd".to_string(), version);
            }
        }

        ("none".to_string(), String::new())
    }

    /// Detect environment type.
    fn detect_environment_type(&self) -> EnvironmentType {
        // Check if running in a container
        if std::path::Path::new("/.dockerenv").exists() {
            return EnvironmentType::Container;
        }
        if let Ok(cgroup) = std::fs::read_to_string("/proc/1/cgroup") {
            if cgroup.contains("docker") || cgroup.contains("kubepods") {
                return EnvironmentType::Container;
            }
        }

        // Check for VM
        #[cfg(target_os = "linux")]
        {
            if let Ok(output) = Command::new("systemd-detect-virt").output() {
                if output.status.success() {
                    let virt = String::from_utf8_lossy(&output.stdout);
                    let virt = virt.trim();
                    if virt != "none" {
                        return EnvironmentType::Virtual;
                    }
                }
            }
        }

        // Check for common VM indicators
        if let Ok(dmi) = std::fs::read_to_string("/sys/class/dmi/id/product_name") {
            let dmi = dmi.to_lowercase();
            if dmi.contains("virtual")
                || dmi.contains("vmware")
                || dmi.contains("kvm")
                || dmi.contains("qemu")
                || dmi.contains("hyper-v")
            {
                return EnvironmentType::Virtual;
            }
        }

        EnvironmentType::Physical
    }
}

fn read_machine_id() -> String {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/etc/machine-id")
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    }
    #[cfg(target_os = "macos")]
    {
        machine_id_macos()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        String::new()
    }
}

#[cfg(target_os = "macos")]
fn machine_id_macos() -> String {
    Command::new("system_profiler")
        .args(["SPHardwareDataType"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| {
            for line in s.lines() {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix("Serial Number (system):") {
                    let v = rest.trim();
                    if !v.is_empty() && v != "Not Available" {
                        return Some(v.to_string());
                    }
                }
            }
            None
        })
        .unwrap_or_default()
}

fn collect_interface_ips() -> (Vec<String>, Vec<String>) {
    let mut public = Vec::new();
    let mut private = Vec::new();
    let ifs = if_addrs::get_if_addrs().unwrap_or_default();
    for iface in ifs {
        let ip = match iface.addr {
            IfAddr::V4(a) => std::net::IpAddr::V4(a.ip),
            IfAddr::V6(a) => std::net::IpAddr::V6(a.ip),
        };
        if ip.is_loopback() {
            continue;
        }
        let s = ip.to_string();
        if is_private_or_local_ip(&ip) {
            private.push(s);
        } else {
            public.push(s);
        }
    }
    public.sort();
    public.dedup();
    private.sort();
    private.dedup();
    (public, private)
}

fn is_private_or_local_ip(ip: &std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => v4.is_private() || v4.is_link_local() || v4.is_loopback(),
        std::net::IpAddr::V6(v6) => {
            v6.is_loopback() || v6.is_unique_local() || v6.is_unicast_link_local()
        }
    }
}

async fn fetch_egress_public_ip() -> String {
    let url = std::env::var("MET_AGENT_EGRESS_IP_URL")
        .unwrap_or_else(|_| "https://api.ipify.org".to_string());
    if url.is_empty() {
        return String::new();
    }
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "failed to build HTTP client for egress IP discovery");
            return String::new();
        }
    };
    match client.get(url).send().await {
        Ok(resp) => match resp.text().await {
            Ok(body) => body.trim().to_string(),
            Err(e) => {
                warn!(error = %e, "egress IP response body error");
                String::new()
            }
        },
        Err(e) => {
            debug!(error = %e, "egress public IP fetch failed (offline or blocked)");
            String::new()
        }
    }
}

impl Default for SecurityBundleCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-job PKI for secret encryption.
///
/// Holds both an rcgen keypair (for X.509 CSRs) and an X25519 static secret
/// (for hybrid decryption of secrets encrypted by the controller).
pub struct JobPki {
    /// One-time keypair for this job (X.509/CSR use).
    key_pair: KeyPair,
    /// Private key (zeroized on drop).
    private_key_der: Zeroizing<Vec<u8>>,
    /// X25519 static secret for hybrid decryption.
    x25519_secret: X25519StaticSecret,
}

impl JobPki {
    /// Generate a new per-job PKI keypair and X25519 secret.
    pub fn generate() -> Result<Self, rcgen::Error> {
        let key_pair = KeyPair::generate()?;
        let private_key_der = Zeroizing::new(key_pair.serialize_der().to_vec());
        let x25519_secret = X25519StaticSecret::random_from_rng(OsRng);

        Ok(Self {
            key_pair,
            private_key_der,
            x25519_secret,
        })
    }

    /// Get the public key in DER format (for X.509 CSR).
    pub fn public_key_der(&self) -> Vec<u8> {
        self.key_pair.public_key_der().to_vec()
    }

    /// Get the X25519 public key (32 bytes) for hybrid encryption.
    ///
    /// The controller encrypts secrets with this public key using
    /// X25519 ECDH + HKDF-SHA256 + AES-256-GCM.
    pub fn x25519_public_key(&self) -> [u8; 32] {
        X25519PublicKey::from(&self.x25519_secret).to_bytes()
    }

    /// Decrypt a secret value using X25519 + AES-256-GCM hybrid decryption.
    ///
    /// Expects `encrypted` to be a serialized `EncryptedEnvelope` (ephemeral public
    /// key || nonce || HMAC || ciphertext length || ciphertext). Plaintext HMAC is
    /// verified using a key derived from ECDH (same as controller-side encrypt).
    pub fn decrypt(&self, encrypted: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
        let envelope = EncryptedEnvelope::from_bytes(encrypted)
            .map_err(|e| format!("failed to parse encrypted envelope: {e}"))?;

        let private_key_bytes = self.x25519_secret.to_bytes();

        HybridDecryption::decrypt(&private_key_bytes, &envelope)
            .map_err(|e| format!("hybrid decryption failed: {e}"))
    }
}
