//! Semver version resolution for workflow references.
//!
//! Supports parsing version constraints and resolving them against available versions.

use std::cmp::Ordering;

/// A parsed semver version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub prerelease: Option<String>,
}

impl Version {
    /// Parse a version string.
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().trim_start_matches('v');

        let (version_part, prerelease) = if let Some(idx) = s.find('-') {
            (&s[..idx], Some(s[idx + 1..].to_string()))
        } else {
            (s, None)
        };

        let parts: Vec<&str> = version_part.split('.').collect();

        let major = parts.first()?.parse().ok()?;
        let minor = parts.get(1).and_then(|p| p.parse().ok()).unwrap_or(0);
        let patch = parts.get(2).and_then(|p| p.parse().ok()).unwrap_or(0);

        Some(Version {
            major,
            minor,
            patch,
            prerelease,
        })
    }

    /// Check if this version satisfies a constraint.
    pub fn satisfies(&self, constraint: &VersionConstraint) -> bool {
        match constraint {
            VersionConstraint::Exact(v) => self == v,
            VersionConstraint::Caret(v) => {
                if v.major == 0 {
                    if v.minor == 0 {
                        self.major == 0 && self.minor == 0 && self.patch == v.patch
                    } else {
                        self.major == 0 && self.minor == v.minor && self.patch >= v.patch
                    }
                } else {
                    self.major == v.major && self >= v
                }
            }
            VersionConstraint::Tilde(v) => {
                self.major == v.major && self.minor == v.minor && self.patch >= v.patch
            }
            VersionConstraint::Range { min, max } => {
                let above_min = min.as_ref().is_none_or(|m| self >= m);
                let below_max = max.as_ref().is_none_or(|m| self < m);
                above_min && below_max
            }
            VersionConstraint::GreaterThan(v) => self > v,
            VersionConstraint::GreaterThanOrEqual(v) => self >= v,
            VersionConstraint::LessThan(v) => self < v,
            VersionConstraint::LessThanOrEqual(v) => self <= v,
            VersionConstraint::Any => true,
        }
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.major.cmp(&other.major) {
            Ordering::Equal => {}
            ord => return ord,
        }
        match self.minor.cmp(&other.minor) {
            Ordering::Equal => {}
            ord => return ord,
        }
        match self.patch.cmp(&other.patch) {
            Ordering::Equal => {}
            ord => return ord,
        }
        match (&self.prerelease, &other.prerelease) {
            (None, None) => Ordering::Equal,
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (Some(a), Some(b)) => a.cmp(b),
        }
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(pre) = &self.prerelease {
            write!(f, "-{}", pre)?;
        }
        Ok(())
    }
}

/// A version constraint for matching.
#[derive(Debug, Clone)]
pub enum VersionConstraint {
    /// Exact version match.
    Exact(Version),
    /// Caret constraint (^1.2.3): compatible with version.
    Caret(Version),
    /// Tilde constraint (~1.2.3): patch-level changes allowed.
    Tilde(Version),
    /// Range constraint (>=1.0.0 <2.0.0).
    Range {
        min: Option<Version>,
        max: Option<Version>,
    },
    /// Greater than.
    GreaterThan(Version),
    /// Greater than or equal.
    GreaterThanOrEqual(Version),
    /// Less than.
    LessThan(Version),
    /// Less than or equal.
    LessThanOrEqual(Version),
    /// Any version (*, latest).
    Any,
}

/// Parse a version constraint string.
pub fn parse_version_constraint(s: &str) -> Result<VersionConstraint, String> {
    let s = s.trim();

    if s == "*" || s == "latest" || s.is_empty() {
        return Ok(VersionConstraint::Any);
    }

    if let Some(rest) = s.strip_prefix('^') {
        let version = Version::parse(rest)
            .ok_or_else(|| format!("Invalid version in constraint: {}", s))?;
        return Ok(VersionConstraint::Caret(version));
    }

    if let Some(rest) = s.strip_prefix('~') {
        let version = Version::parse(rest)
            .ok_or_else(|| format!("Invalid version in constraint: {}", s))?;
        return Ok(VersionConstraint::Tilde(version));
    }

    if let Some(rest) = s.strip_prefix(">=") {
        let version = Version::parse(rest)
            .ok_or_else(|| format!("Invalid version in constraint: {}", s))?;
        return Ok(VersionConstraint::GreaterThanOrEqual(version));
    }

    if let Some(rest) = s.strip_prefix('>') {
        let version = Version::parse(rest)
            .ok_or_else(|| format!("Invalid version in constraint: {}", s))?;
        return Ok(VersionConstraint::GreaterThan(version));
    }

    if let Some(rest) = s.strip_prefix("<=") {
        let version = Version::parse(rest)
            .ok_or_else(|| format!("Invalid version in constraint: {}", s))?;
        return Ok(VersionConstraint::LessThanOrEqual(version));
    }

    if let Some(rest) = s.strip_prefix('<') {
        let version = Version::parse(rest)
            .ok_or_else(|| format!("Invalid version in constraint: {}", s))?;
        return Ok(VersionConstraint::LessThan(version));
    }

    let version = Version::parse(s).ok_or_else(|| format!("Invalid version: {}", s))?;
    Ok(VersionConstraint::Exact(version))
}

/// Resolve a version constraint against a list of available versions.
///
/// Returns the highest matching version, or None if no match.
pub fn resolve_version(constraint: &VersionConstraint, available: &[String]) -> Option<String> {
    let mut versions: Vec<(String, Version)> = available
        .iter()
        .filter_map(|s| Version::parse(s).map(|v| (s.clone(), v)))
        .collect();

    versions.sort_by(|a, b| b.1.cmp(&a.1));

    for (original, version) in versions {
        if version.satisfies(constraint) {
            return Some(original);
        }
    }

    None
}

/// Check if a version string is a valid semver.
pub fn is_valid_semver(s: &str) -> bool {
    Version::parse(s).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version() {
        assert_eq!(
            Version::parse("1.2.3"),
            Some(Version {
                major: 1,
                minor: 2,
                patch: 3,
                prerelease: None,
            })
        );

        assert_eq!(
            Version::parse("v1.2.3"),
            Some(Version {
                major: 1,
                minor: 2,
                patch: 3,
                prerelease: None,
            })
        );

        assert_eq!(
            Version::parse("1.0.0-alpha"),
            Some(Version {
                major: 1,
                minor: 0,
                patch: 0,
                prerelease: Some("alpha".to_string()),
            })
        );

        assert_eq!(
            Version::parse("2.0"),
            Some(Version {
                major: 2,
                minor: 0,
                patch: 0,
                prerelease: None,
            })
        );
    }

    #[test]
    fn test_version_ordering() {
        let v1 = Version::parse("1.0.0").unwrap();
        let v2 = Version::parse("2.0.0").unwrap();
        let v3 = Version::parse("1.1.0").unwrap();
        let v4 = Version::parse("1.0.1").unwrap();

        assert!(v1 < v2);
        assert!(v1 < v3);
        assert!(v1 < v4);
        assert!(v3 < v2);
        assert!(v4 < v3);
    }

    #[test]
    fn test_caret_constraint() {
        let constraint = parse_version_constraint("^1.2.3").unwrap();

        assert!(Version::parse("1.2.3").unwrap().satisfies(&constraint));
        assert!(Version::parse("1.2.4").unwrap().satisfies(&constraint));
        assert!(Version::parse("1.9.0").unwrap().satisfies(&constraint));
        assert!(!Version::parse("2.0.0").unwrap().satisfies(&constraint));
        assert!(!Version::parse("1.2.2").unwrap().satisfies(&constraint));
    }

    #[test]
    fn test_tilde_constraint() {
        let constraint = parse_version_constraint("~1.2.3").unwrap();

        assert!(Version::parse("1.2.3").unwrap().satisfies(&constraint));
        assert!(Version::parse("1.2.9").unwrap().satisfies(&constraint));
        assert!(!Version::parse("1.3.0").unwrap().satisfies(&constraint));
        assert!(!Version::parse("1.2.2").unwrap().satisfies(&constraint));
    }

    #[test]
    fn test_resolve_version() {
        let available = vec![
            "1.0.0".to_string(),
            "1.1.0".to_string(),
            "1.2.0".to_string(),
            "2.0.0".to_string(),
        ];

        let constraint = parse_version_constraint("^1.0.0").unwrap();
        assert_eq!(
            resolve_version(&constraint, &available),
            Some("1.2.0".to_string())
        );

        let constraint = parse_version_constraint("~1.1.0").unwrap();
        assert_eq!(
            resolve_version(&constraint, &available),
            Some("1.1.0".to_string())
        );

        let constraint = parse_version_constraint(">=2.0.0").unwrap();
        assert_eq!(
            resolve_version(&constraint, &available),
            Some("2.0.0".to_string())
        );

        let constraint = parse_version_constraint("3.0.0").unwrap();
        assert_eq!(resolve_version(&constraint, &available), None);
    }

    #[test]
    fn test_any_constraint() {
        let constraint = parse_version_constraint("*").unwrap();
        assert!(Version::parse("1.0.0").unwrap().satisfies(&constraint));
        assert!(Version::parse("999.0.0").unwrap().satisfies(&constraint));

        let constraint = parse_version_constraint("latest").unwrap();
        assert!(Version::parse("1.0.0").unwrap().satisfies(&constraint));
    }
}
