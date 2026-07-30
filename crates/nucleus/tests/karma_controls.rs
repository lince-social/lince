use std::collections::{BTreeMap, BTreeSet};

use nucleus::karma::{
    CandidateRoute, CapabilitySet, ControlState, DatumState, DurationMs, EvaluationErrorCode,
    EvaluationLimits, EvaluationResult, ExpressionAst, FrozenEvaluationContext, InputBinding,
    InputSource, LateEventPolicy, LiteralValue, LocalId, NodeAst, NodeOperation, OutputRef,
    PortContract, ProgramAst, ProgramSchema, ProofIssueCode, ProofStatus, Sensitivity,
    SimulationStatePolicy, Slug, StateContract, StateMigrationPolicy, StatePersistence,
    StateResetPolicy, ThresholdDirection, TimestampMs, ValueType, evaluate_program, format_program,
    parse_program, prove_program,
};

#[test]
fn above_threshold_hysteresis_emits_single_enter_and_leave_pulses() {
    let program = threshold_program(ThresholdDirection::Above, 10, 8);
    let mut state = BTreeMap::new();
    let expected = [
        (9, false, false, false),
        (10, true, true, false),
        (9, true, false, false),
        (8, false, false, true),
        (9, false, false, false),
        (10, true, true, false),
    ];
    for (value, active, entered, left) in expected {
        let result = evaluate_scalar(&program, value, None, state);
        assert_bools(&result, active, entered, left);
        if entered {
            let trace = result
                .trace
                .iter()
                .find(|trace| trace.node == id("control"))
                .unwrap();
            assert_eq!(
                trace.control_state_before,
                Some(ControlState::Threshold { active: false })
            );
            assert_eq!(
                trace.staged_control_state_update,
                Some(ControlState::Threshold { active: true })
            );
        }
        state = result.control_state_updates;
    }
}

#[test]
fn below_threshold_and_invalid_bands_are_proven_exactly() {
    let program = threshold_program(ThresholdDirection::Below, 3, 5);
    let first = evaluate_scalar(&program, 3, None, BTreeMap::new());
    assert_bools(&first, true, true, false);
    let second = evaluate_scalar(&program, 5, None, first.control_state_updates);
    assert_bools(&second, false, false, true);

    let invalid_above = threshold_program(ThresholdDirection::Above, 8, 10);
    let proof = prove_program(&invalid_above);
    assert_eq!(proof.status, ProofStatus::Rejected);
    assert!(
        proof
            .issues
            .iter()
            .any(|issue| issue.code == ProofIssueCode::InvalidOperationShape)
    );

    let invalid_below = threshold_program(ThresholdDirection::Below, 5, 3);
    assert_eq!(prove_program(&invalid_below).status, ProofStatus::Rejected);
}

#[test]
fn debounce_requires_continuity_and_promotes_on_the_exact_boundary() {
    let program = debounce_program(250, LateEventPolicy::Reject);
    let mut state = BTreeMap::new();
    for (offset, observed, stable, entered, left) in [
        (0, true, false, false, false),
        (249, true, false, false, false),
        (250, true, true, true, false),
        (251, true, true, false, false),
        (252, false, true, false, false),
        (501, false, true, false, false),
        (502, false, false, false, true),
    ] {
        let result = evaluate_bool(&program, observed, Some(timestamp(offset)), state);
        assert_bools(&result, stable, entered, left);
        state = result.control_state_updates;
    }

    let immediate = debounce_program(0, LateEventPolicy::Reject);
    let result = evaluate_bool(&immediate, true, Some(timestamp(0)), BTreeMap::new());
    assert_bools(&result, true, true, false);
}

#[test]
fn cooldown_and_rate_limit_use_independent_exact_windows() {
    let cooldown = cooldown_program(5_000, LateEventPolicy::Reject);
    let mut state = BTreeMap::new();
    for (offset, fired, allowed) in [
        (0, true, true),
        (4_999, true, false),
        (5_000, false, false),
        (5_000, true, true),
    ] {
        let result = evaluate_bool(&cooldown, fired, Some(timestamp(offset)), state);
        assert_eq!(result.outputs.get(&id("allowed")), Some(&boolean(allowed)));
        state = result.control_state_updates;
    }

    let rate = rate_limit_program(2, 1_000, LateEventPolicy::Reject);
    let mut state = BTreeMap::new();
    for (offset, allowed) in [(0, true), (1, true), (999, false), (1_000, true)] {
        let result = evaluate_bool(&rate, true, Some(timestamp(offset)), state);
        assert_eq!(result.outputs.get(&id("allowed")), Some(&boolean(allowed)));
        state = result.control_state_updates;
    }
    assert_eq!(
        state.get(&id("control")),
        Some(&ControlState::RateLimit {
            accepted_at: vec![timestamp(1), timestamp(1_000)],
            last_observed_at: Some(timestamp(1_000)),
        })
    );
}

#[test]
fn temporal_controls_fail_closed_for_missing_and_late_logical_time() {
    let reject = cooldown_program(10, LateEventPolicy::Reject);
    let missing = evaluate_program(
        &reject,
        &FrozenEvaluationContext {
            boundary_values: BTreeMap::from([(id("source"), boolean(true))]),
            ..FrozenEvaluationContext::default()
        },
        EvaluationLimits::default(),
    )
    .unwrap_err();
    assert_eq!(missing.code, EvaluationErrorCode::MissingLogicalTime);

    let first = evaluate_bool(&reject, true, Some(timestamp(100)), BTreeMap::new());
    let late = evaluate_program(
        &reject,
        &FrozenEvaluationContext {
            boundary_values: BTreeMap::from([(id("source"), boolean(true))]),
            logical_at: Some(timestamp(99)),
            control_state: first.control_state_updates.clone(),
            ..FrozenEvaluationContext::default()
        },
        EvaluationLimits::default(),
    )
    .unwrap_err();
    assert_eq!(late.code, EvaluationErrorCode::NonMonotonicLogicalTime);

    for policy in [LateEventPolicy::Recompute, LateEventPolicy::Compensate] {
        let program = cooldown_program(10, policy);
        let error = evaluate_program(
            &program,
            &FrozenEvaluationContext {
                boundary_values: BTreeMap::from([(id("source"), boolean(true))]),
                logical_at: Some(timestamp(99)),
                control_state: first.control_state_updates.clone(),
                ..FrozenEvaluationContext::default()
            },
            EvaluationLimits::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, EvaluationErrorCode::ReplayRequired);
    }

    let ignore = cooldown_program(10, LateEventPolicy::Ignore);
    let ignored = evaluate_bool(
        &ignore,
        true,
        Some(timestamp(99)),
        first.control_state_updates.clone(),
    );
    assert_eq!(ignored.outputs.get(&id("allowed")), Some(&boolean(false)));
    assert!(ignored.control_state_updates.is_empty());
    let trace = ignored
        .trace
        .iter()
        .find(|trace| trace.node == id("control"))
        .unwrap();
    assert_eq!(
        trace.control_state_before,
        first.control_state_updates.get(&id("control")).cloned()
    );
    assert_eq!(trace.staged_control_state_update, None);
}

#[test]
fn candidate_routes_are_typed_deterministic_and_inert() {
    let program = candidate_program();
    let false_result = evaluate_candidate(&program, false, 4);
    assert_eq!(
        false_result.outputs.get(&id("proposal")),
        Some(&LiteralValue::Datum {
            value_type: Box::new(candidate_type()),
            state: DatumState::Missing,
            value: None,
        })
    );
    assert!(false_result.state_updates.is_empty());
    assert!(false_result.control_state_updates.is_empty());

    let first = evaluate_candidate(&program, true, 4);
    let second = evaluate_candidate(&program, true, 4);
    assert_eq!(first, second);
    assert_eq!(
        first.outputs.get(&id("proposal")),
        Some(&LiteralValue::Datum {
            value_type: Box::new(candidate_type()),
            state: DatumState::Value,
            value: Some(Box::new(LiteralValue::Candidate {
                route: CandidateRoute::Recommend,
                template: Slug::new("buy.apple").unwrap(),
                fields: BTreeMap::from([(id("amount"), int(4))]),
            })),
        })
    );
    assert!(first.state_updates.is_empty());
    assert!(first.control_state_updates.is_empty());
}

#[test]
fn every_k1_6_operation_and_candidate_constructor_round_trips_in_the_dsl() {
    for program in [
        threshold_program(ThresholdDirection::Above, 10, 8),
        debounce_program(250, LateEventPolicy::Reject),
        cooldown_program(5_000, LateEventPolicy::Ignore),
        rate_limit_program(3, 60_000, LateEventPolicy::Compensate),
        candidate_program(),
    ] {
        let text = format_program(&program);
        let parsed = parse_program(&text).unwrap();
        assert_eq!(parsed, program, "{text}");
        assert_eq!(format_program(&parsed), text);
    }

    let candidate_text = format_program(&candidate_program());
    assert!(candidate_text.contains("candidate(recommend, buy.apple, {amount: i64})"));
    assert!(candidate_text.contains(
        "op route-candidate(condition, proposal, recommend, buy.apple, {amount = amount});"
    ));
}

#[test]
fn malformed_temporal_and_candidate_contracts_are_rejected_by_proof() {
    assert_eq!(
        prove_program(&debounce_program(-1, LateEventPolicy::Reject)).status,
        ProofStatus::Rejected
    );
    assert_eq!(
        prove_program(&cooldown_program(-1, LateEventPolicy::Reject)).status,
        ProofStatus::Rejected
    );
    assert_eq!(
        prove_program(&rate_limit_program(0, 10, LateEventPolicy::Reject)).status,
        ProofStatus::Rejected
    );
    assert_eq!(
        prove_program(&rate_limit_program(1, 0, LateEventPolicy::Reject)).status,
        ProofStatus::Rejected
    );

    let mut candidate = candidate_program();
    candidate
        .nodes
        .get_mut(&id("candidate"))
        .unwrap()
        .outputs
        .get_mut(&id("proposal"))
        .unwrap()
        .value_type = ValueType::Datum {
        value: Box::new(ValueType::I64),
    };
    assert_eq!(prove_program(&candidate).status, ProofStatus::Rejected);

    let mut cycle = threshold_program(ThresholdDirection::Above, 10, 8);
    cycle.nodes.insert(
        id("source"),
        NodeAst {
            inputs: BTreeMap::from([(
                id("active"),
                InputBinding {
                    source: output("control", "active"),
                    expected_type: ValueType::Bool,
                },
            )]),
            outputs: BTreeMap::from([(id("value"), port(ValueType::I64))]),
            operation: NodeOperation::Derive {
                expressions: BTreeMap::from([(
                    id("value"),
                    ExpressionAst::If {
                        condition: Box::new(ExpressionAst::Input {
                            input: id("active"),
                        }),
                        then_value: Box::new(ExpressionAst::Literal { value: int(1) }),
                        else_value: Box::new(ExpressionAst::Literal { value: int(2) }),
                    },
                )]),
            },
        },
    );
    let proof = prove_program(&cycle);
    assert!(
        proof
            .issues
            .iter()
            .any(|issue| issue.code == ProofIssueCode::CombinationalCycle)
    );
}

#[test]
fn malformed_persisted_control_state_fails_closed() {
    let debounce = debounce_program(10, LateEventPolicy::Reject);
    let error = evaluate_program(
        &debounce,
        &FrozenEvaluationContext {
            boundary_values: BTreeMap::from([(id("source"), boolean(true))]),
            logical_at: Some(timestamp(11)),
            control_state: BTreeMap::from([(
                id("control"),
                ControlState::Debounce {
                    stable: false,
                    pending: Some(true),
                    pending_since: None,
                    last_observed_at: Some(timestamp(10)),
                },
            )]),
            ..FrozenEvaluationContext::default()
        },
        EvaluationLimits::default(),
    )
    .unwrap_err();
    assert_eq!(error.code, EvaluationErrorCode::RuntimeTypeMismatch);

    let rate = rate_limit_program(2, 1_000, LateEventPolicy::Reject);
    let error = evaluate_program(
        &rate,
        &FrozenEvaluationContext {
            boundary_values: BTreeMap::from([(id("source"), boolean(true))]),
            logical_at: Some(timestamp(11)),
            control_state: BTreeMap::from([(
                id("control"),
                ControlState::RateLimit {
                    accepted_at: vec![timestamp(1), timestamp(2), timestamp(3)],
                    last_observed_at: Some(timestamp(10)),
                },
            )]),
            ..FrozenEvaluationContext::default()
        },
        EvaluationLimits::default(),
    )
    .unwrap_err();
    assert_eq!(error.code, EvaluationErrorCode::RuntimeTypeMismatch);
}

fn evaluate_scalar(
    program: &ProgramAst,
    value: i64,
    logical_at: Option<TimestampMs>,
    control_state: BTreeMap<LocalId, ControlState>,
) -> EvaluationResult {
    evaluate_program(
        program,
        &FrozenEvaluationContext {
            boundary_values: BTreeMap::from([(id("source"), int(value))]),
            logical_at,
            control_state,
            ..FrozenEvaluationContext::default()
        },
        EvaluationLimits::default(),
    )
    .unwrap()
}

fn evaluate_bool(
    program: &ProgramAst,
    value: bool,
    logical_at: Option<TimestampMs>,
    control_state: BTreeMap<LocalId, ControlState>,
) -> EvaluationResult {
    evaluate_program(
        program,
        &FrozenEvaluationContext {
            boundary_values: BTreeMap::from([(id("source"), boolean(value))]),
            logical_at,
            control_state,
            ..FrozenEvaluationContext::default()
        },
        EvaluationLimits::default(),
    )
    .unwrap()
}

fn evaluate_candidate(program: &ProgramAst, condition: bool, amount: i64) -> EvaluationResult {
    evaluate_program(
        program,
        &FrozenEvaluationContext {
            boundary_values: BTreeMap::from([
                (id("condition_source"), boolean(condition)),
                (id("amount_source"), int(amount)),
            ]),
            ..FrozenEvaluationContext::default()
        },
        EvaluationLimits::default(),
    )
    .unwrap()
}

fn assert_bools(result: &EvaluationResult, active: bool, entered: bool, left: bool) {
    assert_eq!(result.outputs.get(&id("active")), Some(&boolean(active)));
    assert_eq!(result.outputs.get(&id("entered")), Some(&boolean(entered)));
    assert_eq!(result.outputs.get(&id("left")), Some(&boolean(left)));
}

fn threshold_program(direction: ThresholdDirection, enter: i64, exit: i64) -> ProgramAst {
    let mut program = base_program("controls.threshold");
    program
        .nodes
        .insert(id("source"), boundary_node(ValueType::I64));
    program.nodes.insert(
        id("control"),
        NodeAst {
            inputs: BTreeMap::from([(id("value"), binding("source", ValueType::I64))]),
            outputs: transition_ports("active"),
            operation: NodeOperation::Threshold {
                input: id("value"),
                active: id("active"),
                entered: id("entered"),
                left: id("left"),
                direction,
                enter: int(enter),
                exit: int(exit),
                initial_active: false,
                state: state(LateEventPolicy::Reject),
            },
        },
    );
    transition_program_outputs(&mut program, "active");
    program
}

fn debounce_program(duration: i64, late_event: LateEventPolicy) -> ProgramAst {
    let mut program = base_program("controls.debounce");
    program
        .nodes
        .insert(id("source"), boundary_node(ValueType::Bool));
    program.nodes.insert(
        id("control"),
        NodeAst {
            inputs: BTreeMap::from([(id("input"), binding("source", ValueType::Bool))]),
            outputs: transition_ports("stable"),
            operation: NodeOperation::Debounce {
                input: id("input"),
                stable: id("stable"),
                entered: id("entered"),
                left: id("left"),
                for_at_least: DurationMs::new(duration),
                initial: false,
                state: state(late_event),
            },
        },
    );
    transition_program_outputs(&mut program, "stable");
    program
}

fn cooldown_program(duration: i64, late_event: LateEventPolicy) -> ProgramAst {
    let mut program = base_program("controls.cooldown");
    program
        .nodes
        .insert(id("source"), boundary_node(ValueType::Bool));
    program.nodes.insert(
        id("control"),
        NodeAst {
            inputs: BTreeMap::from([(id("input"), binding("source", ValueType::Bool))]),
            outputs: BTreeMap::from([(id("allowed"), port(ValueType::Bool))]),
            operation: NodeOperation::Cooldown {
                input: id("input"),
                allowed: id("allowed"),
                cooldown: DurationMs::new(duration),
                state: state(late_event),
            },
        },
    );
    program
        .outputs
        .insert(id("allowed"), output("control", "allowed"));
    program
}

fn rate_limit_program(max: u32, window: i64, late_event: LateEventPolicy) -> ProgramAst {
    let mut program = base_program("controls.rate-limit");
    program
        .nodes
        .insert(id("source"), boundary_node(ValueType::Bool));
    program.nodes.insert(
        id("control"),
        NodeAst {
            inputs: BTreeMap::from([(id("input"), binding("source", ValueType::Bool))]),
            outputs: BTreeMap::from([(id("allowed"), port(ValueType::Bool))]),
            operation: NodeOperation::RateLimit {
                input: id("input"),
                allowed: id("allowed"),
                max,
                window: DurationMs::new(window),
                state: state(late_event),
            },
        },
    );
    program
        .outputs
        .insert(id("allowed"), output("control", "allowed"));
    program
}

fn candidate_program() -> ProgramAst {
    let mut program = base_program("candidates.route");
    program
        .nodes
        .insert(id("condition_source"), boundary_node(ValueType::Bool));
    program
        .nodes
        .insert(id("amount_source"), boundary_node(ValueType::I64));
    program.nodes.insert(
        id("candidate"),
        NodeAst {
            inputs: BTreeMap::from([
                (
                    id("condition"),
                    binding("condition_source", ValueType::Bool),
                ),
                (id("amount"), binding("amount_source", ValueType::I64)),
            ]),
            outputs: BTreeMap::from([(
                id("proposal"),
                port(ValueType::Datum {
                    value: Box::new(candidate_type()),
                }),
            )]),
            operation: NodeOperation::RouteCandidate {
                condition: id("condition"),
                output: id("proposal"),
                route: CandidateRoute::Recommend,
                template: Slug::new("buy.apple").unwrap(),
                fields: BTreeMap::from([(id("amount"), id("amount"))]),
            },
        },
    );
    program
        .outputs
        .insert(id("proposal"), output("candidate", "proposal"));
    program
}

fn candidate_type() -> ValueType {
    ValueType::Candidate {
        route: CandidateRoute::Recommend,
        template: Slug::new("buy.apple").unwrap(),
        fields: BTreeMap::from([(id("amount"), ValueType::I64)]),
    }
}

fn base_program(slug: &str) -> ProgramAst {
    ProgramAst {
        schema: ProgramSchema::V1,
        slug: Slug::new(slug).unwrap(),
        purpose: "Exercise deterministic controls".to_string(),
        tags: BTreeSet::new(),
        parameters: BTreeMap::new(),
        nodes: BTreeMap::new(),
        outputs: BTreeMap::new(),
        required_capabilities: CapabilitySet::default(),
    }
}

fn boundary_node(value_type: ValueType) -> NodeAst {
    NodeAst {
        inputs: BTreeMap::new(),
        outputs: BTreeMap::from([(id("value"), port(value_type))]),
        operation: NodeOperation::Input {
            source: InputSource::SecretMetadata {
                secret: id("fixture"),
            },
            output: id("value"),
        },
    }
}

fn transition_ports(stable: &str) -> BTreeMap<LocalId, PortContract> {
    BTreeMap::from([
        (id(stable), port(ValueType::Bool)),
        (id("entered"), port(ValueType::Bool)),
        (id("left"), port(ValueType::Bool)),
    ])
}

fn transition_program_outputs(program: &mut ProgramAst, stable: &str) {
    program
        .outputs
        .insert(id("active"), output("control", stable));
    program
        .outputs
        .insert(id("entered"), output("control", "entered"));
    program
        .outputs
        .insert(id("left"), output("control", "left"));
}

fn state(late_event: LateEventPolicy) -> StateContract {
    StateContract {
        persistence: StatePersistence::Program,
        reset: StateResetPolicy::OnRevisionChange,
        late_event,
        migration: StateMigrationPolicy::RequireExplicit,
        simulation: SimulationStatePolicy::Clone,
    }
}

fn binding(node: &str, value_type: ValueType) -> InputBinding {
    InputBinding {
        source: output(node, "value"),
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

fn int(value: i64) -> LiteralValue {
    LiteralValue::I64 { value }
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
