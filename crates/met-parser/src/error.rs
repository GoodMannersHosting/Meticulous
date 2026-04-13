//! Error types for pipeline parsing with source-span information.

use std::fmt;

/// Result type for parser operations.
pub type Result<T> = std::result::Result<T, ParseError>;

/// Severity level for parse diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Fatal error that prevents parsing.
    Error,
    /// Warning that doesn't prevent parsing but indicates potential issues.
    Warning,
    /// Informational message.
    Info,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Error => write!(f, "error"),
            Severity::Warning => write!(f, "warning"),
            Severity::Info => write!(f, "info"),
        }
    }
}

/// Source location in a YAML file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SourceLocation {
    /// File path (if known).
    pub file: Option<String>,
    /// Line number (1-indexed).
    pub line: usize,
    /// Column number (1-indexed).
    pub column: usize,
    /// Byte offset in the source.
    pub offset: usize,
    /// Length of the span in bytes.
    pub length: usize,
}

impl SourceLocation {
    /// Create a new source location.
    pub fn new(line: usize, column: usize) -> Self {
        Self {
            file: None,
            line,
            column,
            offset: 0,
            length: 0,
        }
    }

    /// Create a source location with file path.
    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    /// Create a source location with offset and length.
    pub fn with_span(mut self, offset: usize, length: usize) -> Self {
        self.offset = offset;
        self.length = length;
        self
    }

    /// Unknown location.
    pub fn unknown() -> Self {
        Self::default()
    }
}

impl fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(file) = &self.file {
            write!(f, "{}:{}:{}", file, self.line, self.column)
        } else {
            write!(f, "{}:{}", self.line, self.column)
        }
    }
}

/// Machine-readable error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    // Deserialization errors (E1xxx)
    /// Invalid YAML syntax.
    E1001,
    /// Invalid UTF-8 in source.
    E1002,

    // Schema validation errors (E2xxx)
    /// Missing required field.
    E2001,
    /// Invalid field type.
    E2002,
    /// Unknown field.
    E2003,
    /// Invalid enum value.
    E2004,
    /// Duplicate identifier.
    E2005,
    /// Invalid identifier format.
    E2006,
    /// Value out of range.
    E2007,

    // Workflow resolution errors (E3xxx)
    /// Workflow not found.
    E3001,
    /// Invalid workflow reference format.
    E3002,
    /// Workflow version not found.
    E3003,
    /// Circular workflow reference.
    E3004,
    /// Maximum workflow nesting depth exceeded.
    E3005,

    // Variable resolution errors (E4xxx)
    /// Undefined variable reference.
    E4001,
    /// Undefined secret reference.
    E4002,
    /// Invalid interpolation syntax.
    E4003,
    /// Recursive variable reference.
    E4004,

    // DAG validation errors (E5xxx)
    /// Cycle detected in dependency graph.
    E5001,
    /// Unknown dependency reference.
    E5002,
    /// Self-dependency.
    E5003,
    /// Unreachable node in DAG.
    E5004,
    /// Concurrent jobs in a shared-workspace affinity group.
    E5005,
    /// Invalid or ambiguous explicit `workspace:` transfer (`from`, dependency, or producer resolution).
    E5006,

    // General errors (E9xxx)
    /// Internal parser error.
    E9001,
    /// I/O error.
    E9002,
}

impl ErrorCode {
    /// Get a short description of the error code.
    pub fn description(&self) -> &'static str {
        match self {
            ErrorCode::E1001 => "invalid YAML syntax",
            ErrorCode::E1002 => "invalid UTF-8 encoding",
            ErrorCode::E2001 => "missing required field",
            ErrorCode::E2002 => "invalid field type",
            ErrorCode::E2003 => "unknown field",
            ErrorCode::E2004 => "invalid enum value",
            ErrorCode::E2005 => "duplicate identifier",
            ErrorCode::E2006 => "invalid identifier format",
            ErrorCode::E2007 => "value out of range",
            ErrorCode::E3001 => "workflow not found",
            ErrorCode::E3002 => "invalid workflow reference",
            ErrorCode::E3003 => "workflow version not found",
            ErrorCode::E3004 => "circular workflow reference",
            ErrorCode::E3005 => "maximum nesting depth exceeded",
            ErrorCode::E4001 => "undefined variable",
            ErrorCode::E4002 => "undefined secret",
            ErrorCode::E4003 => "invalid interpolation syntax",
            ErrorCode::E4004 => "recursive variable reference",
            ErrorCode::E5001 => "cycle in dependency graph",
            ErrorCode::E5002 => "unknown dependency",
            ErrorCode::E5003 => "self-dependency not allowed",
            ErrorCode::E5004 => "unreachable node",
            ErrorCode::E5005 => "unsafe concurrent jobs for shared workspace affinity",
            ErrorCode::E5006 => "invalid workspace snapshot transfer",
            ErrorCode::E9001 => "internal error",
            ErrorCode::E9002 => "I/O error",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// A parse error with source location and context.
#[derive(Debug, Clone)]
pub struct ParseError {
    /// Severity of the error.
    pub severity: Severity,
    /// Human-readable error message.
    pub message: String,
    /// Source location where the error occurred.
    pub source: SourceLocation,
    /// Optional hint for fixing the error.
    pub hint: Option<String>,
    /// Machine-readable error code.
    pub code: ErrorCode,
}

impl ParseError {
    /// Create a new parse error.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            message: message.into(),
            source: SourceLocation::unknown(),
            hint: None,
            code,
        }
    }

    /// Create a warning.
    pub fn warning(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            message: message.into(),
            source: SourceLocation::unknown(),
            hint: None,
            code,
        }
    }

    /// Set the source location.
    pub fn with_source(mut self, source: SourceLocation) -> Self {
        self.source = source;
        self
    }

    /// Set the hint.
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// Check if this is an error (not a warning or info).
    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} [{}]: {} at {}",
            self.severity, self.code, self.message, self.source
        )?;
        if let Some(hint) = &self.hint {
            write!(f, "\n  hint: {}", hint)?;
        }
        Ok(())
    }
}

impl std::error::Error for ParseError {}

/// Collection of parse errors/warnings.
#[derive(Debug, Default)]
pub struct ParseDiagnostics {
    diagnostics: Vec<ParseError>,
}

impl ParseDiagnostics {
    /// Create an empty diagnostics collection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a diagnostic.
    pub fn push(&mut self, error: ParseError) {
        self.diagnostics.push(error);
    }

    /// Add an error.
    pub fn error(&mut self, code: ErrorCode, message: impl Into<String>) {
        self.push(ParseError::new(code, message));
    }

    /// Add an error with source location.
    pub fn error_at(
        &mut self,
        code: ErrorCode,
        message: impl Into<String>,
        source: SourceLocation,
    ) {
        self.push(ParseError::new(code, message).with_source(source));
    }

    /// Add a warning.
    pub fn warning(&mut self, code: ErrorCode, message: impl Into<String>) {
        self.push(ParseError::warning(code, message));
    }

    /// Check if there are any errors (not just warnings).
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.is_error())
    }

    /// Get the number of diagnostics.
    pub fn len(&self) -> usize {
        self.diagnostics.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Get all diagnostics.
    pub fn all(&self) -> &[ParseError] {
        &self.diagnostics
    }

    /// Get only errors.
    pub fn errors(&self) -> impl Iterator<Item = &ParseError> {
        self.diagnostics.iter().filter(|d| d.is_error())
    }

    /// Get only warnings.
    pub fn warnings(&self) -> impl Iterator<Item = &ParseError> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
    }

    /// Merge another diagnostics collection into this one.
    pub fn merge(&mut self, other: ParseDiagnostics) {
        self.diagnostics.extend(other.diagnostics);
    }

    /// Convert to a result, failing if there are any errors.
    pub fn into_result<T>(self, value: T) -> std::result::Result<T, Vec<ParseError>> {
        if self.has_errors() {
            Err(self.diagnostics)
        } else {
            Ok(value)
        }
    }
}

impl IntoIterator for ParseDiagnostics {
    type Item = ParseError;
    type IntoIter = std::vec::IntoIter<ParseError>;

    fn into_iter(self) -> Self::IntoIter {
        self.diagnostics.into_iter()
    }
}

impl<'a> IntoIterator for &'a ParseDiagnostics {
    type Item = &'a ParseError;
    type IntoIter = std::slice::Iter<'a, ParseError>;

    fn into_iter(self) -> Self::IntoIter {
        self.diagnostics.iter()
    }
}

impl From<serde_yaml::Error> for ParseError {
    fn from(err: serde_yaml::Error) -> Self {
        let location = err
            .location()
            .map(|loc| SourceLocation::new(loc.line(), loc.column()).with_span(loc.index(), 1));

        let mut error = ParseError::new(ErrorCode::E1001, err.to_string());
        if let Some(loc) = location {
            error = error.with_source(loc);
        }
        error
    }
}

impl From<std::io::Error> for ParseError {
    fn from(err: std::io::Error) -> Self {
        ParseError::new(ErrorCode::E9002, err.to_string())
    }
}
