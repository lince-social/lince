use std::collections::{BTreeMap, BTreeSet};

use nucleus::karma::{
    CandidateRoute, CapabilitySet, ControlState, DurationMs, EvaluationErrorCode, EvaluationLimits,
    EvaluationReplayCapsuleSchema, EvaluatorRevision, FrozenEvaluationContext, InputBinding,
    InputSource, LateEventPolicy, LiteralValue, LocalId, NodeAst, NodeOperation, OutputRef,
    PortContract, ProgramAst, ProgramSchema, ReferenceKind, ReplayErrorCode, ResolvedReference,
    SealedEvaluationReplayCapsule, Sensitivity, SimulationStatePolicy, Slug, StateContract,
    StateMigrationPolicy, StatePersistence, StateResetPolicy, TimestampMs, TypedUid, ValueType,
    canonical_hash, capture_evaluation_replay,
};
use serde::Serialize;

#[test]
fn a_sealed_capsule_round_trips_and_replays_byte_identically() {
    let sealed = fixture_capsule();
    let first = sealed.verify_and_replay().unwrap();
    let second = sealed.verify_and_replay().unwrap();
    assert_eq!(first, second);
    assert_eq!(first, sealed.capsule.expected_result);
    assert_eq!(
        sealed.capsule_hash.as_str(),
        "sha256:cfc6cd1b872dce9b958439b8d71e6a5b39f0c2d914ccacac81e0d52c02e05764"
    );

    let bytes = serde_json::to_vec(&sealed).unwrap();
    let decoded: SealedEvaluationReplayCapsule = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(decoded, sealed);
    assert_eq!(decoded.verify_and_replay().unwrap(), first);
}

#[test]
fn mutation_fails_at_the_precise_replay_boundary_even_after_resealing() {
    let sealed = fixture_capsule();

    let mut outer_tamper = sealed.clone();
    outer_tamper.capsule.context.logical_at = Some(timestamp(251));
    assert_eq!(
        outer_tamper.verify_and_replay().unwrap_err().code,
        ReplayErrorCode::CapsuleHashMismatch
    );

    let mut changed_context = sealed.capsule.clone();
    changed_context.context.logical_at = Some(timestamp(251));
    let changed_context = changed_context.seal().unwrap();
    assert_eq!(
        changed_context.verify_and_replay().unwrap_err().code,
        ReplayErrorCode::ResultMismatch
    );

    let mut changed_program = sealed.capsule.clone();
    changed_program.program.purpose = "Substituted Program".to_string();
    let changed_program = changed_program.seal().unwrap();
    assert_eq!(
        changed_program.verify_and_replay().unwrap_err().code,
        ReplayErrorCode::ProgramRevisionMismatch
    );

    let mut changed_expected = sealed.capsule.clone();
    changed_expected.expected_result.fuel_used += 1;
    let changed_expected = changed_expected.seal().unwrap();
    assert_eq!(
        changed_expected.verify_and_replay().unwrap_err().code,
        ReplayErrorCode::ExpectedResultHashMismatch
    );
}

#[test]
fn replay_preserves_the_original_typed_evaluation_failure() {
    let sealed = fixture_capsule();
    let mut invalid_context = sealed.capsule.clone();
    invalid_context
        .context
        .control_state
        .insert(id("debounce"), ControlState::Threshold { active: false });
    let invalid_context = invalid_context.seal().unwrap();
    let error = invalid_context.verify_and_replay().unwrap_err();
    assert_eq!(error.code, ReplayErrorCode::EvaluationFailed);
    assert_eq!(
        error.evaluation.as_ref().map(|error| error.code),
        Some(EvaluationErrorCode::RuntimeTypeMismatch)
    );
}

#[test]
fn replay_wire_vocabulary_has_a_golden_hash() {
    let fixture = ReplayVocabulary {
        schemas: vec![EvaluationReplayCapsuleSchema::V1],
        evaluators: vec![EvaluatorRevision::V1],
        errors: vec![
            ReplayErrorCode::EvaluationFailed,
            ReplayErrorCode::CanonicalizationFailed,
            ReplayErrorCode::CapsuleHashMismatch,
            ReplayErrorCode::ProgramRevisionMismatch,
            ReplayErrorCode::ExpectedResultHashMismatch,
            ReplayErrorCode::ResultMismatch,
        ],
    };
    assert_eq!(
        canonical_hash("karma.evaluation-replay-vocabulary.v1", &fixture)
            .unwrap()
            .as_str(),
        "sha256:aa4500b9881e56a7c6055d7bc02cf558a1ef7fa7effea2e7cef75a870267626f"
    );
}

#[derive(Serialize)]
struct ReplayVocabulary {
    schemas: Vec<EvaluationReplayCapsuleSchema>,
    evaluators: Vec<EvaluatorRevision>,
    errors: Vec<ReplayErrorCode>,
}

fn fixture_capsule() -> SealedEvaluationReplayCapsule {
    let program = replay_program();
    let context = FrozenEvaluationContext {
        boundary_values: BTreeMap::from([(id("source"), boolean(true))]),
        logical_at: Some(timestamp(250)),
        control_state: BTreeMap::from([(
            id("debounce"),
            ControlState::Debounce {
                stable: false,
                pending: Some(true),
                pending_since: Some(timestamp(0)),
                last_observed_at: Some(timestamp(0)),
            },
        )]),
        ..FrozenEvaluationContext::default()
    };
    capture_evaluation_replay(&program, &context, EvaluationLimits::default()).unwrap()
}

fn replay_program() -> ProgramAst {
    let candidate_type = ValueType::Candidate {
        route: CandidateRoute::Recommend,
        template: Slug::new("notify.ready").unwrap(),
        fields: BTreeMap::from([(id("stable"), ValueType::Bool)]),
    };
    ProgramAst {
        schema: ProgramSchema::V1,
        slug: Slug::new("replay.control-candidate").unwrap(),
        purpose: "Replay one temporal transition and inert candidate".to_string(),
        tags: BTreeSet::new(),
        parameters: BTreeMap::new(),
        nodes: BTreeMap::from([
            (
                id("source"),
                NodeAst {
                    inputs: BTreeMap::new(),
                    outputs: BTreeMap::from([(id("value"), port(ValueType::Bool))]),
                    operation: NodeOperation::Input {
                        source: InputSource::Signal {
                            signal: reference(ReferenceKind::Signal, RECORD_UID, "kitchen.scale"),
                        },
                        output: id("value"),
                    },
                },
            ),
            (
                id("debounce"),
                NodeAst {
                    inputs: BTreeMap::from([(
                        id("input"),
                        binding("source", "value", ValueType::Bool),
                    )]),
                    outputs: BTreeMap::from([
                        (id("stable"), port(ValueType::Bool)),
                        (id("entered"), port(ValueType::Bool)),
                        (id("left"), port(ValueType::Bool)),
                    ]),
                    operation: NodeOperation::Debounce {
                        input: id("input"),
                        stable: id("stable"),
                        entered: id("entered"),
                        left: id("left"),
                        for_at_least: DurationMs::new(250),
                        initial: false,
                        state: state(),
                    },
                },
            ),
            (
                id("candidate"),
                NodeAst {
                    inputs: BTreeMap::from([
                        (
                            id("condition"),
                            binding("debounce", "entered", ValueType::Bool),
                        ),
                        (id("stable"), binding("debounce", "stable", ValueType::Bool)),
                    ]),
                    outputs: BTreeMap::from([(
                        id("proposal"),
                        port(ValueType::Datum {
                            value: Box::new(candidate_type),
                        }),
                    )]),
                    operation: NodeOperation::RouteCandidate {
                        condition: id("condition"),
                        output: id("proposal"),
                        route: CandidateRoute::Recommend,
                        template: Slug::new("notify.ready").unwrap(),
                        fields: BTreeMap::from([(id("stable"), id("stable"))]),
                    },
                },
            ),
        ]),
        outputs: BTreeMap::from([
            (id("active"), output("debounce", "stable")),
            (id("proposal"), output("candidate", "proposal")),
        ]),
        required_capabilities: CapabilitySet::default(),
    }
}

fn state() -> StateContract {
    StateContract {
        persistence: StatePersistence::Program,
        reset: StateResetPolicy::OnRevisionChange,
        late_event: LateEventPolicy::Reject,
        migration: StateMigrationPolicy::RequireExplicit,
        simulation: SimulationStatePolicy::Clone,
    }
}

fn binding(node: &str, port: &str, value_type: ValueType) -> InputBinding {
    InputBinding {
        source: output(node, port),
        expected_type: value_type,
    }
}

fn port(value_type: ValueType) -> PortContract {
    PortContract {
        value_type,
        sensitivity: Sensitivity::Secret,
        freshness: None,
    }
}

fn output(node: &str, port: &str) -> OutputRef {
    OutputRef {
        node: id(node),
        port: id(port),
    }
}

fn id(value: &str) -> LocalId {
    LocalId::new(value).unwrap()
}

fn boolean(value: bool) -> LiteralValue {
    LiteralValue::Bool { value }
}

fn timestamp(offset: i64) -> TimestampMs {
    TimestampMs::parse_canonical("2026-07-22T00:00:00.000Z")
        .unwrap()
        .checked_add(DurationMs::new(offset))
        .unwrap()
}

const RECORD_UID: &str = "r_01ARZ3NDEKTSV4RRFFQ69G5FAV";

fn reference(kind: ReferenceKind, uid_value: &str, slug: &str) -> ResolvedReference {
    ResolvedReference {
        target: TypedUid::new(kind, uid_value).unwrap(),
        display_slug: Some(Slug::new(slug).unwrap()),
    }
}
