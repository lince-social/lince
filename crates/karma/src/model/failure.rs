use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, de};

/// The stable category of a Karma failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FailureCode {
    InvalidDefinition,
    InvalidInput,
    MissingData,
    StaleData,
    DeniedData,
    ProofRejected,
    PolicyDenied,
    AuthorityDenied,
    Conflict,
    StaleRevision,
    BudgetExhausted,
    FuelExhausted,
    AdapterUnavailable,
    RetryableEffect,
    TerminalEffect,
    UncertainEffect,
    InvariantViolation,
    EngineFault,
}

/// The condition under which a failed operation may be retried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RetryDisposition {
    Never,
    AfterInput,
    AfterPolicyChange,
    OnConflict,
    WithBackoff,
    NeedsReconciliation,
    OperatorRequired,
}

/// A validated JSON pointer to the value associated with a failure.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct FailurePath(String);

impl FailurePath {
    /// Validates and stores a pointer. The empty string denotes the root.
    ///
    /// # Errors
    /// Rejects malformed JSON pointers and pointers containing control characters.
    pub fn new(path: impl Into<String>) -> Result<Self, KarmaBoundaryError> {
        let path = path.into();
        if valid_json_pointer(&path) {
            Ok(Self(path))
        } else {
            Err(KarmaBoundaryError::invalid_input(
                "failure path must be an RFC 6901 JSON pointer",
            ))
        }
    }

    pub fn root() -> Self {
        Self(String::new())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn valid_json_pointer(path: &str) -> bool {
    let Some(tail) = path.strip_prefix('/') else {
        return path.is_empty();
    };
    let mut chars = tail.chars();
    while let Some(ch) = chars.next() {
        if ch.is_control() || ch == '~' && !matches!(chars.next(), Some('0' | '1')) {
            return false;
        }
    }
    true
}

impl fmt::Display for FailurePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for FailurePath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let path = String::deserialize(deserializer)?;
        Self::new(path).map_err(de::Error::custom)
    }
}

/// A recorded failure with diagnostic and recovery information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KarmaFailure {
    pub code: FailureCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<FailurePath>,
    pub retry: RetryDisposition,
}

/// An error validating an input or definition at a Karma boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KarmaBoundaryError {
    pub code: FailureCode,
    pub message: String,
}

impl KarmaBoundaryError {
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self {
            code: FailureCode::InvalidInput,
            message: message.into(),
        }
    }

    pub fn invalid_definition(message: impl Into<String>) -> Self {
        Self {
            code: FailureCode::InvalidDefinition,
            message: message.into(),
        }
    }
}

impl fmt::Display for KarmaBoundaryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", failure_code_name(self.code), self.message)
    }
}

impl std::error::Error for KarmaBoundaryError {}

fn failure_code_name(code: FailureCode) -> &'static str {
    use FailureCode::*;
    match code {
        InvalidDefinition => "invalid-definition",
        InvalidInput => "invalid-input",
        MissingData => "missing-data",
        StaleData => "stale-data",
        DeniedData => "denied-data",
        ProofRejected => "proof-rejected",
        PolicyDenied => "policy-denied",
        AuthorityDenied => "authority-denied",
        Conflict => "conflict",
        StaleRevision => "stale-revision",
        BudgetExhausted => "budget-exhausted",
        FuelExhausted => "fuel-exhausted",
        AdapterUnavailable => "adapter-unavailable",
        RetryableEffect => "retryable-effect",
        TerminalEffect => "terminal-effect",
        UncertainEffect => "uncertain-effect",
        InvariantViolation => "invariant-violation",
        EngineFault => "engine-fault",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_escapes_consume_one_digit_and_preserve_unicode() {
        for path in ["", "/", "/é/~01", "/~0~1"] {
            assert_eq!(FailurePath::new(path).unwrap().as_str(), path);
        }
        for path in ["/bad~", "/~é", "/~~0", "/état\u{0085}"] {
            assert!(FailurePath::new(path).is_err(), "{path:?}");
        }
    }
}
