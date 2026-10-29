//! Fabrication-file parsers.

pub mod excellon;
pub mod gerber;

pub use excellon::parse_excellon;
pub use gerber::parse_gerber;

use std::fmt;

/// A recoverable file syntax error, with a one-based line number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// Line where the problem was found (0 when not line-specific).
    pub line: usize,
    /// Human-readable description.
    pub message: String,
}

impl ParseError {
    /// Build an error for a line.
    pub fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "{}", self.message)
        } else {
            write!(f, "line {}: {}", self.line, self.message)
        }
    }
}

impl std::error::Error for ParseError {}
