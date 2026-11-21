//! Static validation, diagnostics, and deterministic evaluation order for Karma programs.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{CanonicalHash, FailurePath, LocalId, ProgramAst, canonical_hash};

mod expression;
mod graph;
mod node;
mod operation;

/// Canonical hash domain for a typed Program revision.
pub const PROGRAM_REVISION_HASH_DOMAIN: &str = "karma.program-revision.v1";

/// Whether the program can be evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProofStatus {
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProofSeverity {
    /// Rejects the program.
    Error,
    /// Advisory diagnostic.
    Warning,
}

/// Stable categories for static program diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProofIssueCode {
    CanonicalizationFailed,
    EmptyPurpose,
    InvalidType,
    InvalidLiteral,
    DefaultTypeMismatch,
    MissingParameter,
    MissingNode,
    MissingPort,
    InputTypeMismatch,
    UndeclaredExpressionInput,
    ExpressionTypeMismatch,
    InvalidOperationShape,
    InvalidPrecision,
    InvalidReferenceKind,
    TaintDowngrade,
    CombinationalCycle,
}

/// One located diagnostic, with the node IDs involved in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofIssue {
    pub severity: ProofSeverity,
    pub code: ProofIssueCode,
    pub path: FailurePath,
    pub message: String,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub related_nodes: BTreeSet<LocalId>,
}

/// Static validation result and the program's deterministic evaluation order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proof {
    pub status: ProofStatus,
    /// Canonical revision digest, when the program can be canonicalized.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision_hash: Option<CanonicalHash>,
    /// Ready nodes in lexical order at each step, with delay inputs deferred.
    pub evaluation_order: Vec<LocalId>,
    /// Diagnostics sorted by severity, code, path, message, and related nodes.
    pub issues: Vec<ProofIssue>,
}

/// Checks a typed program's ports, operations, sensitivities, and dependencies.
pub fn prove_program(program: &ProgramAst) -> Proof {
    let mut validator = Validator::default();
    validator.validate_program(program);
    let evaluation_order = validator.validate_graph(program);
    let revision_hash = match canonical_hash(PROGRAM_REVISION_HASH_DOMAIN, program) {
        Ok(hash) => Some(hash),
        Err(error) => {
            validator.issue(
                ProofIssueCode::CanonicalizationFailed,
                "",
                error.to_string(),
                BTreeSet::new(),
            );
            None
        }
    };
    validator
        .issues
        .sort_unstable_by(|left, right| issue_key(left).cmp(&issue_key(right)));
    let status = if validator
        .issues
        .iter()
        .any(|issue| issue.severity == ProofSeverity::Error)
    {
        ProofStatus::Rejected
    } else {
        ProofStatus::Accepted
    };
    Proof {
        status,
        revision_hash,
        evaluation_order,
        issues: validator.issues,
    }
}

#[derive(Default)]
struct Validator {
    issues: Vec<ProofIssue>,
}

impl Validator {
    fn issue(
        &mut self,
        code: ProofIssueCode,
        path: impl Into<String>,
        message: impl Into<String>,
        related_nodes: BTreeSet<LocalId>,
    ) {
        // Validated LocalIds and the fixed path segments form valid JSON pointers.
        self.issues.push(ProofIssue {
            severity: ProofSeverity::Error,
            code,
            path: FailurePath::new(path).expect("validator emits valid JSON pointers"),
            message: message.into(),
            related_nodes,
        });
    }
}

fn nodes<'a>(ids: impl IntoIterator<Item = &'a LocalId>) -> BTreeSet<LocalId> {
    ids.into_iter().cloned().collect()
}

fn issue_key(
    issue: &ProofIssue,
) -> (
    ProofSeverity,
    ProofIssueCode,
    &str,
    &str,
    &BTreeSet<LocalId>,
) {
    (
        issue.severity,
        issue.code,
        issue.path.as_str(),
        &issue.message,
        &issue.related_nodes,
    )
}
