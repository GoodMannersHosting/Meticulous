//! Secret redaction from log output.
//!
//! Automatically detects and redacts sensitive information from logs,
//! including configured secrets and common patterns.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::RwLock;
use tracing::debug;

/// Configuration for the redactor.
#[derive(Debug, Clone)]
pub struct RedactorConfig {
    /// Replacement text for redacted content.
    pub replacement: String,
    /// Whether to redact common patterns (tokens, keys, etc.).
    pub redact_common_patterns: bool,
    /// Minimum length for a secret to be considered.
    pub min_secret_length: usize,
}

impl Default for RedactorConfig {
    fn default() -> Self {
        Self {
            replacement: "[REDACTED]".to_string(),
            redact_common_patterns: true,
            min_secret_length: 4,
        }
    }
}

/// A pattern to redact from logs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionPattern {
    /// Name of the pattern (for logging).
    pub name: String,
    /// Regex pattern to match.
    pub pattern: String,
}

impl RedactionPattern {
    /// Common patterns for tokens and secrets.
    pub fn common_patterns() -> Vec<Self> {
        vec![
            Self {
                name: "github_token".to_string(),
                pattern: r"gh[pousr]_[A-Za-z0-9_]{36,}".to_string(),
            },
            Self {
                name: "github_app_token".to_string(),
                pattern: r"ghs_[A-Za-z0-9_]{36,}".to_string(),
            },
            Self {
                name: "aws_access_key".to_string(),
                pattern: r"AKIA[0-9A-Z]{16}".to_string(),
            },
            Self {
                name: "aws_secret_key".to_string(),
                pattern: r#"(?i)aws[_\-]?secret[_\-]?access[_\-]?key['"]?\s*[:=]\s*['"]?([A-Za-z0-9/+=]{40})['"]?"#.to_string(),
            },
            Self {
                name: "jwt_token".to_string(),
                pattern: r"eyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+".to_string(),
            },
            Self {
                name: "private_key".to_string(),
                pattern: r"-----BEGIN (RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----".to_string(),
            },
            Self {
                name: "slack_token".to_string(),
                pattern: r"xox[baprs]-[A-Za-z0-9-]+".to_string(),
            },
            Self {
                name: "docker_config".to_string(),
                pattern: r#"(?i)"auth"\s*:\s*"[A-Za-z0-9+/=]+""#.to_string(),
            },
            Self {
                name: "generic_secret".to_string(),
                pattern: r#"(?i)(password|secret|token|api[_-]?key|auth)['"]?\s*[:=]\s*['"]?[^\s'"]{8,}['"]?"#.to_string(),
            },
        ]
    }
}

/// Log redactor that removes secrets and sensitive patterns.
pub struct Redactor {
    config: RedactorConfig,
    secrets: RwLock<HashSet<String>>,
    compiled_patterns: Vec<(String, Regex)>,
}

impl Redactor {
    /// Create a new redactor with the given configuration.
    pub fn new(config: RedactorConfig) -> Self {
        let compiled_patterns = if config.redact_common_patterns {
            RedactionPattern::common_patterns()
                .into_iter()
                .filter_map(|p| {
                    match Regex::new(&p.pattern) {
                        Ok(regex) => Some((p.name, regex)),
                        Err(e) => {
                            debug!("Failed to compile pattern {}: {}", p.name, e);
                            None
                        }
                    }
                })
                .collect()
        } else {
            Vec::new()
        };

        Self {
            config,
            secrets: RwLock::new(HashSet::new()),
            compiled_patterns,
        }
    }

    /// Add a secret value to be redacted.
    pub fn add_secret(&self, secret: impl Into<String>) {
        let secret = secret.into();
        if secret.len() >= self.config.min_secret_length {
            let mut secrets = self.secrets.write().unwrap();
            secrets.insert(secret);
        }
    }

    /// Add multiple secrets.
    pub fn add_secrets(&self, secrets: impl IntoIterator<Item = impl Into<String>>) {
        for secret in secrets {
            self.add_secret(secret);
        }
    }

    /// Clear all registered secrets.
    pub fn clear_secrets(&self) {
        let mut secrets = self.secrets.write().unwrap();
        secrets.clear();
    }

    /// Redact sensitive content from a string.
    pub fn redact(&self, input: &str) -> String {
        let mut output = input.to_string();

        // First, replace exact secret matches
        let secrets = self.secrets.read().unwrap();
        for secret in secrets.iter() {
            if output.contains(secret) {
                output = output.replace(secret, &self.config.replacement);
            }
        }
        drop(secrets);

        // Then apply pattern-based redaction
        for (name, regex) in &self.compiled_patterns {
            if regex.is_match(&output) {
                output = regex.replace_all(&output, &self.config.replacement).to_string();
                debug!("Redacted match for pattern: {}", name);
            }
        }

        output
    }

    /// Check if a string contains any known secrets.
    pub fn contains_secret(&self, input: &str) -> bool {
        // Check exact secrets
        let secrets = self.secrets.read().unwrap();
        for secret in secrets.iter() {
            if input.contains(secret) {
                return true;
            }
        }
        drop(secrets);

        // Check patterns
        for (_name, regex) in &self.compiled_patterns {
            if regex.is_match(input) {
                return true;
            }
        }

        false
    }

    /// Get the number of registered secrets.
    pub fn secret_count(&self) -> usize {
        self.secrets.read().unwrap().len()
    }
}

impl Default for Redactor {
    fn default() -> Self {
        Self::new(RedactorConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_secret_redaction() {
        let redactor = Redactor::new(RedactorConfig {
            redact_common_patterns: false,
            ..Default::default()
        });
        redactor.add_secret("my-super-secret-key");

        let input = "Using key: my-super-secret-key for auth";
        let output = redactor.redact(input);

        assert_eq!(output, "Using key: [REDACTED] for auth");
        assert!(!output.contains("my-super-secret-key"));
    }

    #[test]
    fn test_pattern_redaction() {
        let redactor = Redactor::default();

        // GitHub token
        let input = "Token: ghp_1234567890abcdefghijklmnopqrstuvwxyz";
        let output = redactor.redact(input);
        assert!(output.contains("[REDACTED]"));

        // AWS access key
        let input = "Key: AKIAIOSFODNN7EXAMPLE";
        let output = redactor.redact(input);
        assert!(output.contains("[REDACTED]"));

        // JWT
        let input = "Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.abc123";
        let output = redactor.redact(input);
        assert!(output.contains("[REDACTED]"));
    }

    #[test]
    fn test_multiple_secrets() {
        let redactor = Redactor::new(RedactorConfig {
            redact_common_patterns: false,
            ..Default::default()
        });
        redactor.add_secrets(vec!["secret1", "secret2", "secret3"]);

        let input = "Using secret1 and secret2 and secret3";
        let output = redactor.redact(input);

        assert_eq!(output, "Using [REDACTED] and [REDACTED] and [REDACTED]");
    }

    #[test]
    fn test_min_secret_length() {
        let redactor = Redactor::new(RedactorConfig {
            min_secret_length: 8,
            redact_common_patterns: false,
            ..Default::default()
        });
        redactor.add_secret("abc"); // Too short
        redactor.add_secret("longersecret"); // Long enough

        let input = "abc and longersecret";
        let output = redactor.redact(input);

        assert_eq!(output, "abc and [REDACTED]");
    }

    #[test]
    fn test_contains_secret() {
        let redactor = Redactor::default();
        redactor.add_secret("mysecret123");

        assert!(redactor.contains_secret("Using mysecret123 here"));
        assert!(!redactor.contains_secret("No secrets here"));
    }
}
