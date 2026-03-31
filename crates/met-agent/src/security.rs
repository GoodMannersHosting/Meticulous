//! Security bundle collection and per-job PKI.

use std::process::Command;

use rcgen::{CertificateParams, KeyPair};
use tracing::{debug, warn};
use zeroize::Zeroizing;

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
        let arch = std::env::consts::ARCH.to_string();
        let kernel_version = self.get_kernel_version();
        let (public_ips, private_ips) = self.get_ip_addresses();
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
        }
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

    /// Get IP addresses.
    fn get_ip_addresses(&self) -> (Vec<String>, Vec<String>) {
        let mut public_ips = Vec::new();
        let mut private_ips = Vec::new();

        // Use sysinfo for network interfaces
        // For now, just try to get the hostname IP
        if let Ok(addrs) = std::net::ToSocketAddrs::to_socket_addrs(&(
            hostname::get()
                .ok()
                .and_then(|h| h.into_string().ok())
                .unwrap_or_else(|| "localhost".to_string()),
            0u16,
        )) {
            for addr in addrs {
                let ip = addr.ip();
                let ip_str = ip.to_string();
                if ip.is_loopback() {
                    continue;
                }
                if is_private_ip(&ip) {
                    private_ips.push(ip_str);
                } else {
                    public_ips.push(ip_str);
                }
            }
        }

        (public_ips, private_ips)
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

impl Default for SecurityBundleCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if an IP address is private.
fn is_private_ip(ip: &std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(ipv4) => {
            ipv4.is_private() || ipv4.is_link_local() || ipv4.is_loopback()
        }
        std::net::IpAddr::V6(ipv6) => ipv6.is_loopback(),
    }
}

/// Per-job PKI for secret encryption.
pub struct JobPki {
    /// One-time keypair for this job.
    key_pair: KeyPair,
    /// Private key (zeroized on drop).
    private_key_der: Zeroizing<Vec<u8>>,
}

impl JobPki {
    /// Generate a new per-job PKI keypair.
    pub fn generate() -> Result<Self, rcgen::Error> {
        let key_pair = KeyPair::generate()?;
        let private_key_der = Zeroizing::new(key_pair.serialize_der().to_vec());

        Ok(Self {
            key_pair,
            private_key_der,
        })
    }

    /// Get the public key in DER format.
    pub fn public_key_der(&self) -> Vec<u8> {
        self.key_pair.public_key_der().to_vec()
    }

    /// Decrypt a secret value.
    pub fn decrypt(&self, _encrypted: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
        // TODO: Implement actual decryption using hybrid encryption
        // 1. Decrypt the symmetric key with our private key
        // 2. Decrypt the actual secret with the symmetric key
        // For now, just return a placeholder
        Err("decryption not yet implemented".to_string())
    }
}
