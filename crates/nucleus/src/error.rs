// non-lean-yet
// Scope: enum NucleusError.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 4.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 4, 5,
//   6, 7, 8, 9, 10, 11.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum NucleusError {
    Parse(String),
    Eval(String),
    UnknownToken(String),
    InvalidGate(String),
    InvalidCarry(String),
    InvalidTransition { from: String, to: String },
    InvalidSlug(String),
}

impl fmt::Display for NucleusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(m) => write!(f, "parse error: {m}"),
            Self::Eval(m) => write!(f, "eval error: {m}"),
            Self::UnknownToken(m) => write!(f, "unknown token: {m}"),
            Self::InvalidGate(m) => write!(f, "invalid gate `{m}`"),
            Self::InvalidCarry(m) => write!(f, "invalid carry `{m}`"),
            Self::InvalidTransition { from, to } => {
                write!(f, "invalid promise transition {from} -> {to}")
            }
            Self::InvalidSlug(m) => write!(f, "invalid slug `{m}`"),
        }
    }
}

impl std::error::Error for NucleusError {}
