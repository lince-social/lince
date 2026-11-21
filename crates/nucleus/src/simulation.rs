// non-lean-yet
// Scope: mod checks, mod sharing, enum Version, struct FactId.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 27.
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 14, 86, 107, 117, 225, 233, 251, 307, 367, 396, 405, 429, 471, 490.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 5, 6,
//   14, 16, 21, 58, 59, 60, 65, 66, 67, 68, 69, 70, 75, 76, 77, 78, 79, 80, 81, 86, 87, 88, 90, 91,
//   92, 94, 95, 96, 98, 99, 101, 102, 107, 108, 109, 110, 111, 112, 117, 118, 119, 121, 122, 123,
//   124, 125, 126, 128, 129, 130, 131, 133, 134, 136, 137, 138, 139, 140, 141, 142, 143, 144, 145,
//   147, 149, 150, 151, 152, 154, 155, 157, 158, 159, 161, 162, 163, 165, 166, 168, 169, 170, 171,
//   172, 174, 175, 176, 177, 179, 180, 181, 182, 184, 185, 186, 188, 189, 190, 191, 193, 194, 195,
//   196, 198, 199, 200, 202, 203, 204, 205, 207, 208, 209, 211, 212, 213, 215, 216, 218, 219, 225,
//   226, 227, 228, 233, 234, 235, 236, 241, 242, 243, 244, 245, 246, 251, 252, 254, 256, 257, 258,
//   259, 260, 262, 263, 264, 265, 266, 268, 269, 270, 271, 273, 274, 275, 277, 278, 279, 280, 281,
//   282, 283, 285, 286, 287, 289, 290, 291, 292, 298, 299, 300, 301, 302, 307, 308, 309, 311, 312,
//   313, 315, 316, 317, 318, 320, 321, 322, 323, 325, 326, 327, 328, 329, 330, 332, 333, 334, 335,
//   337, 338, 339, 340, 342, 343, 344, 345, 347, 348, 349, 350, 352, 353, 355, 356, 357, 359, 360,
//   361, 367, 368, 369, 370, 371, 376, 377, 378, 383, 384, 385, 386, 387, 388, 389, 390, 391, 396,
//   397, 398, 399, 400, 405, 406, 407, 409, 410, 411, 412, 413, 414, 415, 416, 418, 419, 420, 421,
//   422, 423, 429, 430, 431, 432, 433, 438, 439, 440, 441, 442, 443, 444, 445, 446, 447, 452, 453,
//   454, 455, 456, 457, 458, 459, 460, 461, 462, 463, 464, 465, 466, 471, 472, 473, 474, 475, 477,
//   478, 479, 480, 481, 483, 484, 490, 491, 492, 494, 495, 496, 497, 499, 500, 501, 503, 504, 505,
//   507, 508.

use serde::{Deserialize, Serialize};

use crate::karma::{CanonicalHash, DecimalValue, TypedUid};

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

impl TryFrom<String> for FactId {
    type Error = String;

    fn try_from(value: String) -> std::result::Result<Self, Self::Error> {
        let regular = TypedUid::new(crate::karma::ReferenceKind::Fact, value.clone()).is_ok();
        let delivery = value
            .strip_prefix("tdf:")
            .and_then(|tail| tail.split_once(':'));
        let delivery = delivery.is_some_and(|(operation, request)| {
            matches!(
                operation,
                "configure" | "set-mode" | "revoke" | "enqueue" | "retry"
            ) && !request.is_empty()
                && request.len() <= 1024
                && !request.chars().any(char::is_control)
        });
        let application = value
            .strip_prefix("taf:taa:")
            .is_some_and(|uid| crate::valid_uid(uid, "tla"));
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
    pub time: Option<crate::schedule::TimeRange>,
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
        cause_kind: crate::CauseKind,
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
        files: std::collections::BTreeMap<String, CanonicalHash>,
        created: Vec<String>,
        updated: Vec<String>,
    },
    LinguaInterrupted {
        files: std::collections::BTreeMap<String, CanonicalHash>,
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
        records: std::collections::BTreeMap<String, Quantity>,
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
mod tests {
    use super::*;

    #[test]
    fn delivery_fact_identities_round_trip_and_reject_unknown_operations() {
        for operation in ["configure", "set-mode", "revoke", "enqueue", "retry"] {
            let value = format!("tdf:{operation}:request");
            let fact = FactId::try_from(value.clone()).unwrap();
            let encoded = serde_json::to_string(&fact).unwrap();
            assert_eq!(serde_json::from_str::<FactId>(&encoded).unwrap(), fact);
            assert_eq!(String::from(fact), value);
        }
        let application = "taf:taa:tla_01Q3DCBD008RR8522CDK0Y63C5".to_owned();
        assert_eq!(
            String::from(FactId::try_from(application.clone()).unwrap()),
            application
        );
        for value in [
            "tdf:unknown:request",
            "tdf:enqueue:",
            "tdf:retry:bad\nrequest",
            "taf:taa:tla_unknown",
        ] {
            assert!(FactId::try_from(value.to_owned()).is_err());
        }
    }

    #[test]
    fn semantic_contract_rejects_unknown_fields_kinds_and_versions() {
        assert!(serde_json::from_str::<Version>("\"lince.simulation.v2\"").is_err());
        assert!(
            serde_json::from_str::<Stop>(r#"{"kind":"horizon-reached","ignored":true}"#).is_err()
        );
        assert!(serde_json::from_str::<Observation>(r#"{"kind":"unknown"}"#).is_err());
        assert!(serde_json::from_str::<Quantity>(r#"{"value":"NaN","unit":null}"#).is_err());
    }

    #[test]
    fn empty_report_variants_reject_unrecognized_fields() {
        fn strict<T: Serialize + serde::de::DeserializeOwned>(variants: &[T]) {
            for variant in variants {
                let mut value = serde_json::to_value(variant).unwrap();
                assert!(serde_json::from_value::<T>(value.clone()).is_ok());
                value
                    .as_object_mut()
                    .unwrap()
                    .insert("ignored".into(), true.into());
                assert!(serde_json::from_value::<T>(value).is_err());
            }
        }
        strict(&[Cause::Timer {}, Cause::Seed {}]);
        strict(&[
            Refusal::Forbidden {},
            Refusal::InvalidAction {},
            Refusal::UnsupportedBoundary {},
        ]);
        strict(&[Observation::Restarted {}]);
        strict(&[
            Predicate::OncePerOccurrence {},
            Predicate::NoUnexpectedRefusals {},
            Predicate::FactChain {},
        ]);
        strict(&[
            Stop::HorizonReached {},
            Stop::EventBudget {},
            Stop::EvidenceBudget {},
            Stop::Cancelled {},
            Stop::Paused {},
        ]);
        strict(&[ReplayStatus::Unverified {}]);
        use crate::projection::{Incomplete, Status};
        strict(&[
            Incomplete::Budget {},
            Incomplete::ExternalEffects {},
            Incomplete::RuleFailure {},
            Incomplete::UnavailableRuntime {},
            Incomplete::PastWindow {},
            Incomplete::UnsupportedFilter {},
        ]);
        strict(&[Status::Updating {}]);
    }
}
