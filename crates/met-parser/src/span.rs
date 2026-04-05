//! Source span tracking for YAML parsing.
//!
//! This module provides utilities for tracking source locations during YAML parsing,
//! enabling line-numbered error messages.

use crate::error::SourceLocation;
use serde_yaml::Value;
use std::collections::HashMap;

/// Tracks source spans for YAML values.
#[derive(Debug, Default)]
pub struct SpanTracker {
    /// Map from value path to source location.
    spans: HashMap<String, SourceLocation>,
    /// Source file path.
    source_file: Option<String>,
}

impl SpanTracker {
    /// Create a new span tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a span tracker with a source file path.
    pub fn with_file(source_file: impl Into<String>) -> Self {
        Self {
            spans: HashMap::new(),
            source_file: Some(source_file.into()),
        }
    }

    /// Create a span tracker from an existing reference (clones the data).
    pub fn from_existing(other: &SpanTracker) -> Self {
        Self {
            spans: other.spans.clone(),
            source_file: other.source_file.clone(),
        }
    }

    /// Parse YAML and track spans by walking through the source.
    pub fn parse_with_spans(&mut self, yaml: &str) -> Result<Value, serde_yaml::Error> {
        let value = serde_yaml::from_str(yaml)?;
        self.extract_spans(yaml);
        Ok(value)
    }

    /// Extract span information from YAML source.
    fn extract_spans(&mut self, yaml: &str) {
        let mut line_offsets: Vec<usize> = vec![0];
        for (i, c) in yaml.char_indices() {
            if c == '\n' {
                line_offsets.push(i + 1);
            }
        }

        for (line_num, line) in yaml.lines().enumerate() {
            let trimmed = line.trim();
            
            if let Some(key) = extract_yaml_key(trimmed) {
                let col = line.find(&key).unwrap_or(0) + 1;
                let offset = line_offsets.get(line_num).copied().unwrap_or(0);
                
                let location = SourceLocation {
                    file: self.source_file.clone(),
                    line: line_num + 1,
                    column: col,
                    offset,
                    length: key.len(),
                };
                
                self.spans.insert(key, location);
            }
        }
    }

    /// Get the source location for a key path.
    pub fn get_span(&self, key: &str) -> Option<&SourceLocation> {
        self.spans.get(key)
    }

    /// Get a source location with a fallback to unknown.
    pub fn get_span_or_unknown(&self, key: &str) -> SourceLocation {
        self.spans.get(key).cloned().unwrap_or_else(|| {
            SourceLocation {
                file: self.source_file.clone(),
                ..Default::default()
            }
        })
    }

    /// Register a span for a key.
    pub fn register_span(&mut self, key: impl Into<String>, location: SourceLocation) {
        self.spans.insert(key.into(), location);
    }
}

/// Extract a YAML key from a line.
fn extract_yaml_key(line: &str) -> Option<String> {
    let trimmed = line.trim();
    
    if trimmed.starts_with('#') || trimmed.starts_with('-') || trimmed.is_empty() {
        return None;
    }
    
    if let Some(colon_pos) = trimmed.find(':') {
        let key = &trimmed[..colon_pos];
        let key = key.trim().trim_matches('"').trim_matches('\'');
        if !key.is_empty() {
            return Some(key.to_string());
        }
    }
    
    None
}

/// Enhanced YAML parser with span tracking.
pub struct SpannedYamlParser {
    tracker: SpanTracker,
}

impl SpannedYamlParser {
    /// Create a new parser.
    pub fn new() -> Self {
        Self {
            tracker: SpanTracker::new(),
        }
    }

    /// Create a parser with source file information.
    pub fn with_file(source_file: impl Into<String>) -> Self {
        Self {
            tracker: SpanTracker::with_file(source_file),
        }
    }

    /// Parse YAML with span tracking.
    pub fn parse<T: serde::de::DeserializeOwned>(
        &mut self,
        yaml: &str,
    ) -> Result<(T, &SpanTracker), SpannedParseError> {
        self.tracker.extract_spans(yaml);
        
        match serde_yaml::from_str(yaml) {
            Ok(value) => Ok((value, &self.tracker)),
            Err(e) => {
                let location = e.location().map(|loc| {
                    SourceLocation {
                        file: self.tracker.source_file.clone(),
                        line: loc.line(),
                        column: loc.column(),
                        offset: loc.index(),
                        length: 1,
                    }
                });
                Err(SpannedParseError {
                    message: e.to_string(),
                    location,
                })
            }
        }
    }

    /// Get the span tracker.
    pub fn tracker(&self) -> &SpanTracker {
        &self.tracker
    }
}

impl Default for SpannedYamlParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse error with span information.
#[derive(Debug)]
pub struct SpannedParseError {
    /// Error message.
    pub message: String,
    /// Source location.
    pub location: Option<SourceLocation>,
}

impl std::fmt::Display for SpannedParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(loc) = &self.location {
            write!(f, "{} at {}", self.message, loc)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

impl std::error::Error for SpannedParseError {}

/// Find the line and column for a byte offset in a string.
pub fn offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    
    for (i, c) in source.char_indices() {
        if i >= offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    
    (line, col)
}

/// Find the byte offset for a line and column.
pub fn line_col_to_offset(source: &str, line: usize, col: usize) -> usize {
    let mut current_line = 1;
    let mut current_col = 1;
    
    for (i, c) in source.char_indices() {
        if current_line == line && current_col == col {
            return i;
        }
        if c == '\n' {
            current_line += 1;
            current_col = 1;
        } else {
            current_col += 1;
        }
    }
    
    source.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_yaml_key() {
        assert_eq!(extract_yaml_key("name: test"), Some("name".to_string()));
        assert_eq!(extract_yaml_key("  id: abc"), Some("id".to_string()));
        assert_eq!(extract_yaml_key("\"key\": value"), Some("key".to_string()));
        assert_eq!(extract_yaml_key("# comment"), None);
        assert_eq!(extract_yaml_key("- list item"), None);
        assert_eq!(extract_yaml_key(""), None);
    }

    #[test]
    fn test_offset_to_line_col() {
        let source = "line1\nline2\nline3";
        assert_eq!(offset_to_line_col(source, 0), (1, 1));
        assert_eq!(offset_to_line_col(source, 5), (1, 6));
        assert_eq!(offset_to_line_col(source, 6), (2, 1));
        assert_eq!(offset_to_line_col(source, 12), (3, 1));
    }

    #[test]
    fn test_spanned_parser() {
        let yaml = r#"
name: test
id: abc123
version: "1.0"
"#;
        
        let mut parser = SpannedYamlParser::with_file("test.yaml");
        let result = parser.parse::<serde_yaml::Value>(yaml);

        assert!(result.is_ok());

        let tracker = parser.tracker();
        assert!(tracker.get_span("name").is_some());
        assert!(tracker.get_span("id").is_some());
    }
}
