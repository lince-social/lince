use std::collections::{BTreeMap, BTreeSet};

use nucleus::karma::{
    BinaryOperator, CandidateRoute, CandidateStatus, Capability, CapabilitySet, Confidence,
    DatumState, DecimalValue, DslErrorKind, DurationMs, ExpressionAst, InputBinding,
    InputSource, KarmaDslError, LateEventPolicy, LiteralValue, LocalId, NodeAst, NodeOperation,
    OutputRef, ParameterDefinition, PortContract, Probability, ProgramAst, ProgramSchema,
    ProofIssueCode, ProofSeverity, ProofStatus, ReferenceKind, ResolvedReference, Sensitivity,
    SimulationStatePolicy, Slug, StateContract, StateMigrationPolicy, StatePersistence,
    StateResetPolicy, ThresholdDirection, TimestampMs, TriggerSource, TypedUid, UnaryOperator,
    ValueType, canonical_hash, prove_program,
};
use serde::Serialize;

const RECORD_UID: &str = "r_01ARZ3NDEKTSV4RRFFQ69G5FAV";
const FACT_UID: &str = "f_01ARZ3NDEKTSV4RRFFQ69G5FAV";
const CONCEPT_UID: &str = "c_01ARZ3NDEKTSV4RRFFQ69G5FAV";

#[test]
fn valid_program_has_stable_hash_order_and_serde_round_trip() {
    let program = valid_program();
    let proof = prove_program(&program);
    assert_eq!(proof.status, ProofStatus::Accepted, "{:#?}", proof.issues);
    assert!(proof.issues.is_empty());
    assert_eq!(
        proof.evaluation_order,
        vec![id("stock"), id("threshold"), id("low")]
    );
    assert_eq!(
        proof.revision_hash.as_ref().unwrap().as_str(),
        "sha256:bf176086adb643746c6896fa3ca350e3f3db95b9315e066bddcebd8ee302410d"
    );

    let encoded = serde_json::to_vec(&program).unwrap();
    let decoded: ProgramAst = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(decoded, program);
    assert_eq!(prove_program(&decoded), proof);
}

#[test]
fn proof_rejects_cycles_but_accepts_explicit_delay_feedback() {
    let mut cycle = empty_program("cycle");
    cycle.nodes.insert(
        id("a"),
        passthrough_node(output("b", "value"), Sensitivity::Private),
    );
    cycle.nodes.insert(
        id("b"),
        passthrough_node(output("a", "value"), Sensitivity::Private),
    );
    let proof = prove_program(&cycle);
    assert_eq!(proof.status, ProofStatus::Rejected);
    let issue = proof
        .issues
        .iter()
        .find(|issue| issue.code == ProofIssueCode::CombinationalCycle)
        .expect("cycle issue");
    assert_eq!(issue.related_nodes, BTreeSet::from([id("a"), id("b")]));

    let mut delayed = empty_program("delayed-cycle");
    delayed.nodes.insert(
        id("derive"),
        passthrough_node(output("memory", "value"), Sensitivity::Private),
    );
    delayed.nodes.insert(
        id("memory"),
        delay_node(output("derive", "value"), Sensitivity::Private),
    );
    let proof = prove_program(&delayed);
    assert_eq!(proof.status, ProofStatus::Accepted, "{:#?}", proof.issues);
    assert_eq!(proof.evaluation_order, vec![id("memory"), id("derive")]);
}

#[test]
fn proof_rejects_missing_types_references_and_taint_downgrades() {
    let mut program = empty_program("invalid");
    program.nodes.insert(
        id("secret"),
        input_node(
            InputSource::SecretMetadata {
                secret: id("api_key"),
            },
            ValueType::Text,
            Sensitivity::Secret,
        ),
    );
    program.nodes.insert(
        id("leak"),
        passthrough_text_node(output("secret", "value"), Sensitivity::Public),
    );
    program.nodes.insert(
        id("missing_parameter"),
        input_node(
            InputSource::Parameter {
                parameter: id("not_declared"),
            },
            ValueType::I64,
            Sensitivity::Private,
        ),
    );
    program.nodes.insert(
        id("wrong_view"),
        input_node(
            InputSource::SavedProtein {
                view: reference(ReferenceKind::Program, RECORD_UID, "wrong.kind"),
            },
            ValueType::Text,
            Sensitivity::Private,
        ),
    );
    program.nodes.insert(
        id("bad_freshness"),
        NodeAst {
            inputs: BTreeMap::new(),
            outputs: BTreeMap::from([(
                id("value"),
                PortContract {
                    value_type: ValueType::I64,
                    sensitivity: Sensitivity::Private,
                    freshness: Some(DurationMs::new(-1)),
                },
            )]),
            operation: NodeOperation::Input {
                source: InputSource::SecretMetadata {
                    secret: id("metadata"),
                },
                output: id("value"),
            },
        },
    );

    let proof = prove_program(&program);
    assert_eq!(proof.status, ProofStatus::Rejected);
    let codes = proof
        .issues
        .iter()
        .map(|issue| issue.code)
        .collect::<BTreeSet<_>>();
    assert!(codes.contains(&ProofIssueCode::TaintDowngrade));
    assert!(codes.contains(&ProofIssueCode::MissingParameter));
    assert!(codes.contains(&ProofIssueCode::InvalidReferenceKind));
    assert!(codes.contains(&ProofIssueCode::InvalidType));
}

#[test]
fn expression_inputs_and_exact_types_are_proven_before_evaluation() {
    let mut program = empty_program("expression-errors");
    program.nodes.insert(
        id("source"),
        input_node(
            InputSource::SecretMetadata {
                secret: id("counter"),
            },
            ValueType::I64,
            Sensitivity::Private,
        ),
    );
    program.nodes.insert(
        id("bad"),
        NodeAst {
            inputs: BTreeMap::from([(
                id("declared"),
                InputBinding {
                    source: output("source", "value"),
                    expected_type: ValueType::Bool,
                },
            )]),
            outputs: BTreeMap::from([(id("value"), port(ValueType::I64, Sensitivity::Private))]),
            operation: NodeOperation::Derive {
                expressions: BTreeMap::from([(
                    id("value"),
                    ExpressionAst::Input { input: id("ghost") },
                )]),
            },
        },
    );
    let proof = prove_program(&program);
    let codes = proof
        .issues
        .iter()
        .map(|issue| issue.code)
        .collect::<BTreeSet<_>>();
    assert!(codes.contains(&ProofIssueCode::InputTypeMismatch));
    assert!(codes.contains(&ProofIssueCode::UndeclaredExpressionInput));

    let mut exact = empty_program("exact-expression");
    exact.nodes.insert(
        id("math"),
        NodeAst {
            inputs: BTreeMap::new(),
            outputs: BTreeMap::from([(id("value"), port(ValueType::I64, Sensitivity::Public))]),
            operation: NodeOperation::Derive {
                expressions: BTreeMap::from([(
                    id("value"),
                    ExpressionAst::Binary {
                        operator: BinaryOperator::Add,
                        left: Box::new(int_literal(20)),
                        right: Box::new(int_literal(22)),
                        precision: None,
                    },
                )]),
            },
        },
    );
    assert_eq!(prove_program(&exact).status, ProofStatus::Accepted);
}

#[test]
fn dynamic_decimals_and_typed_missing_data_are_exact() {
    let decimal = DecimalValue::parse_canonical(3, "12.340").unwrap();
    assert_eq!(decimal.scale(), 3);
    assert_eq!(decimal.mantissa(), 12_340);
    assert_eq!(decimal.to_string(), "12.340");
    assert!(DecimalValue::parse_canonical(3, "12.34").is_err());
    assert!(DecimalValue::parse_canonical(3, "-0.000").is_err());

    let missing = LiteralValue::Datum {
        value_type: Box::new(ValueType::I64),
        state: DatumState::Missing,
        value: None,
    };
    assert_eq!(
        missing.value_type().unwrap(),
        ValueType::Datum {
            value: Box::new(ValueType::I64)
        }
    );
    let invalid = LiteralValue::Datum {
        value_type: Box::new(ValueType::I64),
        state: DatumState::Missing,
        value: Some(Box::new(LiteralValue::I64 { value: 0 })),
    };
    assert!(invalid.value_type().is_err());
}

#[test]
fn all_new_k1_wire_variants_have_a_golden_hash() {
    let fixture = k1_vocabulary_fixture();
    let hash = canonical_hash("karma.i1-vocabulary.v1", &fixture).unwrap();
    assert_eq!(
        hash.as_str(),
        "sha256:5b87aaa15cf68b4eee34d5ee144f7737b417fca53495941c7471123b00bdd5ef",
        "adding a K1 wire variant requires a deliberate golden update"
    );
}

#[derive(Serialize)]
struct K1VocabularyFixture {
    binary_operators: Vec<BinaryOperator>,
    candidate_routes: Vec<CandidateRoute>,
    candidate_status_example: CandidateStatus,
    decimals: Vec<DecimalValue>,
    dsl_error: KarmaDslError,
    dsl_error_kinds: Vec<DslErrorKind>,
    input_sources: Vec<InputSource>,
    late_event_policies: Vec<LateEventPolicy>,
    literals: Vec<LiteralValue>,
    node_operations: Vec<NodeOperation>,
    program_schemas: Vec<ProgramSchema>,
    proof_issue_codes: Vec<ProofIssueCode>,
    proof_severities: Vec<ProofSeverity>,
    proof_statuses: Vec<ProofStatus>,
    simulation_state_policies: Vec<SimulationStatePolicy>,
    state_migration_policies: Vec<StateMigrationPolicy>,
    state_persistences: Vec<StatePersistence>,
    state_reset_policies: Vec<StateResetPolicy>,
    sensitivities: Vec<Sensitivity>,
    trigger_sources: Vec<TriggerSource>,
    threshold_directions: Vec<ThresholdDirection>,
    unary_operators: Vec<UnaryOperator>,
    value_types: Vec<ValueType>,
}

fn k1_vocabulary_fixture() -> K1VocabularyFixture {
    let unit = uid(ReferenceKind::Unit, CONCEPT_UID);
    let decimal = DecimalValue::parse_canonical(3, "1.250").unwrap();
    let two_places = DecimalValue::parse_canonical(2, "12.50").unwrap();
    let state = state_contract();
    K1VocabularyFixture {
        binary_operators: vec![
            BinaryOperator::Add,
            BinaryOperator::Subtract,
            BinaryOperator::Multiply,
            BinaryOperator::Divide,
            BinaryOperator::Remainder,
            BinaryOperator::Equal,
            BinaryOperator::NotEqual,
            BinaryOperator::Less,
            BinaryOperator::LessOrEqual,
            BinaryOperator::Greater,
            BinaryOperator::GreaterOrEqual,
            BinaryOperator::And,
            BinaryOperator::Or,
        ],
        candidate_routes: vec![
            CandidateRoute::Observe,
            CandidateRoute::Recommend,
            CandidateRoute::Draft,
            CandidateRoute::Ask,
            CandidateRoute::Act,
        ],
        candidate_status_example: CandidateStatus::Proposed,
        decimals: vec![decimal, two_places],
        dsl_error: KarmaDslError {
            kind: DslErrorKind::UnexpectedToken,
            offset: 12,
            line: 2,
            column: 4,
            message: "expected program".to_string(),
        },
        dsl_error_kinds: vec![
            DslErrorKind::SourceTooLarge,
            DslErrorKind::TooManyTokens,
            DslErrorKind::StringTooLarge,
            DslErrorKind::NestingTooDeep,
            DslErrorKind::UnexpectedCharacter,
            DslErrorKind::UnexpectedToken,
            DslErrorKind::UnexpectedEnd,
            DslErrorKind::InvalidAtom,
            DslErrorKind::DuplicateDeclaration,
            DslErrorKind::MissingDeclaration,
            DslErrorKind::TrailingInput,
        ],
        input_sources: vec![
            InputSource::Parameter {
                parameter: id("threshold"),
            },
            InputSource::RecordQuantity {
                record: reference(ReferenceKind::Record, RECORD_UID, "apple.stock"),
            },
            InputSource::SavedProtein {
                view: reference(ReferenceKind::View, RECORD_UID, "apple.offers"),
            },
            InputSource::Signal {
                signal: reference(ReferenceKind::Signal, RECORD_UID, "kitchen.scale"),
            },
            InputSource::SecretMetadata {
                secret: id("weather_key"),
            },
            InputSource::CapturedFact {
                fact: reference(ReferenceKind::Fact, FACT_UID, "apple.observation"),
            },
        ],
        late_event_policies: vec![
            LateEventPolicy::Ignore,
            LateEventPolicy::Recompute,
            LateEventPolicy::Compensate,
            LateEventPolicy::Reject,
        ],
        literals: vec![
            LiteralValue::Bool { value: true },
            LiteralValue::I64 { value: -2 },
            LiteralValue::Decimal { value: decimal },
            LiteralValue::Probability {
                value: Probability::from_parts_per_billion(720_000_000).unwrap(),
            },
            LiteralValue::Confidence {
                value: Confidence::from_parts_per_billion(650_000_000).unwrap(),
            },
            LiteralValue::Text {
                value: "apple".to_string(),
            },
            LiteralValue::Duration {
                value: DurationMs::new(3),
            },
            LiteralValue::Timestamp {
                value: TimestampMs::parse_canonical("2026-07-21T08:00:00.125Z").unwrap(),
            },
            LiteralValue::Quantity {
                amount: decimal,
                unit: unit.clone(),
            },
            LiteralValue::Reference {
                value: reference(ReferenceKind::Record, RECORD_UID, "apple.stock"),
            },
            datum(DatumState::Value, Some(LiteralValue::I64 { value: 1 })),
            datum(DatumState::Missing, None),
            datum(DatumState::Stale, None),
            datum(DatumState::Denied, None),
            datum(DatumState::Invalid, None),
            LiteralValue::Candidate {
                route: CandidateRoute::Recommend,
                template: Slug::new("buy.apple").unwrap(),
                fields: BTreeMap::from([(id("amount"), LiteralValue::I64 { value: 2 })]),
            },
        ],
        node_operations: vec![
            NodeOperation::Trigger {
                source: TriggerSource::Manual,
                output: id("event"),
            },
            NodeOperation::Input {
                source: InputSource::Parameter {
                    parameter: id("threshold"),
                },
                output: id("value"),
            },
            NodeOperation::Derive {
                expressions: BTreeMap::from([(id("value"), int_literal(1))]),
            },
            NodeOperation::Delay {
                input: id("next"),
                output: id("previous"),
                initial: LiteralValue::I64 { value: 0 },
                state: state.clone(),
            },
            NodeOperation::Threshold {
                input: id("value"),
                active: id("active"),
                entered: id("entered"),
                left: id("left"),
                direction: ThresholdDirection::Above,
                enter: LiteralValue::I64 { value: 10 },
                exit: LiteralValue::I64 { value: 8 },
                initial_active: false,
                state: state.clone(),
            },
            NodeOperation::Debounce {
                input: id("input"),
                stable: id("stable"),
                entered: id("entered"),
                left: id("left"),
                for_at_least: DurationMs::new(250),
                initial: false,
                state: state.clone(),
            },
            NodeOperation::Cooldown {
                input: id("input"),
                allowed: id("allowed"),
                cooldown: DurationMs::new(5_000),
                state: state.clone(),
            },
            NodeOperation::RateLimit {
                input: id("input"),
                allowed: id("allowed"),
                max: 3,
                window: DurationMs::new(60_000),
                state,
            },
            NodeOperation::RouteCandidate {
                condition: id("condition"),
                output: id("proposal"),
                route: CandidateRoute::Recommend,
                template: Slug::new("buy.apple").unwrap(),
                fields: BTreeMap::from([(id("amount"), id("amount"))]),
            },
        ],
        program_schemas: vec![ProgramSchema::V1],
        proof_issue_codes: vec![
            ProofIssueCode::CanonicalizationFailed,
            ProofIssueCode::EmptyPurpose,
            ProofIssueCode::InvalidType,
            ProofIssueCode::InvalidLiteral,
            ProofIssueCode::DefaultTypeMismatch,
            ProofIssueCode::MissingParameter,
            ProofIssueCode::MissingNode,
            ProofIssueCode::MissingPort,
            ProofIssueCode::InputTypeMismatch,
            ProofIssueCode::UndeclaredExpressionInput,
            ProofIssueCode::ExpressionTypeMismatch,
            ProofIssueCode::InvalidOperationShape,
            ProofIssueCode::InvalidReferenceKind,
            ProofIssueCode::TaintDowngrade,
            ProofIssueCode::CombinationalCycle,
        ],
        proof_severities: vec![ProofSeverity::Error, ProofSeverity::Warning],
        proof_statuses: vec![ProofStatus::Accepted, ProofStatus::Rejected],
        simulation_state_policies: vec![SimulationStatePolicy::Clone, SimulationStatePolicy::Reset],
        state_migration_policies: vec![
            StateMigrationPolicy::Reset,
            StateMigrationPolicy::RequireExplicit,
            StateMigrationPolicy::CompatibleTypeOnly,
        ],
        state_persistences: vec![
            StatePersistence::Program,
            StatePersistence::Workflow,
            StatePersistence::ModelCheckpoint,
        ],
        state_reset_policies: vec![
            StateResetPolicy::Never,
            StateResetPolicy::Manual,
            StateResetPolicy::OnRevisionChange,
            StateResetPolicy::OnProgramActivation,
        ],
        sensitivities: vec![
            Sensitivity::Public,
            Sensitivity::Shared,
            Sensitivity::Private,
            Sensitivity::Secret,
        ],
        trigger_sources: vec![
            TriggerSource::Manual,
            TriggerSource::Fact {
                record: Some(reference(ReferenceKind::Record, RECORD_UID, "apple.stock")),
                concept: Some(reference(ReferenceKind::Concept, CONCEPT_UID, "apple")),
            },
            TriggerSource::Frequency {
                frequency: reference(ReferenceKind::Frequency, RECORD_UID, "hourly"),
            },
            TriggerSource::Signal {
                signal: reference(ReferenceKind::Signal, RECORD_UID, "kitchen.scale"),
            },
            TriggerSource::Decision {
                decision: reference(ReferenceKind::Decision, RECORD_UID, "apple.buy"),
            },
            TriggerSource::Receipt {
                receipt: reference(ReferenceKind::Receipt, RECORD_UID, "apple.delivery"),
            },
            TriggerSource::Sync,
        ],
        threshold_directions: vec![ThresholdDirection::Above, ThresholdDirection::Below],
        unary_operators: vec![UnaryOperator::Not, UnaryOperator::Negate],
        value_types: vec![
            ValueType::Bool,
            ValueType::I64,
            ValueType::Decimal { scale: 3 },
            ValueType::Probability,
            ValueType::Confidence,
            ValueType::Text,
            ValueType::Duration,
            ValueType::Timestamp,
            ValueType::Quantity {
                scale: 3,
                unit: unit.clone(),
            },
            ValueType::Reference {
                target: ReferenceKind::Record,
            },
            ValueType::List {
                item: Box::new(ValueType::I64),
            },
            ValueType::Set {
                item: Box::new(ValueType::Text),
            },
            ValueType::Map {
                key: Box::new(ValueType::Text),
                value: Box::new(ValueType::I64),
            },
            ValueType::Datum {
                value: Box::new(ValueType::I64),
            },
            ValueType::Estimate {
                value: Box::new(ValueType::Quantity { scale: 3, unit }),
            },
            ValueType::Candidate {
                route: CandidateRoute::Recommend,
                template: Slug::new("buy.apple").unwrap(),
                fields: BTreeMap::from([(id("amount"), ValueType::I64)]),
            },
        ],
    }
}

fn valid_program() -> ProgramAst {
    let unit = uid(ReferenceKind::Unit, CONCEPT_UID);
    let quantity = ValueType::Quantity {
        scale: 3,
        unit: unit.clone(),
    };
    let mut program = empty_program("apple-restock");
    program.parameters.insert(
        id("restock_level"),
        ParameterDefinition {
            value_type: quantity.clone(),
            default: LiteralValue::Quantity {
                amount: DecimalValue::parse_canonical(3, "1.000").unwrap(),
                unit,
            },
            mutable: true,
        },
    );
    program.nodes.insert(
        id("low"),
        NodeAst {
            inputs: BTreeMap::from([
                (
                    id("current"),
                    InputBinding {
                        source: output("stock", "value"),
                        expected_type: quantity.clone(),
                    },
                ),
                (
                    id("limit"),
                    InputBinding {
                        source: output("threshold", "value"),
                        expected_type: quantity.clone(),
                    },
                ),
            ]),
            outputs: BTreeMap::from([(id("value"), port(ValueType::Bool, Sensitivity::Private))]),
            operation: NodeOperation::Derive {
                expressions: BTreeMap::from([(
                    id("value"),
                    ExpressionAst::Binary {
                        operator: BinaryOperator::Less,
                        left: Box::new(ExpressionAst::Input {
                            input: id("current"),
                        }),
                        right: Box::new(ExpressionAst::Input { input: id("limit") }),
                        precision: None,
                    },
                )]),
            },
        },
    );
    program.nodes.insert(
        id("threshold"),
        input_node(
            InputSource::Parameter {
                parameter: id("restock_level"),
            },
            quantity.clone(),
            Sensitivity::Private,
        ),
    );
    program.nodes.insert(
        id("stock"),
        input_node(
            InputSource::RecordQuantity {
                record: reference(ReferenceKind::Record, RECORD_UID, "apple.stock"),
            },
            quantity,
            Sensitivity::Private,
        ),
    );
    program
        .outputs
        .insert(id("stock_low"), output("low", "value"));
    program.required_capabilities = CapabilitySet::new([Capability::KarmaRead]);
    program
}

fn empty_program(slug: &str) -> ProgramAst {
    ProgramAst {
        schema: ProgramSchema::V1,
        slug: Slug::new(format!("test.{slug}")).unwrap(),
        purpose: "Exercise deterministic Karma semantics".to_string(),
        tags: BTreeSet::new(),
        parameters: BTreeMap::new(),
        nodes: BTreeMap::new(),
        outputs: BTreeMap::new(),
        required_capabilities: CapabilitySet::default(),
    }
}

fn input_node(source: InputSource, value_type: ValueType, sensitivity: Sensitivity) -> NodeAst {
    NodeAst {
        inputs: BTreeMap::new(),
        outputs: BTreeMap::from([(id("value"), port(value_type, sensitivity))]),
        operation: NodeOperation::Input {
            source,
            output: id("value"),
        },
    }
}

fn passthrough_node(source: OutputRef, sensitivity: Sensitivity) -> NodeAst {
    NodeAst {
        inputs: BTreeMap::from([(
            id("input"),
            InputBinding {
                source,
                expected_type: ValueType::I64,
            },
        )]),
        outputs: BTreeMap::from([(id("value"), port(ValueType::I64, sensitivity))]),
        operation: NodeOperation::Derive {
            expressions: BTreeMap::from([(
                id("value"),
                ExpressionAst::Input { input: id("input") },
            )]),
        },
    }
}

fn passthrough_text_node(source: OutputRef, sensitivity: Sensitivity) -> NodeAst {
    NodeAst {
        inputs: BTreeMap::from([(
            id("input"),
            InputBinding {
                source,
                expected_type: ValueType::Text,
            },
        )]),
        outputs: BTreeMap::from([(id("value"), port(ValueType::Text, sensitivity))]),
        operation: NodeOperation::Derive {
            expressions: BTreeMap::from([(
                id("value"),
                ExpressionAst::Input { input: id("input") },
            )]),
        },
    }
}

fn delay_node(source: OutputRef, sensitivity: Sensitivity) -> NodeAst {
    NodeAst {
        inputs: BTreeMap::from([(
            id("next"),
            InputBinding {
                source,
                expected_type: ValueType::I64,
            },
        )]),
        outputs: BTreeMap::from([(id("value"), port(ValueType::I64, sensitivity))]),
        operation: NodeOperation::Delay {
            input: id("next"),
            output: id("value"),
            initial: LiteralValue::I64 { value: 0 },
            state: state_contract(),
        },
    }
}

fn state_contract() -> StateContract {
    StateContract {
        persistence: StatePersistence::Program,
        reset: StateResetPolicy::OnRevisionChange,
        late_event: LateEventPolicy::Recompute,
        migration: StateMigrationPolicy::CompatibleTypeOnly,
        simulation: SimulationStatePolicy::Clone,
    }
}

fn int_literal(value: i64) -> ExpressionAst {
    ExpressionAst::Literal {
        value: LiteralValue::I64 { value },
    }
}

fn datum(state: DatumState, value: Option<LiteralValue>) -> LiteralValue {
    LiteralValue::Datum {
        value_type: Box::new(ValueType::I64),
        state,
        value: value.map(Box::new),
    }
}

fn port(value_type: ValueType, sensitivity: Sensitivity) -> PortContract {
    PortContract {
        value_type,
        sensitivity,
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

fn uid(kind: ReferenceKind, value: &str) -> TypedUid {
    TypedUid::new(kind, value).unwrap()
}

fn reference(kind: ReferenceKind, uid_value: &str, slug: &str) -> ResolvedReference {
    ResolvedReference {
        target: uid(kind, uid_value),
        display_slug: Some(Slug::new(slug).unwrap()),
    }
}
