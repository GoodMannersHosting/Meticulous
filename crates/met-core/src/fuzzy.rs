//! Fuzzy string matching for typo suggestions (ADR-019).

/// Compute Levenshtein edit distance between two strings.
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a_len = a.len();
    let b_len = b.len();

    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();

    let mut prev: Vec<usize> = (0..=b_len).collect();
    let mut curr = vec![0; b_len + 1];

    for i in 1..=a_len {
        curr[0] = i;
        for j in 1..=b_len {
            let cost = if a_bytes[i - 1] == b_bytes[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1)
                .min(curr[j - 1] + 1)
                .min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[b_len]
}

/// Find fuzzy suggestions for `needle` from `candidates`.
///
/// Returns up to `max_results` candidates sorted by edit distance,
/// filtered by max distance thresholds from ADR-019.
pub fn suggest(needle: &str, candidates: &[&str], max_results: usize) -> Vec<String> {
    let max_distance = if needle.len() > 10 { 3 } else { 2 };

    let mut matches: Vec<(usize, &str)> = candidates
        .iter()
        .filter_map(|c| {
            let d = levenshtein(needle, c);
            if d <= max_distance && d > 0 {
                Some((d, *c))
            } else {
                None
            }
        })
        .collect();

    matches.sort_by_key(|(d, _)| *d);
    matches
        .into_iter()
        .take(max_results)
        .map(|(_, s)| s.to_string())
        .collect()
}

/// Format suggestions into a human-readable string.
pub fn format_suggestions(suggestions: &[String]) -> Option<String> {
    match suggestions.len() {
        0 => None,
        1 => Some(format!("Did you mean '{}'?", suggestions[0])),
        _ => {
            let quoted: Vec<String> = suggestions.iter().map(|s| format!("'{s}'")).collect();
            Some(format!("Did you mean one of: {}?", quoted.join(", ")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein_identical() {
        assert_eq!(levenshtein("hello", "hello"), 0);
    }

    #[test]
    fn test_levenshtein_empty() {
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("abc", ""), 3);
        assert_eq!(levenshtein("", ""), 0);
    }

    #[test]
    fn test_levenshtein_one_off() {
        assert_eq!(levenshtein("kitten", "sitten"), 1);
        assert_eq!(levenshtein("DB_PASSWORD", "DB_PASWORD"), 1);
    }

    #[test]
    fn test_levenshtein_multi() {
        assert_eq!(levenshtein("kitten", "sitting"), 3);
    }

    #[test]
    fn test_suggest_finds_close_match() {
        let candidates = vec!["DB_PASSWORD", "DB_HOST", "DB_PORT"];
        let results = suggest("DB_PASWORD", &candidates, 3);
        assert_eq!(results, vec!["DB_PASSWORD"]);
    }

    #[test]
    fn test_suggest_no_match_when_too_far() {
        let candidates = vec!["DB_PASSWORD", "DB_HOST"];
        let results = suggest("COMPLETELY_DIFFERENT", &candidates, 3);
        assert!(results.is_empty());
    }

    #[test]
    fn test_suggest_multiple() {
        let candidates = vec!["DB_PASS", "DB_PATH", "DB_PORT"];
        let results = suggest("DB_PASH", &candidates, 3);
        assert!(results.contains(&"DB_PASS".to_string()));
        assert!(results.contains(&"DB_PATH".to_string()));
    }

    #[test]
    fn test_format_suggestions_none() {
        assert_eq!(format_suggestions(&[]), None);
    }

    #[test]
    fn test_format_suggestions_one() {
        let s = format_suggestions(&["DB_PASSWORD".to_string()]);
        assert_eq!(s, Some("Did you mean 'DB_PASSWORD'?".to_string()));
    }

    #[test]
    fn test_format_suggestions_multiple() {
        let s = format_suggestions(&["DB_PASS".to_string(), "DB_PATH".to_string()]);
        assert_eq!(
            s,
            Some("Did you mean one of: 'DB_PASS', 'DB_PATH'?".to_string())
        );
    }
}
