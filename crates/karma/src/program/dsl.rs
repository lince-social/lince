//! Versioned Karma program and Frequency text.

use std::fmt;

use serde::{Deserialize, Serialize};

mod format;
mod lexer;
mod parse;

use format::enum_atom;

pub use format::{format_frequency, format_program};
pub use parse::{parse_frequency, parse_program};

/// Maximum source length in bytes, including comments and whitespace.
pub const MAX_DSL_BYTES: usize = 1_048_576;
/// Maximum token count after removing comments and whitespace.
pub const MAX_DSL_TOKENS: usize = 100_000;
/// Maximum UTF-8 byte length of a decoded string.
pub const MAX_DSL_STRING_BYTES: usize = 65_536;
/// Maximum recursive expression, type, or literal depth.
pub const MAX_DSL_NESTING: usize = 64;

/// Stable machine-readable categories for a DSL diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DslErrorKind {
    /// The input exceeds the source byte limit.
    SourceTooLarge,
    /// The input exceeds the token limit.
    TooManyTokens,
    /// A decoded string exceeds the string byte limit.
    StringTooLarge,
    /// Recursive syntax exceeds the nesting limit.
    NestingTooDeep,
    /// A character is invalid at its position in the source.
    UnexpectedCharacter,
    /// A token does not fit the grammar at this position.
    UnexpectedToken,
    /// The source ends before the current construct is complete.
    UnexpectedEnd,
    /// A value or rule in the definition fails validation.
    InvalidAtom,
    /// A declaration or entry is repeated.
    DuplicateDeclaration,
    /// A required declaration is absent.
    MissingDeclaration,
    /// Tokens remain after a complete definition.
    TrailingInput,
}

/// A DSL diagnostic with a UTF-8 byte position and a human-readable message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KarmaDslError {
    /// Stable category for handling or translating the diagnostic.
    pub kind: DslErrorKind,
    /// Zero-based byte offset from the start of the input.
    pub offset: usize,
    /// One-based line number.
    pub line: usize,
    /// One-based byte column within the line.
    pub column: usize,
    /// Additional context for the reported failure.
    pub message: String,
}

impl fmt::Display for KarmaDslError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} at {}:{} (byte {}): {}",
            enum_atom(&self.kind),
            self.line,
            self.column,
            self.offset,
            self.message
        )
    }
}

impl std::error::Error for KarmaDslError {}

#[derive(Clone, Copy)]
struct Position {
    offset: usize,
    line: usize,
    column: usize,
}

impl Position {
    const START: Self = Self {
        offset: 0,
        line: 1,
        column: 1,
    };

    fn advance(&mut self, byte: u8) {
        self.offset += 1;
        if byte == b'\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
    }

    fn error(self, kind: DslErrorKind, message: impl Into<String>) -> KarmaDslError {
        KarmaDslError {
            kind,
            offset: self.offset,
            line: self.line,
            column: self.column,
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DslErrorKind, KarmaDslError, parse_program};

    #[test]
    fn lexical_errors_precede_grammar_errors_and_report_byte_columns() {
        assert_eq!(
            parse_program("wrong\n\"é\";界").unwrap_err(),
            KarmaDslError {
                kind: DslErrorKind::UnexpectedCharacter,
                offset: 11,
                line: 2,
                column: 6,
                message: "Unicode is allowed only inside string literals".into(),
            }
        );
    }
}
