use std::collections::{BTreeMap, BTreeSet};

use nucleus::karma::{
    BinaryOperator, CapabilitySet, ControlState, EvaluationErrorCode, EvaluationLimits,
    ExpressionAst, FrozenEvaluationContext, InputBinding, InputSource, LateEventPolicy,
    LiteralValue, LocalId, NodeAst, NodeOperation, OutputRef, ParameterDefinition, PortContract,
    ProgramAst, ProgramSchema, Sensitivity, SimulationStatePolicy, Slug, StateContract,
    StateMigrationPolicy, StatePersistence, StateResetPolicy, TimestampMs, UnaryOperator,
    ValueType, canonical_hash, evaluate_program,
};
use serde::Serialize;

#[test]
fn same_frozen_context_produces_byte_identical_result_and_parameter_override_is_explicit() {
    let program = arithmetic_program(BinaryOperator::Add);
    let context = FrozenEvaluationContext {
        boundary_values: BTreeMap::from([(id("input"), int(40))]),
        ..FrozenEvaluationContext::default()
    };
    let first = evaluate_program(&program, &context, EvaluationLimits::default()).unwrap();
    let second = evaluate_program(&program, &context, EvaluationLimits::default()).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.outputs.get(&id("answer")), Some(&int(42)));
    assert_eq!(first.fuel_used, 6);
    assert_eq!(
        canonical_hash("karma.evaluation-result.v1", &first)
            .unwrap()
            .as_str(),
        "sha256:8e832acf128cb89ff22e0ef287bf6392acc261ace11fb763b7933249f5137968"
    );

    let overridden = FrozenEvaluationContext {
        boundary_values: BTreeMap::from([(id("input"), int(40))]),
        parameter_values: BTreeMap::from([(id("increment"), int(3))]),
        delay_state: BTreeMap::new(),
        ..FrozenEvaluationContext::default()
    };
    let result = evaluate_program(&program, &overridden, EvaluationLimits::default()).unwrap();
    assert_eq!(result.outputs.get(&id("answer")), Some(&int(43)));
}

#[test]
fn missing_and_wrong_typed_boundary_values_fail_without_partial_success() {
    let program = arithmetic_program(BinaryOperator::Add);
    let missing = evaluate_program(
        &program,
        &FrozenEvaluationContext::default(),
        EvaluationLimits::default(),
    )
    .unwrap_err();
    assert_eq!(missing.code, EvaluationErrorCode::MissingInput);
    assert_eq!(missing.node, Some(id("input")));

    let wrong = FrozenEvaluationContext {
        boundary_values: BTreeMap::from([(id("input"), LiteralValue::Bool { value: true })]),
        ..FrozenEvaluationContext::default()
    };
    let error = evaluate_program(&program, &wrong, EvaluationLimits::default()).unwrap_err();
    assert_eq!(error.code, EvaluationErrorCode::RuntimeTypeMismatch);
}

#[test]
fn checked_arithmetic_reports_overflow_and_division_by_zero() {
    let program = arithmetic_program(BinaryOperator::Add);
    let overflow = FrozenEvaluationContext {
        boundary_values: BTreeMap::from([(id("input"), int(i64::MAX))]),
        ..FrozenEvaluationContext::default()
    };
    let error = evaluate_program(&program, &overflow, EvaluationLimits::default()).unwrap_err();
    assert_eq!(error.code, EvaluationErrorCode::ArithmeticOverflow);

    let divide = literal_binary_program(BinaryOperator::Divide, 1, 0);
    let error = evaluate_program(
        &divide,
        &FrozenEvaluationContext::default(),
        EvaluationLimits::default(),
    )
    .unwrap_err();
    assert_eq!(error.code, EvaluationErrorCode::DivisionByZero);
}

#[test]
fn if_is_lazy_but_both_branches_remain_statically_typed() {
    let expression = ExpressionAst::If {
        condition: Box::new(ExpressionAst::Literal {
            value: LiteralValue::Bool { value: true },
        }),
        then_value: Box::new(literal(7)),
        else_value: Box::new(ExpressionAst::Binary {
            operator: BinaryOperator::Divide,
            left: Box::new(literal(1)),
            right: Box::new(literal(0)),
            precision: None,
        }),
    };
    let program = expression_program("lazy", expression);
    let result = evaluate_program(
        &program,
        &FrozenEvaluationContext::default(),
        EvaluationLimits::default(),
    )
    .unwrap();
    assert_eq!(result.outputs.get(&id("answer")), Some(&int(7)));
}

#[test]
fn fuel_and_depth_are_deterministic_work_limits() {
    let program = arithmetic_program(BinaryOperator::Add);
    let context = FrozenEvaluationContext {
        boundary_values: BTreeMap::from([(id("input"), int(40))]),
        ..FrozenEvaluationContext::default()
    };
    let error = evaluate_program(
        &program,
        &context,
        EvaluationLimits {
            fuel: 5,
            max_expression_depth: 128,
        },
    )
    .unwrap_err();
    assert_eq!(error.code, EvaluationErrorCode::FuelExhausted);
    assert_eq!(error.fuel_used, 5);

    let nested = expression_program(
        "deep",
        ExpressionAst::Unary {
            operator: UnaryOperator::Negate,
            value: Box::new(literal(1)),
        },
    );
    let error = evaluate_program(
        &nested,
        &FrozenEvaluationContext::default(),
        EvaluationLimits {
            fuel: 100,
            max_expression_depth: 1,
        },
    )
    .unwrap_err();
    assert_eq!(error.code, EvaluationErrorCode::ExpressionDepthExceeded);
}

#[test]
fn delay_reads_old_state_and_stages_new_state_for_the_next_epoch() {
    let program = feedback_program();
    let first = evaluate_program(
        &program,
        &FrozenEvaluationContext::default(),
        EvaluationLimits::default(),
    )
    .unwrap();
    assert_eq!(first.outputs.get(&id("current")), Some(&int(0)));
    assert_eq!(first.outputs.get(&id("next")), Some(&int(1)));
    assert_eq!(first.state_updates.get(&id("memory")), Some(&int(1)));
    let memory_trace = first
        .trace
        .iter()
        .find(|trace| trace.node == id("memory"))
        .unwrap();
    assert_eq!(memory_trace.state_before, Some(int(0)));
    assert_eq!(memory_trace.staged_state_update, Some(int(1)));

    let second_context = FrozenEvaluationContext {
        delay_state: first.state_updates.clone(),
        ..FrozenEvaluationContext::default()
    };
    let second = evaluate_program(&program, &second_context, EvaluationLimits::default()).unwrap();
    assert_eq!(second.outputs.get(&id("current")), Some(&int(1)));
    assert_eq!(second.outputs.get(&id("next")), Some(&int(2)));
    assert_eq!(second.state_updates.get(&id("memory")), Some(&int(2)));
}

#[test]
fn evaluation_wire_vocabulary_has_a_golden_hash() {
    let fixture = EvaluationVocabularyFixture {
        error_codes: vec![
            EvaluationErrorCode::ProofRejected,
            EvaluationErrorCode::MissingInput,
            EvaluationErrorCode::MissingParameter,
            EvaluationErrorCode::MissingStateSource,
            EvaluationErrorCode::RuntimeTypeMismatch,
            EvaluationErrorCode::ArithmeticOverflow,
            EvaluationErrorCode::DivisionByZero,
            EvaluationErrorCode::FuelExhausted,
            EvaluationErrorCode::ExpressionDepthExceeded,
            EvaluationErrorCode::MissingLogicalTime,
            EvaluationErrorCode::NonMonotonicLogicalTime,
            EvaluationErrorCode::ReplayRequired,
            EvaluationErrorCode::InvariantViolation,
        ],
        control_states: vec![
            ControlState::Threshold { active: true },
            ControlState::Debounce {
                stable: false,
                pending: Some(true),
                pending_since: Some(timestamp(10)),
                last_observed_at: Some(timestamp(10)),
            },
            ControlState::Cooldown {
                last_allowed_at: Some(timestamp(5)),
                last_observed_at: Some(timestamp(10)),
            },
            ControlState::RateLimit {
                accepted_at: vec![timestamp(5), timestamp(10)],
                last_observed_at: Some(timestamp(10)),
            },
        ],
        limits: EvaluationLimits {
            fuel: 1_000,
            max_expression_depth: 32,
        },
        context: FrozenEvaluationContext {
            boundary_values: BTreeMap::from([(id("input"), int(40))]),
            parameter_values: BTreeMap::from([(id("increment"), int(2))]),
            delay_state: BTreeMap::from([(id("memory"), int(1))]),
            ..FrozenEvaluationContext::default()
        },
    };
    assert_eq!(
        canonical_hash("karma.evaluation-vocabulary.v1", &fixture)
            .unwrap()
            .as_str(),
        "sha256:0a0dbbfed7dd1e090d75d1da4038659f839e48acc0422eb293b8e531fa7bb167"
    );
}

#[derive(Serialize)]
struct EvaluationVocabularyFixture {
    error_codes: Vec<EvaluationErrorCode>,
    control_states: Vec<ControlState>,
    limits: EvaluationLimits,
    context: FrozenEvaluationContext,
}

fn timestamp(offset: i64) -> TimestampMs {
    TimestampMs::parse_canonical("2026-07-22T00:00:00.000Z")
        .unwrap()
        .checked_add(nucleus::karma::DurationMs::new(offset))
        .unwrap()
}

fn arithmetic_program(operator: BinaryOperator) -> ProgramAst {
    let mut program = empty_program("arithmetic");
    program.parameters.insert(
        id("increment"),
        ParameterDefinition {
            value_type: ValueType::I64,
            default: int(2),
            mutable: true,
        },
    );
    program.nodes.insert(
        id("input"),
        input_node(InputSource::SecretMetadata {
            secret: id("value"),
        }),
    );
    program.nodes.insert(
        id("increment"),
        NodeAst {
            inputs: BTreeMap::new(),
            outputs: BTreeMap::from([(id("value"), i64_port())]),
            operation: NodeOperation::Input {
                source: InputSource::Parameter {
                    parameter: id("increment"),
                },
                output: id("value"),
            },
        },
    );
    program.nodes.insert(
        id("sum"),
        NodeAst {
            inputs: BTreeMap::from([
                (id("left"), binding("input", "value")),
                (id("right"), binding("increment", "value")),
            ]),
            outputs: BTreeMap::from([(id("value"), i64_port())]),
            operation: NodeOperation::Derive {
                expressions: BTreeMap::from([(
                    id("value"),
                    ExpressionAst::Binary {
                        operator,
                        left: Box::new(ExpressionAst::Input { input: id("left") }),
                        right: Box::new(ExpressionAst::Input { input: id("right") }),
                        precision: None,
                    },
                )]),
            },
        },
    );
    program.outputs.insert(id("answer"), output("sum", "value"));
    program
}

fn literal_binary_program(operator: BinaryOperator, left: i64, right: i64) -> ProgramAst {
    expression_program(
        "binary",
        ExpressionAst::Binary {
            operator,
            left: Box::new(literal(left)),
            right: Box::new(literal(right)),
            precision: None,
        },
    )
}

fn expression_program(slug: &str, expression: ExpressionAst) -> ProgramAst {
    let mut program = empty_program(slug);
    program.nodes.insert(
        id("expression"),
        NodeAst {
            inputs: BTreeMap::new(),
            outputs: BTreeMap::from([(id("value"), i64_port())]),
            operation: NodeOperation::Derive {
                expressions: BTreeMap::from([(id("value"), expression)]),
            },
        },
    );
    program
        .outputs
        .insert(id("answer"), output("expression", "value"));
    program
}

fn feedback_program() -> ProgramAst {
    let mut program = empty_program("feedback");
    program.nodes.insert(
        id("memory"),
        NodeAst {
            inputs: BTreeMap::from([(id("update"), binding("increment", "value"))]),
            outputs: BTreeMap::from([(id("value"), i64_port())]),
            operation: NodeOperation::Delay {
                input: id("update"),
                output: id("value"),
                initial: int(0),
                state: StateContract {
                    persistence: StatePersistence::Program,
                    reset: StateResetPolicy::OnRevisionChange,
                    late_event: LateEventPolicy::Recompute,
                    migration: StateMigrationPolicy::CompatibleTypeOnly,
                    simulation: SimulationStatePolicy::Clone,
                },
            },
        },
    );
    program.nodes.insert(
        id("increment"),
        NodeAst {
            inputs: BTreeMap::from([(id("current"), binding("memory", "value"))]),
            outputs: BTreeMap::from([(id("value"), i64_port())]),
            operation: NodeOperation::Derive {
                expressions: BTreeMap::from([(
                    id("value"),
                    ExpressionAst::Binary {
                        operator: BinaryOperator::Add,
                        left: Box::new(ExpressionAst::Input {
                            input: id("current"),
                        }),
                        right: Box::new(literal(1)),
                        precision: None,
                    },
                )]),
            },
        },
    );
    program
        .outputs
        .insert(id("current"), output("memory", "value"));
    program
        .outputs
        .insert(id("next"), output("increment", "value"));
    program
}

fn empty_program(slug: &str) -> ProgramAst {
    ProgramAst {
        schema: ProgramSchema::V1,
        slug: Slug::new(format!("test.{slug}")).unwrap(),
        purpose: "Evaluate exact deterministic behavior".to_string(),
        tags: BTreeSet::new(),
        parameters: BTreeMap::new(),
        nodes: BTreeMap::new(),
        outputs: BTreeMap::new(),
        required_capabilities: CapabilitySet::default(),
    }
}

fn input_node(source: InputSource) -> NodeAst {
    NodeAst {
        inputs: BTreeMap::new(),
        outputs: BTreeMap::from([(id("value"), i64_port())]),
        operation: NodeOperation::Input {
            source,
            output: id("value"),
        },
    }
}

fn i64_port() -> PortContract {
    PortContract {
        value_type: ValueType::I64,
        sensitivity: Sensitivity::Private,
        freshness: None,
    }
}

fn binding(node: &str, port: &str) -> InputBinding {
    InputBinding {
        source: output(node, port),
        expected_type: ValueType::I64,
    }
}

fn output(node: &str, port: &str) -> OutputRef {
    OutputRef {
        node: id(node),
        port: id(port),
    }
}

fn literal(value: i64) -> ExpressionAst {
    ExpressionAst::Literal { value: int(value) }
}

fn int(value: i64) -> LiteralValue {
    LiteralValue::I64 { value }
}

fn id(value: &str) -> LocalId {
    LocalId::new(value).unwrap()
}
