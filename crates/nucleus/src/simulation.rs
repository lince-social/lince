//! The wire model for simulation events, evidence, and reports.
//!
//! [Events](Event) record an observation and its cause at virtual time. Quantities
//! pair an exact decimal with its unit, and [rule occurrences](RuleOccurrence)
//! identify scheduled actions independently of when the simulation observes them.
//!
//! Checks use a [Predicate] to evaluate the observations, with [Coverage] stating
//! where the check ran and a [Finding] retaining evidence for a failure. A [Result]
//! combines those checks with the run's [Verdict] and [Stop]. [ReplayStatus] records
//! whether another execution reproduced that evidence.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    CauseKind,
    karma::{CanonicalHash, DecimalValue, TypedUid},
    schedule::TimeRange,
    valid_uid,
};

pub mod checks;
pub mod sharing;
pub use checks::{
    CheckDefinition, CheckOptions, CheckSet, CheckStatus, Checking, Comparison, CoverageKind,
    CoverageReason, Evaluation, FailureMode, QuantityBasis, RunCost,
};

pub use crate::execution::causal::{CycleKind, RuleChange, RuleCycle, RuleStep, TransferChange};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum Version {
    #[serde(rename = "lince.simulation.v1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct FactId(String);

const MAX_DELIVERY_REQUEST_BYTES: usize = 1024;

impl TryFrom<String> for FactId {
    type Error = String;

    fn try_from(value: String) -> std::result::Result<Self, Self::Error> {
        let regular = valid_uid(&value, "f");
        let delivery = value
            .strip_prefix("tdf:")
            .and_then(|tail| tail.split_once(':'));
        let delivery = delivery.is_some_and(|(operation, request)| {
            matches!(
                operation,
                "configure" | "set-mode" | "revoke" | "enqueue" | "retry"
            ) && !request.is_empty()
                && request.len() <= MAX_DELIVERY_REQUEST_BYTES
                && !request.chars().any(char::is_control)
        });
        let application = value
            .strip_prefix("taf:taa:")
            .is_some_and(|uid| valid_uid(uid, "tla"));
        if regular || delivery || application {
            Ok(Self(value))
        } else {
            Err("invalid Fact identity".into())
        }
    }
}

impl From<FactId> for String {
    fn from(value: FactId) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Quantity {
    pub value: DecimalValue,
    pub unit: Option<TypedUid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuleOccurrence {
    pub rule_uid: String,
    pub revision: u64,
    pub event_id: String,
    pub frequency: Option<TypedUid>,
    pub intended_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordedSchedule {
    pub cell: String,
    pub occurrence: RuleOccurrence,
    pub record: TypedUid,
    pub observed_at_ms: i64,
    pub time: Option<TimeRange>,
    pub unavailable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Cause {
    Input {
        id: String,
    },
    Rule {
        occurrence: RuleOccurrence,
        consequence: u32,
    },
    Fact {
        cause_kind: CauseKind,
        uid: Option<String>,
    },
    Delivery {
        message: u64,
    },
    Timer {},
    Seed {},
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Refusal {
    Forbidden {},
    UnknownRecord { reference: String },
    Conflict { code: String },
    InvalidAction {},
    UnsupportedBoundary {},
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Observation {
    RuleCycle {
        cycle: RuleCycle,
    },
    LoanQuantity {
        record: TypedUid,
        before: Quantity,
        after: Quantity,
        physical: Quantity,
        at_ms: i64,
    },
    LinguaImported {
        files: BTreeMap<String, CanonicalHash>,
        created: Vec<String>,
        updated: Vec<String>,
    },
    LinguaInterrupted {
        files: BTreeMap<String, CanonicalHash>,
    },
    CommittedQuantity {
        record: TypedUid,
        fact: FactId,
        before: Quantity,
        after: Quantity,
        delta: DecimalValue,
        at_ms: i64,
        cause: Cause,
        previous_fact_hash: String,
        fact_hash: String,
        #[serde(default)]
        commit: Option<i64>,
    },
    RuleApplication {
        occurrence: RuleOccurrence,
        status: ApplicationStatus,
        reason: Option<String>,
    },
    RuleSchedule {
        schedule: RecordedSchedule,
    },
    ActionAccepted {
        input: String,
        created: Option<String>,
    },
    ActionRefused {
        input: String,
        refusal: Refusal,
    },
    ActionInterrupted {
        input: String,
    },
    DatabaseEffect {
        uid: String,
        effect: DatabaseEffectKind,
        ok: bool,
        result: String,
    },
    MessageQueued {
        message: u64,
        to: String,
        payload_hash: CanonicalHash,
    },
    MessageDelivered {
        message: u64,
        from: String,
        imported: u64,
    },
    MessageDropped {
        message: u64,
        reason: DropReason,
    },
    MessageRefused {
        message: u64,
        from: String,
        refusal: Refusal,
    },
    TransferReceived {
        message: u64,
        from: String,
        receipt: bool,
    },
    Discovered {
        peer: String,
        organ: TypedUid,
    },
    Enrolled {
        host: String,
        organ: TypedUid,
        roster_version: u64,
    },
    LinkChanged {
        peer: String,
        connected: bool,
    },
    Restarted {},
    AvailabilityChanged {
        online: bool,
    },
    ClockChanged {
        offset_ms: i64,
    },
    State {
        hash: CanonicalHash,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ApplicationStatus {
    Applied,
    Blocked,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum DropReason {
    Partition,
    ScriptedLoss,
    Offline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub sequence: u64,
    pub virtual_ms: i64,
    pub cell: String,
    pub caused_by: Cause,
    pub observation: Observation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Predicate {
    NoRuleCycles {
        #[serde(default)]
        include_timed_recurrence: bool,
    },
    Quantity {
        cell: String,
        record: String,
        comparison: Comparison,
        expected: Quantity,
    },
    QuantityEquals {
        cell: String,
        record: String,
        expected: Quantity,
        at_ms: i64,
    },
    QuantitiesEqual {
        cell: String,
        records: BTreeMap<String, Quantity>,
        at_ms: i64,
    },
    Nonnegative {
        cell: String,
        record: String,
    },
    OncePerOccurrence {},
    NoUnexpectedRefusals {},
    FactChain {},
    Converged {
        cells: Vec<String>,
        record: String,
        at_ms: i64,
    },
    ExpectedRefusal {
        input: String,
        refusal: Refusal,
    },
    ExpectedMessageRefusal {
        input: String,
        copy: u8,
        refusal: Refusal,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Check {
    pub id: String,
    pub implementation: CanonicalHash,
    pub predicate: Predicate,
    pub options: CheckOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Witness {
    RuleCycle {
        cycle: RuleCycle,
    },
    RefusedRule {
        occurrence: RuleOccurrence,
        reason: Option<String>,
    },
    RefusedEffect {
        uid: String,
        effect: DatabaseEffectKind,
        reason: String,
    },
    Quantity {
        record: TypedUid,
        expected: Quantity,
        observed: Quantity,
    },
    DuplicateApplication {
        occurrence: RuleOccurrence,
        consequence: Option<u32>,
        expected: u64,
        observed: u64,
        commits: Vec<u64>,
    },
    BrokenFactChain {
        fact: FactId,
        previous: String,
        expected: String,
    },
    DivergentCells {
        record: String,
        values: Vec<CellQuantity>,
        deadline_ms: i64,
    },
    UnexpectedActionResult {
        input: String,
        expected: Refusal,
        observed: Option<Refusal>,
    },
    UnexpectedMessageResult {
        message: u64,
        expected: Refusal,
        observed: Option<Refusal>,
    },
    MissingRecord {
        reference: String,
    },
    RefusedAction {
        input: String,
        refusal: Refusal,
    },
    RefusedMessage {
        message: u64,
        refusal: Refusal,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum DatabaseEffectKind {
    Command,
    Action,
    Consequence,
    Notify,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CellQuantity {
    pub cell: String,
    pub quantity: Quantity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    pub check: Check,
    pub sequence: u64,
    pub virtual_ms: i64,
    pub cells: Vec<String>,
    pub witness: Witness,
    pub evidence: Vec<u64>,
    pub before_state: CanonicalHash,
    pub after_state: CanonicalHash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    Passed,
    Failed,
    Inconclusive,
    Unverified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Stop {
    UnsupportedEffect {
        cell: String,
    },
    HorizonReached {},
    EventBudget {},
    RuleEvaluationBudget {},
    WallTimeBudget {},
    EvidenceBudget {},
    CheckBudget {},
    CheckFailed {
        check: String,
    },
    Cancelled {},
    Paused {},
    ExecutionError {
        cell: String,
        input: Option<String>,
        category: ExecutionError,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutionError {
    Storage,
    Domain,
    Io,
    Serialization,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Coverage {
    pub check: String,
    pub observations: u64,
    pub complete: bool,
    pub status: CheckStatus,
    pub kind: CoverageKind,
    pub from_ms: i64,
    pub until_ms: i64,
    pub last_evaluated_ms: Option<i64>,
    pub reason: Option<CoverageReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Result {
    pub version: Version,
    pub stop: Stop,
    pub verdict: Verdict,
    pub stopped_at_ms: i64,
    pub steps: u64,
    pub rule_evaluations: u64,
    pub execution_checkpoints: u64,
    pub execution_interrupted: bool,
    pub cycles: Vec<RuleCycle>,
    pub inputs: u64,
    pub events: u64,
    pub findings: u64,
    pub coverage: Vec<Coverage>,
    pub final_state: CanonicalHash,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ReplayStatus {
    Unverified {},
    Verified {
        trace: CanonicalHash,
        reproduced_checks: Vec<String>,
    },
    Diverged {
        sequence: u64,
        field: String,
        expected: String,
        observed: String,
    },
    Unavailable {
        reason: ArtifactError,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ArtifactError {
    Incomplete {
        path: String,
    },
    Corrupt {
        path: String,
        expected: String,
        observed: String,
    },
    Invalid {
        path: String,
        reason: String,
    },
    BuildMismatch {
        expected: String,
        observed: String,
    },
    MissingEvidence {
        check: String,
    },
}

#[cfg(test)]
mod tests;
