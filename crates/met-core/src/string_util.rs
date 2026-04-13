//! Small string helpers shared across crates.

/// Split `key=value` on the **first** `=`. The key must be non-empty.
///
/// The value may contain additional `=` characters (e.g. base64 or URLs).
#[must_use]
pub fn split_key_value(input: &str) -> Option<(&str, &str)> {
    let (k, v) = input.split_once('=')?;
    if k.is_empty() {
        return None;
    }
    Some((k, v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_equals_splits() {
        assert_eq!(split_key_value("a=b=c"), Some(("a", "b=c")));
    }

    #[test]
    fn empty_key_rejected() {
        assert_eq!(split_key_value("=x"), None);
    }

    #[test]
    fn no_equals() {
        assert_eq!(split_key_value("nope"), None);
    }
}
