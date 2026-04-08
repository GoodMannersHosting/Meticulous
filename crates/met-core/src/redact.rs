//! Values safe to emit in logs (never passwords or raw connection secrets).

/// Returns a database connection URL suitable for structured logs and stdout.
///
/// Passwords (if present) are replaced with `REDACTED`. If the string does not parse as a URL,
/// returns a constant placeholder so malformed or exotic DSNs are not echoed (they may still
/// contain secrets).
pub fn database_url_for_log(connection_url: &str) -> String {
    let Ok(mut u) = url::Url::parse(connection_url) else {
        return "<invalid database URL>".to_owned();
    };
    if u.password().is_some() {
        let _ = u.set_password(Some("REDACTED"));
    }
    u.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_password_postgres() {
        let raw = "postgres://myuser:supersecret@db.example.com:5432/appdb";
        let safe = database_url_for_log(raw);
        assert!(!safe.contains("supersecret"));
        assert!(
            safe.contains("REDACTED"),
            "expected redacted password in {safe:?}"
        );
        assert!(safe.contains("myuser"));
        assert!(safe.contains("db.example.com"));
    }

    #[test]
    fn leaves_passwordless_url_unchanged_except_normalization() {
        let raw = "postgres://myuser@db.example.com:5432/appdb";
        let safe = database_url_for_log(raw);
        assert!(!safe.contains("REDACTED"));
        assert!(safe.contains("myuser"));
    }

    #[test]
    fn garbage_url_not_echoed() {
        let safe = database_url_for_log("not a url password=secret");
        assert_eq!(safe, "<invalid database URL>");
        assert!(!safe.contains("secret"));
    }
}
