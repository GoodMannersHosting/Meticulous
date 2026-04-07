//! Secret masking filter for log pipelines.
//!
//! Provides agent-side and control-plane masking of secret values
//! in raw, base64, URL-encoded, and shell-escaped variants.

use base64::Engine;
use regex::Regex;
use std::collections::HashSet;
use std::sync::RwLock;
use tracing::debug;

const REPLACEMENT: &str = "***";

/// Secret masking filter for log output.
///
/// Runs in the agent process to mask secrets before they leave the agent.
/// A second-pass filter runs on the control plane as defense-in-depth.
pub struct SecretMaskingFilter {
    raw_secrets: RwLock<HashSet<String>>,
    encoded_variants: RwLock<Vec<String>>,
    common_patterns: Vec<Regex>,
}

impl SecretMaskingFilter {
    /// Create a new masking filter.
    pub fn new() -> Self {
        let common_patterns = Self::compile_common_patterns();
        Self {
            raw_secrets: RwLock::new(HashSet::new()),
            encoded_variants: RwLock::new(Vec::new()),
            common_patterns,
        }
    }

    fn compile_common_patterns() -> Vec<Regex> {
        let patterns = [
            r"ghp_[A-Za-z0-9_]{36,}",
            r"gho_[A-Za-z0-9_]{36,}",
            r"ghs_[A-Za-z0-9_]{36,}",
            r"AKIA[0-9A-Z]{16}",
            r"eyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+",
            r"-----BEGIN[A-Z ]*PRIVATE KEY-----",
            r"xox[baprs]-[A-Za-z0-9-]+",
            r"sk_live_[A-Za-z0-9]{24,}",
            r"met_join_[A-Za-z0-9]{32,}",
        ];
        patterns.iter().filter_map(|p| Regex::new(p).ok()).collect()
    }

    /// Register a secret value to be masked in all its encoded forms.
    pub fn add_secret(&self, secret: &str) {
        if secret.len() < 4 {
            return;
        }

        let mut raw = self.raw_secrets.write().unwrap();
        raw.insert(secret.to_string());

        let mut variants = self.encoded_variants.write().unwrap();

        // Base64 variant
        let b64 = base64::engine::general_purpose::STANDARD.encode(secret.as_bytes());
        if b64.len() >= 4 {
            variants.push(b64);
        }
        let b64url = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(secret.as_bytes());
        if b64url.len() >= 4 && b64url != secret {
            variants.push(b64url);
        }

        // URL-encoded variant
        let url_encoded = urlencoding(secret);
        if url_encoded != secret && url_encoded.len() >= 4 {
            variants.push(url_encoded);
        }

        // Shell-escaped variant (single-quoted)
        let shell_escaped = secret.replace('\'', "'\\''");
        if shell_escaped != secret && shell_escaped.len() >= 4 {
            variants.push(shell_escaped);
        }
    }

    /// Add multiple secrets.
    pub fn add_secrets(&self, secrets: impl IntoIterator<Item = impl AsRef<str>>) {
        for s in secrets {
            self.add_secret(s.as_ref());
        }
    }

    /// Apply the masking filter to a log line.
    pub fn mask(&self, input: &str) -> String {
        let mut output = input.to_string();

        // Mask raw secret values (longest first to handle overlapping substrings)
        let raw = self.raw_secrets.read().unwrap();
        let mut sorted: Vec<&String> = raw.iter().collect();
        sorted.sort_by(|a, b| b.len().cmp(&a.len()));
        for secret in sorted {
            if output.contains(secret.as_str()) {
                output = output.replace(secret.as_str(), REPLACEMENT);
            }
        }
        drop(raw);

        // Mask encoded variants
        let variants = self.encoded_variants.read().unwrap();
        for variant in variants.iter() {
            if output.contains(variant.as_str()) {
                output = output.replace(variant.as_str(), REPLACEMENT);
            }
        }
        drop(variants);

        // Mask common patterns
        for regex in &self.common_patterns {
            if regex.is_match(&output) {
                output = regex.replace_all(&output, REPLACEMENT).to_string();
            }
        }

        output
    }

    /// Check if a line contains any registered secrets.
    pub fn contains_secret(&self, input: &str) -> bool {
        let raw = self.raw_secrets.read().unwrap();
        for secret in raw.iter() {
            if input.contains(secret.as_str()) {
                return true;
            }
        }
        drop(raw);

        let variants = self.encoded_variants.read().unwrap();
        for variant in variants.iter() {
            if input.contains(variant.as_str()) {
                return true;
            }
        }
        drop(variants);

        for regex in &self.common_patterns {
            if regex.is_match(input) {
                return true;
            }
        }

        false
    }

    /// Clear all registered secrets.
    pub fn clear(&self) {
        self.raw_secrets.write().unwrap().clear();
        self.encoded_variants.write().unwrap().clear();
    }

    /// Number of registered secrets.
    pub fn secret_count(&self) -> usize {
        self.raw_secrets.read().unwrap().len()
    }
}

impl Default for SecretMaskingFilter {
    fn default() -> Self {
        Self::new()
    }
}

fn urlencoding(input: &str) -> String {
    let mut encoded = String::new();
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(b as char);
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", b));
            }
        }
    }
    encoded
}

/// Second-pass masking filter for the control plane (met-logging).
///
/// Runs as defense-in-depth after the agent-side filter.
pub struct ControlPlaneMaskingFilter {
    inner: SecretMaskingFilter,
}

impl ControlPlaneMaskingFilter {
    pub fn new() -> Self {
        Self {
            inner: SecretMaskingFilter::new(),
        }
    }

    pub fn add_secret(&self, secret: &str) {
        self.inner.add_secret(secret);
    }

    pub fn mask(&self, input: &str) -> String {
        self.inner.mask(input)
    }
}

impl Default for ControlPlaneMaskingFilter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raw_masking() {
        let filter = SecretMaskingFilter::new();
        filter.add_secret("my-super-secret-key-1234");
        let output = filter.mask("Using key: my-super-secret-key-1234 for auth");
        assert!(!output.contains("my-super-secret-key-1234"));
        assert!(output.contains(REPLACEMENT));
    }

    #[test]
    fn test_base64_masking() {
        let filter = SecretMaskingFilter::new();
        let secret = "my-api-secret-value";
        filter.add_secret(secret);
        let b64 = base64::engine::general_purpose::STANDARD.encode(secret.as_bytes());
        let output = filter.mask(&format!("Encoded: {b64}"));
        assert!(!output.contains(&b64));
    }

    #[test]
    fn test_url_encoded_masking() {
        let filter = SecretMaskingFilter::new();
        let secret = "pass word&special=chars";
        filter.add_secret(secret);
        let encoded = urlencoding(secret);
        let output = filter.mask(&format!("URL: {encoded}"));
        assert!(!output.contains(&encoded));
    }

    #[test]
    fn test_common_patterns() {
        let filter = SecretMaskingFilter::new();
        let output = filter.mask("Token: ghp_1234567890abcdefghijklmnopqrstuvwxyz");
        assert!(output.contains(REPLACEMENT));
        assert!(!output.contains("ghp_"));
    }

    #[test]
    fn test_multiple_secrets() {
        let filter = SecretMaskingFilter::new();
        filter.add_secret("secret-one-value");
        filter.add_secret("secret-two-value");
        let output = filter.mask("A=secret-one-value B=secret-two-value");
        assert!(!output.contains("secret-one"));
        assert!(!output.contains("secret-two"));
    }

    #[test]
    fn test_short_secrets_ignored() {
        let filter = SecretMaskingFilter::new();
        filter.add_secret("ab");
        assert_eq!(filter.secret_count(), 0);
    }

    #[test]
    fn test_contains_secret() {
        let filter = SecretMaskingFilter::new();
        filter.add_secret("check-this-secret");
        assert!(filter.contains_secret("log: check-this-secret found"));
        assert!(!filter.contains_secret("nothing here"));
    }
}
