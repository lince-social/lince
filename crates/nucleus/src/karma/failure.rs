// non-lean-yet
// Scope: enum FailureCode, enum RetryDisposition, struct FailurePath, fn valid_json_pointer.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::indexing_slicing] Verify bounds or use checked access on fallible inputs.
//   Sites 74, 76.
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 7, 30.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 44.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 7, 8,
//   9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 30, 31, 32, 33, 34, 35, 36,
//   37, 41, 44, 55, 59, 111, 112, 113, 115, 116, 120, 121, 122, 126, 133.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FailurePath(String);

impl FailurePath {
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
    if path.is_empty() {
        return true;
    }
    if !path.starts_with('/') || path.chars().any(char::is_control) {
        return false;
    }
    let bytes = path.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'~' {
            index += 1;
            if index == bytes.len() || !matches!(bytes[index], b'0' | b'1') {
                return false;
            }
        }
        index += 1;
    }
    true
}

impl fmt::Display for FailurePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for FailurePath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KarmaFailure {
    pub code: FailureCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<FailurePath>,
    pub retry: RetryDisposition,
}

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
    match code {
        FailureCode::InvalidDefinition => "invalid-definition",
        FailureCode::InvalidInput => "invalid-input",
        FailureCode::MissingData => "missing-data",
        FailureCode::StaleData => "stale-data",
        FailureCode::DeniedData => "denied-data",
        FailureCode::ProofRejected => "proof-rejected",
        FailureCode::PolicyDenied => "policy-denied",
        FailureCode::AuthorityDenied => "authority-denied",
        FailureCode::Conflict => "conflict",
        FailureCode::StaleRevision => "stale-revision",
        FailureCode::BudgetExhausted => "budget-exhausted",
        FailureCode::FuelExhausted => "fuel-exhausted",
        FailureCode::AdapterUnavailable => "adapter-unavailable",
        FailureCode::RetryableEffect => "retryable-effect",
        FailureCode::TerminalEffect => "terminal-effect",
        FailureCode::UncertainEffect => "uncertain-effect",
        FailureCode::InvariantViolation => "invariant-violation",
        FailureCode::EngineFault => "engine-fault",
    }
}
