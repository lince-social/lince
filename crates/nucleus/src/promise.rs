// non-lean-yet
// Scope: enum PromiseState, mod tests.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 7.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 71.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 7, 8,
//   9, 10, 11, 12, 13, 14, 18, 30, 43, 55, 71.

use serde::{Deserialize, Serialize};

use crate::error::NucleusError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(schemars::JsonSchema)]
pub enum PromiseState {
    Open,
    Proposed,
    Agreed,
    Active,
    Kept,
    Broken,
    Withdrawn,
}

impl PromiseState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Proposed => "proposed",
            Self::Agreed => "agreed",
            Self::Active => "active",
            Self::Kept => "kept",
            Self::Broken => "broken",
            Self::Withdrawn => "withdrawn",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "open" => Self::Open,
            "proposed" => Self::Proposed,
            "agreed" => Self::Agreed,
            "active" => Self::Active,
            "kept" => Self::Kept,
            "broken" => Self::Broken,
            "withdrawn" => Self::Withdrawn,
            _ => return None,
        })
    }

    pub fn ordinal(self) -> f64 {
        match self {
            Self::Open => 0.0,
            Self::Proposed => 1.0,
            Self::Agreed => 2.0,
            Self::Active => 3.0,
            Self::Kept => 4.0,
            Self::Broken => 5.0,
            Self::Withdrawn => 6.0,
        }
    }

    pub fn can_transition(from: Self, to: Self) -> bool {
        use PromiseState::*;
        matches!(
            (from, to),
            (Open, Proposed)
                | (Open, Withdrawn)
                | (Proposed, Agreed)
                | (Proposed, Withdrawn)
                | (Agreed, Active)
                | (Agreed, Proposed)
                | (Agreed, Withdrawn)
                | (Active, Kept)
                | (Active, Broken)
        )
    }

    pub fn transition(from: Self, to: Self) -> Result<Self, NucleusError> {
        if Self::can_transition(from, to) {
            Ok(to)
        } else {
            Err(NucleusError::InvalidTransition {
                from: from.as_str().into(),
                to: to.as_str().into(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PromiseState::*, *};

    #[test]
    fn happy_path() {
        for (a, b) in [
            (Open, Proposed),
            (Proposed, Agreed),
            (Agreed, Active),
            (Active, Kept),
        ] {
            assert!(PromiseState::can_transition(a, b), "{a:?}->{b:?}");
        }
    }

    #[test]
    fn edits_invalidate_agreement() {
        assert!(PromiseState::can_transition(Agreed, Proposed));
    }

    #[test]
    fn kept_is_terminal_and_settlement_only() {
        assert!(!PromiseState::can_transition(Kept, Broken));
        assert!(
            !PromiseState::can_transition(Proposed, Kept),
            "kept only from active"
        );
    }
}
