//! hashFiles() helper function for cache key templates.
//!
//! Computes SHA-256 hashes of file contents matching glob patterns,
//! used for cache key computation.

use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// Options for hash_files operation.
#[derive(Debug, Clone)]
pub struct HashFilesOptions {
    /// Base directory for glob matching.
    pub base_dir: PathBuf,
    /// Whether to follow symlinks.
    pub follow_symlinks: bool,
    /// Maximum file size to hash (larger files are skipped).
    pub max_file_size: u64,
}

impl Default for HashFilesOptions {
    fn default() -> Self {
        Self {
            base_dir: PathBuf::from("."),
            follow_symlinks: false,
            max_file_size: 100 * 1024 * 1024,
        }
    }
}

/// Compute SHA-256 hash of files matching a glob pattern.
///
/// # Arguments
///
/// * `pattern` - Glob pattern to match files (e.g., "**/*.lock", "Cargo.lock")
/// * `options` - Hash options including base directory
///
/// # Returns
///
/// A hex-encoded SHA-256 hash of the concatenated file contents,
/// or an empty string if no files match.
pub fn hash_files_with_glob(pattern: &str, options: &HashFilesOptions) -> String {
    let pattern_path = options.base_dir.join(pattern);
    let pattern_str = pattern_path.to_string_lossy();

    debug!(pattern = %pattern_str, "hashing files matching pattern");

    let mut hasher = Sha256::new();
    let mut file_count = 0;
    let mut total_size = 0u64;

    let paths = match glob::glob(&pattern_str) {
        Ok(paths) => paths,
        Err(e) => {
            warn!(error = %e, pattern = %pattern, "invalid glob pattern");
            return String::new();
        }
    };

    let mut matched_paths: Vec<PathBuf> = paths
        .filter_map(|r| r.ok())
        .filter(|p| p.is_file())
        .collect();

    matched_paths.sort();

    for path in matched_paths {
        match hash_single_file(&path, &options, &mut hasher) {
            Ok(size) => {
                file_count += 1;
                total_size += size;
            }
            Err(e) => {
                warn!(path = %path.display(), error = %e, "failed to hash file");
            }
        }
    }

    if file_count == 0 {
        debug!(pattern = %pattern, "no files matched pattern");
        return String::new();
    }

    let result = hasher.finalize();
    let hash = hex::encode(result);

    debug!(
        pattern = %pattern,
        files = file_count,
        total_bytes = total_size,
        hash = %hash,
        "computed hash for files"
    );

    hash
}

/// Compute hash for multiple patterns combined.
pub fn hash_files(patterns: &[&str], options: &HashFilesOptions) -> String {
    let mut hasher = Sha256::new();

    for pattern in patterns {
        let pattern_hash = hash_files_with_glob(pattern, options);
        if !pattern_hash.is_empty() {
            hasher.update(pattern_hash.as_bytes());
        }
    }

    if patterns.is_empty() {
        return String::new();
    }

    let result = hasher.finalize();
    hex::encode(result)
}

/// Hash a single file and update the hasher.
fn hash_single_file(
    path: &Path,
    options: &HashFilesOptions,
    hasher: &mut Sha256,
) -> std::io::Result<u64> {
    let metadata = if options.follow_symlinks {
        std::fs::metadata(path)?
    } else {
        std::fs::symlink_metadata(path)?
    };

    if metadata.len() > options.max_file_size {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "file too large",
        ));
    }

    hasher.update(path.to_string_lossy().as_bytes());
    hasher.update(b"\0");

    let mut file = std::fs::File::open(path)?;
    let mut buffer = [0u8; 8192];
    let mut total = 0u64;

    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        total += n as u64;
    }

    hasher.update(b"\0");

    Ok(total)
}

/// Parse a hashFiles expression from a cache key template.
///
/// Extracts the glob pattern from expressions like `hashFiles('**/*.lock')`.
pub fn parse_hash_files_expr(expr: &str) -> Option<&str> {
    let expr = expr.trim();

    if !expr.starts_with("hashFiles(") || !expr.ends_with(')') {
        return None;
    }

    let inner = &expr[10..expr.len() - 1].trim();

    let pattern = if (inner.starts_with('\'') && inner.ends_with('\''))
        || (inner.starts_with('"') && inner.ends_with('"'))
    {
        &inner[1..inner.len() - 1]
    } else {
        inner
    };

    Some(pattern)
}

/// Evaluate a cache key template, resolving hashFiles() calls.
pub fn evaluate_cache_key(
    template: &str,
    variables: &std::collections::HashMap<String, String>,
    options: &HashFilesOptions,
) -> String {
    let mut result = template.to_string();

    let hash_pattern = regex::Regex::new(r#"hashFiles\(['"]([^'"]+)['"]\)"#).expect("valid regex");

    for cap in hash_pattern.captures_iter(template) {
        let full_match = cap.get(0).unwrap().as_str();
        let pattern = cap.get(1).unwrap().as_str();
        let hash = hash_files_with_glob(pattern, options);
        result = result.replace(full_match, &hash);
    }

    let var_pattern = regex::Regex::new(r"\$\{([a-zA-Z_][a-zA-Z0-9_]*)\}").expect("valid regex");

    for cap in var_pattern.captures_iter(template) {
        let full_match = cap.get(0).unwrap().as_str();
        let var_name = cap.get(1).unwrap().as_str();
        if let Some(value) = variables.get(var_name) {
            result = result.replace(full_match, value);
        }
    }

    result
}

mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes
            .as_ref()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_files(dir: &Path) {
        std::fs::create_dir_all(dir.join("src")).unwrap();

        std::fs::write(dir.join("Cargo.lock"), b"lock file content").unwrap();
        std::fs::write(dir.join("Cargo.toml"), b"toml file content").unwrap();
        std::fs::write(dir.join("src/main.rs"), b"fn main() {}").unwrap();
    }

    #[test]
    fn test_hash_files_single_file() {
        let dir = TempDir::new().unwrap();
        create_test_files(dir.path());

        let options = HashFilesOptions {
            base_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        let hash = hash_files_with_glob("Cargo.lock", &options);
        assert!(!hash.is_empty());
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn test_hash_files_glob_pattern() {
        let dir = TempDir::new().unwrap();
        create_test_files(dir.path());

        let options = HashFilesOptions {
            base_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        let hash = hash_files_with_glob("**/*.rs", &options);
        assert!(!hash.is_empty());
    }

    #[test]
    fn test_hash_files_no_match() {
        let dir = TempDir::new().unwrap();
        create_test_files(dir.path());

        let options = HashFilesOptions {
            base_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        let hash = hash_files_with_glob("**/*.nonexistent", &options);
        assert!(hash.is_empty());
    }

    #[test]
    fn test_hash_files_deterministic() {
        let dir = TempDir::new().unwrap();
        create_test_files(dir.path());

        let options = HashFilesOptions {
            base_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        let hash1 = hash_files_with_glob("**/*", &options);
        let hash2 = hash_files_with_glob("**/*", &options);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_parse_hash_files_expr() {
        assert_eq!(
            parse_hash_files_expr("hashFiles('**/*.lock')"),
            Some("**/*.lock")
        );
        assert_eq!(
            parse_hash_files_expr("hashFiles(\"**/Cargo.lock\")"),
            Some("**/Cargo.lock")
        );
        assert_eq!(parse_hash_files_expr("not_hashFiles('test')"), None);
        assert_eq!(parse_hash_files_expr("hashFiles"), None);
    }

    #[test]
    fn test_evaluate_cache_key() {
        let dir = TempDir::new().unwrap();
        create_test_files(dir.path());

        let options = HashFilesOptions {
            base_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        let mut vars = std::collections::HashMap::new();
        vars.insert("PROJECT".to_string(), "myproject".to_string());

        let template = "${PROJECT}-hashFiles('Cargo.lock')";
        let result = evaluate_cache_key(template, &vars, &options);

        assert!(result.starts_with("myproject-"));
        assert_eq!(result.len(), "myproject-".len() + 64);
    }
}
