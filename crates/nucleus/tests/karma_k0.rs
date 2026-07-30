use std::collections::BTreeMap;

use nucleus::karma::{
    CandidateStatus, CanonicalHash, Capability, CapabilityFamily, CapabilitySet, Confidence,
    DatumState, DefinitionStatus, DurationMs, EngineMode, FailureCode, FailurePath,
    FixedDecimal, IntentStatus, KarmaActionKind, KarmaFailure, KarmaObjectKind, LocalId,
    Probability, ReferenceKind, ResolvedReference, RetryDisposition, RunStatus, Slug, TimestampMs,
    TypedUid, WorkflowStatus, canonical_hash, canonical_json_bytes,
};
use serde::Serialize;
use serde_json::json;

const RECORD_UID: &str = "r_01ARZ3NDEKTSV4RRFFQ69G5FAV";
const FACT_UID: &str = "f_01ARZ3NDEKTSV4RRFFQ69G5FAV";

#[derive(Serialize)]
struct GoldenFixture {
    actions: Vec<KarmaActionKind>,
    candidate_statuses: Vec<CandidateStatus>,
    capabilities: CapabilitySet,
    capability_families: Vec<CapabilityFamily>,
    confidence: Confidence,
    datum_states: Vec<DatumState>,
    decimal: FixedDecimal<4>,
    definition_statuses: Vec<DefinitionStatus>,
    duration: DurationMs,
    engine_modes: Vec<EngineMode>,
    failure: KarmaFailure,
    failure_codes: Vec<FailureCode>,
    fixture_hash: CanonicalHash,
    intent_statuses: Vec<IntentStatus>,
    local_id: LocalId,
    object_kinds: Vec<KarmaObjectKind>,
    probability: Probability,
    references: BTreeMap<String, ResolvedReference>,
    reference_kinds: Vec<ReferenceKind>,
    retry_dispositions: Vec<RetryDisposition>,
    run_statuses: Vec<RunStatus>,
    timestamp: TimestampMs,
    workflow_statuses: Vec<WorkflowStatus>,
}

#[test]
fn complete_k0_fixture_has_stable_canonical_hash() {
    let fixture = golden_fixture();
    let bytes = canonical_json_bytes(&fixture).expect("canonical fixture");
    let encoded = String::from_utf8(bytes).expect("canonical JSON is UTF-8");

    assert!(
        encoded.starts_with("{\"actions\":"),
        "object fields are lexicographically ordered: {encoded}"
    );
    assert!(
        encoded.contains(&format!(
            "\"confidence\":\"0.650000000\""
        )),
        "exact atoms use canonical strings: {encoded}"
    );
    assert!(
        encoded.contains("\"references\":{\"apple\":"),
        "reverse-inserted map is sorted: {encoded}"
    );
    assert!(
        encoded.contains("\"timestamp\":\"2026-07-21T08:00:00.125Z\""),
        "timestamp retains exactly milliseconds: {encoded}"
    );

    let hash = canonical_hash("karma.i0-golden.v1", &fixture).expect("fixture hash");
    assert_eq!(
        hash.as_str(),
        "sha256:26df83658ab409053ff1b83900243fe6dbdbb26fbbe22431fcb441d204b596e8",
        "a public K0 wire change must deliberately update this golden hash"
    );
}

#[test]
fn exact_atoms_reject_precision_loss_and_noncanonical_forms() {
    let decimal: FixedDecimal<4> = "12.3400".parse().unwrap();
    assert_eq!(decimal.mantissa(), 123_400);
    assert_eq!(decimal.to_string(), "12.3400");
    assert_eq!(
        decimal
            .checked_add("0.6600".parse().unwrap())
            .unwrap()
            .to_string(),
        "13.0000"
    );
    assert!("12.34".parse::<FixedDecimal<4>>().is_err());
    assert!("012.3400".parse::<FixedDecimal<4>>().is_err());
    assert!("-0.0000".parse::<FixedDecimal<4>>().is_err());
    assert!(FixedDecimal::<19>::from_mantissa(1).is_err());

    assert_eq!(
        "0.720000000"
            .parse::<Probability>()
            .unwrap()
            .parts_per_billion(),
        720_000_000
    );
    assert_eq!(Probability::ONE.to_string(), "1.000000000");
    assert_eq!(Confidence::ZERO.to_string(), "0.000000000");
    assert!("0.72".parse::<Probability>().is_err());
    assert!("1.000000001".parse::<Probability>().is_err());
    assert!(serde_json::from_str::<Confidence>("0.5").is_err());
}

#[test]
fn timestamp_boundary_accepts_only_normalized_utc_milliseconds() {
    let timestamp = TimestampMs::parse_canonical("2026-07-21T08:00:00.125Z").unwrap();
    assert_eq!(timestamp.to_string(), "2026-07-21T08:00:00.125Z");
    assert_eq!(
        timestamp
            .checked_add(DurationMs::new(3))
            .unwrap()
            .to_string(),
        "2026-07-21T08:00:00.128Z"
    );
    assert!(TimestampMs::parse_canonical("2026-07-21T08:00:00Z").is_err());
    assert!(TimestampMs::parse_canonical("2026-07-21T05:00:00.125-03:00").is_err());
    assert!(TimestampMs::parse_canonical("2026-07-21T08:00:00.1250Z").is_err());
    assert!(TimestampMs::from_millis(i64::MAX).is_err());
}

#[test]
fn references_validate_kind_uid_and_names_independently() {
    let record = TypedUid::new(ReferenceKind::Program, RECORD_UID).unwrap();
    assert_eq!(record.kind(), ReferenceKind::Program);
    assert_eq!(record.as_str(), RECORD_UID);
    assert!(TypedUid::new(ReferenceKind::Fact, RECORD_UID).is_err());
    assert!(TypedUid::new(ReferenceKind::Program, FACT_UID).is_err());
    assert!(TypedUid::new(ReferenceKind::Program, RECORD_UID.to_lowercase()).is_err());

    assert!(Slug::new("household.apple-restock").is_ok());
    assert!(Slug::new("household.apple_restock").is_err());
    assert!(LocalId::new("min_confidence").is_ok());
    assert!(LocalId::new("MinConfidence").is_err());
}

#[test]
fn paths_enums_hashes_and_capabilities_fail_closed() {
    assert!(FailurePath::new("").is_ok());
    assert!(FailurePath::new("/definition/nodes/0").is_ok());
    assert!(FailurePath::new("definition/nodes").is_err());
    assert!(FailurePath::new("/bad~escape").is_err());

    assert!(serde_json::from_str::<RunStatus>("\"unknown\"").is_err());
    assert!(serde_json::from_str::<Capability>("\"transfer.automatic\"").is_err());
    assert!(CanonicalHash::parse(format!("sha256:{}", "a".repeat(64))).is_ok());
    assert!(CanonicalHash::parse(format!("sha256:{}", "A".repeat(64))).is_err());

    let set = CapabilitySet::new([
        Capability::TransferSettleLocal,
        Capability::KarmaRead,
        Capability::TransferSettleLocal,
    ]);
    assert_eq!(set.iter().count(), 2);
    assert!(set.contains(Capability::KarmaRead));
    assert_eq!(
        Capability::TransferDraftLocal.family(),
        CapabilityFamily::TransferPreparation
    );
    assert_eq!(
        Capability::TransferAgreeOwn.family(),
        CapabilityFamily::TransferCommitment
    );
}

#[test]
fn canonical_json_sorts_keys_rejects_floats_and_separates_domains() {
    let mut reverse = BTreeMap::new();
    reverse.insert("z", 2_u64);
    reverse.insert("a", 1_u64);
    assert_eq!(canonical_json_bytes(&reverse).unwrap(), br#"{"a":1,"z":2}"#);
    assert!(canonical_json_bytes(&json!({ "not_exact": 1.5 })).is_err());
    assert!(canonical_hash("Karma.invalid.v1", &reverse).is_err());
    assert!(canonical_hash("karma.missing-version", &reverse).is_err());

    let first = canonical_hash("karma.program.v1", &reverse).unwrap();
    let second = canonical_hash("karma.run.v1", &reverse).unwrap();
    assert_ne!(first, second, "semantic domains cannot alias");
}

fn golden_fixture() -> GoldenFixture {
    let mut references = BTreeMap::new();
    references.insert(
        "zebra".to_string(),
        ResolvedReference {
            target: TypedUid::new(ReferenceKind::Fact, FACT_UID).unwrap(),
            display_slug: None,
        },
    );
    references.insert(
        "apple".to_string(),
        ResolvedReference {
            target: TypedUid::new(ReferenceKind::Program, RECORD_UID).unwrap(),
            display_slug: Some(Slug::new("household.apple-restock").unwrap()),
        },
    );

    GoldenFixture {
        actions: vec![
            KarmaActionKind::ValidateKarmaDefinition,
            KarmaActionKind::CreateKarmaProgram,
            KarmaActionKind::ForkKarmaProgram,
            KarmaActionKind::ReviseKarmaProgram,
            KarmaActionKind::ActivateKarmaRevision,
            KarmaActionKind::CreateKarmaFrequency,
            KarmaActionKind::ReviseKarmaFrequency,
            KarmaActionKind::ActivateKarmaFrequencyRevision,
            KarmaActionKind::SetKarmaParameter,
            KarmaActionKind::ResetKarmaParameter,
            KarmaActionKind::Deactivate,
            KarmaActionKind::Activate,
            KarmaActionKind::RetireKarmaProgram,
            KarmaActionKind::RunKarmaProgram,
            KarmaActionKind::ReplayKarmaRun,
            KarmaActionKind::RebuildKarmaModel,
            KarmaActionKind::DisableKarmaModel,
            KarmaActionKind::CreateKarmaGrant,
            KarmaActionKind::NarrowKarmaGrant,
            KarmaActionKind::RevokeKarmaGrant,
            KarmaActionKind::CreateAutomationTrustScope,
            KarmaActionKind::ReviseAutomationTrustScope,
            KarmaActionKind::ActivateAutomationTrustRevision,
            KarmaActionKind::RespondKarmaCandidate,
            KarmaActionKind::Decide,
            KarmaActionKind::ControlKarmaWorkflow,
            KarmaActionKind::ControlKarmaIntent,
            KarmaActionKind::SimulateKarmaProgram,
            KarmaActionKind::ImportKarmaTemplate,
        ],
        candidate_statuses: vec![
            CandidateStatus::Proposed,
            CandidateStatus::Accepted,
            CandidateStatus::Edited,
            CandidateStatus::Dismissed,
            CandidateStatus::Snoozed,
            CandidateStatus::Muted,
            CandidateStatus::Stale,
            CandidateStatus::Expired,
        ],
        capabilities: CapabilitySet::new(all_capabilities()),
        capability_families: vec![
            CapabilityFamily::ReadAnalyze,
            CapabilityFamily::LocalReversibleData,
            CapabilityFamily::AttentionPresentation,
            CapabilityFamily::ProgramMetaControl,
            CapabilityFamily::ExternalResource,
            CapabilityFamily::TransferPreparation,
            CapabilityFamily::TransferSocial,
            CapabilityFamily::TransferCommitment,
            CapabilityFamily::IrreversibleSafetyCritical,
        ],
        confidence: Confidence::from_parts_per_billion(650_000_000).unwrap(),
            datum_states: vec![
            DatumState::Value,
            DatumState::Missing,
            DatumState::Stale,
            DatumState::Denied,
            DatumState::Invalid,
        ],
        decimal: "12.3400".parse().unwrap(),
        definition_statuses: vec![
            DefinitionStatus::Draft,
            DefinitionStatus::Proven,
            DefinitionStatus::Shadow,
            DefinitionStatus::Active,
            DefinitionStatus::Paused,
            DefinitionStatus::Superseded,
            DefinitionStatus::Retired,
        ],
        duration: DurationMs::new(3),
        engine_modes: vec![
            EngineMode::Normal,
            EngineMode::ObserveOnly,
            EngineMode::StageEffects,
            EngineMode::EmergencyStop,
            EngineMode::Maintenance,
        ],
        failure: KarmaFailure {
            code: FailureCode::AuthorityDenied,
            message: "grant does not include transfer agreement".to_string(),
            path: Some(FailurePath::new("/intent/capability").unwrap()),
            retry: RetryDisposition::AfterPolicyChange,
        },
        failure_codes: vec![
            FailureCode::InvalidDefinition,
            FailureCode::InvalidInput,
            FailureCode::MissingData,
            FailureCode::StaleData,
            FailureCode::DeniedData,
            FailureCode::ProofRejected,
            FailureCode::PolicyDenied,
            FailureCode::AuthorityDenied,
            FailureCode::Conflict,
            FailureCode::StaleRevision,
            FailureCode::BudgetExhausted,
            FailureCode::FuelExhausted,
            FailureCode::AdapterUnavailable,
            FailureCode::RetryableEffect,
            FailureCode::TerminalEffect,
            FailureCode::UncertainEffect,
            FailureCode::InvariantViolation,
            FailureCode::EngineFault,
        ],
        fixture_hash: CanonicalHash::parse(format!("sha256:{}", "0".repeat(64))).unwrap(),
        intent_statuses: vec![
            IntentStatus::Staged,
            IntentStatus::Authorized,
            IntentStatus::Leased,
            IntentStatus::Dispatching,
            IntentStatus::Succeeded,
            IntentStatus::Failed,
            IntentStatus::Cancelled,
            IntentStatus::Uncertain,
            IntentStatus::Compensated,
            IntentStatus::DeadLetter,
        ],
        local_id: LocalId::new("min_confidence").unwrap(),
        object_kinds: vec![
            KarmaObjectKind::Program,
            KarmaObjectKind::ProgramRevision,
            KarmaObjectKind::Frequency,
            KarmaObjectKind::FrequencyRevision,
            KarmaObjectKind::Occurrence,
            KarmaObjectKind::Run,
            KarmaObjectKind::EvidenceSet,
            KarmaObjectKind::Model,
            KarmaObjectKind::ModelCheckpoint,
            KarmaObjectKind::Candidate,
            KarmaObjectKind::Decision,
            KarmaObjectKind::Grant,
            KarmaObjectKind::TrustScope,
            KarmaObjectKind::ActionIntent,
            KarmaObjectKind::Attempt,
            KarmaObjectKind::Receipt,
            KarmaObjectKind::Workflow,
            KarmaObjectKind::Simulation,
            KarmaObjectKind::EngineHealth,
        ],
        probability: Probability::from_parts_per_billion(720_000_000).unwrap(),
        references,
        reference_kinds: vec![
            ReferenceKind::Record,
            ReferenceKind::Fact,
            ReferenceKind::Person,
            ReferenceKind::Organ,
            ReferenceKind::Place,
            ReferenceKind::Concept,
            ReferenceKind::Unit,
            ReferenceKind::Link,
            ReferenceKind::Promise,
            ReferenceKind::Transfer,
            ReferenceKind::Program,
            ReferenceKind::ProgramRevision,
            ReferenceKind::Node,
            ReferenceKind::Signal,
            ReferenceKind::Frequency,
            ReferenceKind::Sense,
            ReferenceKind::View,
            ReferenceKind::Model,
            ReferenceKind::Objective,
            ReferenceKind::Workflow,
            ReferenceKind::Grant,
            ReferenceKind::TrustScope,
            ReferenceKind::Run,
            ReferenceKind::Candidate,
            ReferenceKind::Decision,
            ReferenceKind::Intent,
            ReferenceKind::Receipt,
            ReferenceKind::Simulation,
        ],
        retry_dispositions: vec![
            RetryDisposition::Never,
            RetryDisposition::AfterInput,
            RetryDisposition::AfterPolicyChange,
            RetryDisposition::OnConflict,
            RetryDisposition::WithBackoff,
            RetryDisposition::NeedsReconciliation,
            RetryDisposition::OperatorRequired,
        ],
        run_statuses: vec![
            RunStatus::Queued,
            RunStatus::Evaluating,
            RunStatus::Staged,
            RunStatus::Waiting,
            RunStatus::Executing,
            RunStatus::Completed,
            RunStatus::Failed,
            RunStatus::Cancelled,
            RunStatus::DeadLetter,
        ],
        timestamp: TimestampMs::parse_canonical("2026-07-21T08:00:00.125Z").unwrap(),
        workflow_statuses: vec![
            WorkflowStatus::Queued,
            WorkflowStatus::Running,
            WorkflowStatus::Waiting,
            WorkflowStatus::Compensating,
            WorkflowStatus::Completed,
            WorkflowStatus::Failed,
            WorkflowStatus::Cancelled,
        ],
    }
}

fn all_capabilities() -> Vec<Capability> {
    vec![
        Capability::KarmaRead,
        Capability::KarmaEvaluate,
        Capability::KarmaAuthor,
        Capability::KarmaActivate,
        Capability::KarmaRun,
        Capability::KarmaManage,
        Capability::KarmaGrantNarrow,
        Capability::KarmaGrantWiden,
        Capability::RecordSetQuantity,
        Capability::RecordAddQuantity,
        Capability::LinkCreate,
        Capability::MetadataWrite,
        Capability::TaskCreate,
        Capability::AttentionDecide,
        Capability::AttentionNotify,
        Capability::InterfaceControl,
        Capability::ExternalHttp,
        Capability::ExternalCommand,
        Capability::ExternalFilesystem,
        Capability::ExternalNetwork,
        Capability::ExternalPayment,
        Capability::DeviceControl,
        Capability::TransferRead,
        Capability::TransferProject,
        Capability::TransferDraftLocal,
        Capability::TransferPublish,
        Capability::TransferPropose,
        Capability::TransferNegotiateOwn,
        Capability::TransferReviseOwn,
        Capability::TransferAgreeOwn,
        Capability::TransferActivateOwn,
        Capability::TransferClaimOccurrenceOwn,
        Capability::TransferConfirmOwn,
        Capability::TransferSettleLocal,
        Capability::TransferWithdrawOwn,
        Capability::TransferCancelOwn,
        Capability::TransferDisputeOwn,
        Capability::TransferCorrectOwn,
        Capability::TransferDeclassify,
    ]
}
