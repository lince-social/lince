use std::collections::{BTreeMap, BTreeSet};

use nucleus::karma::{
    BinaryOperator, Capability, CapabilitySet, Confidence, DatumState, DecimalValue,
    DslErrorKind, DurationMs, ExpressionAst, InputBinding, InputSource, LateEventPolicy,
    LiteralValue, LocalId, MAX_DSL_BYTES, MAX_DSL_NESTING, MAX_DSL_STRING_BYTES, MAX_DSL_TOKENS,
    NodeAst, NodeOperation, OutputRef, ParameterDefinition, PortContract, Probability, ProgramAst,
    ProgramSchema, ReferenceKind, ResolvedReference, Sensitivity, SimulationStatePolicy, Slug,
    StateContract, StateMigrationPolicy, StatePersistence, StateResetPolicy, TimestampMs,
    TriggerSource, TypedUid, UnaryOperator, ValueType, format_program, parse_program,
    prove_program,
};

const RECORD_UID: &str = "r_01ARZ3NDEKTSV4RRFFQ69G5FAV";
const FACT_UID: &str = "f_01ARZ3NDEKTSV4RRFFQ69G5FAV";
const CONCEPT_UID: &str = "c_01ARZ3NDEKTSV4RRFFQ69G5FAV";

#[test]
fn canonical_program_text_is_stable_and_proves_after_round_trip() {
    let program = simple_program();
    let formatted = format_program(&program);
    assert_eq!(formatted, SIMPLE_CANONICAL);
    let parsed = parse_program(&formatted).unwrap();
    assert_eq!(parsed, program);
    assert_eq!(format_program(&parsed), formatted);
    assert_eq!(prove_program(&parsed), prove_program(&program));
}

#[test]
fn every_current_program_ast_form_round_trips_losslessly() {
    let program = comprehensive_program();
    let formatted = format_program(&program);
    let parsed = parse_program(&formatted).unwrap();
    assert_eq!(parsed, program);
    assert_eq!(format_program(&parsed), formatted);
    assert!(formatted.contains("op trigger(fact("));
    assert!(formatted.contains("op delay("));
    assert!(formatted.contains("datum(i64, value, i64(7))"));
    assert!(formatted.contains("map(text, estimate(reference(record)))"));
}

#[test]
fn comments_whitespace_and_declaration_order_canonicalize_away() {
    let source = SIMPLE_CANONICAL
        .replace("karma 1;", "# format version\n karma   1 ; # inline")
        .replace(
            "  schema karma.program.v1;\n  purpose \"Round trip\";",
            "  # metadata may be reordered\n  purpose \"Round trip\";\n  schema karma.program.v1;",
        );
    let parsed = parse_program(&source).unwrap();
    assert_eq!(format_program(&parsed), SIMPLE_CANONICAL);

    let unicode = SIMPLE_CANONICAL.replace("\"Round trip\"", "\"Maçã 🍎\"");
    assert_eq!(parse_program(&unicode).unwrap().purpose, "Maçã 🍎");
}

#[test]
fn unknown_duplicate_missing_trailing_and_invalid_atoms_fail_typed() {
    let unknown = SIMPLE_CANONICAL.replace("  tags [];", "  mystery [];\n  tags [];");
    assert_eq!(
        parse_program(&unknown).unwrap_err().kind,
        DslErrorKind::UnexpectedToken
    );

    let duplicate = SIMPLE_CANONICAL.replace(
        "  purpose \"Round trip\";",
        "  purpose \"Round trip\";\n  purpose \"Again\";",
    );
    assert_eq!(
        parse_program(&duplicate).unwrap_err().kind,
        DslErrorKind::DuplicateDeclaration
    );

    let missing = SIMPLE_CANONICAL.replace("  tags [];\n", "");
    assert_eq!(
        parse_program(&missing).unwrap_err().kind,
        DslErrorKind::MissingDeclaration
    );

    let trailing = format!("{SIMPLE_CANONICAL} extra");
    assert_eq!(
        parse_program(&trailing).unwrap_err().kind,
        DslErrorKind::TrailingInput
    );

    let invalid = SIMPLE_CANONICAL.replace("param threshold: i64", "param threshold: mystery");
    assert_eq!(
        parse_program(&invalid).unwrap_err().kind,
        DslErrorKind::UnexpectedToken
    );

    let unicode_token = SIMPLE_CANONICAL.replace("threshold", "limiarç");
    assert_eq!(
        parse_program(&unicode_token).unwrap_err().kind,
        DslErrorKind::UnexpectedCharacter
    );
}

#[test]
fn parser_resource_limits_are_deterministic() {
    let too_large = " ".repeat(MAX_DSL_BYTES + 1);
    assert_eq!(
        parse_program(&too_large).unwrap_err().kind,
        DslErrorKind::SourceTooLarge
    );

    let too_many_tokens = "x ".repeat(MAX_DSL_TOKENS + 1);
    assert_eq!(
        parse_program(&too_many_tokens).unwrap_err().kind,
        DslErrorKind::TooManyTokens
    );

    let huge_purpose = "a".repeat(MAX_DSL_STRING_BYTES + 1);
    let huge_string = SIMPLE_CANONICAL.replace("Round trip", &huge_purpose);
    assert_eq!(
        parse_program(&huge_string).unwrap_err().kind,
        DslErrorKind::StringTooLarge
    );

    let mut nested = ExpressionAst::Input { input: id("value") };
    for _ in 0..=MAX_DSL_NESTING {
        nested = ExpressionAst::Unary {
            operator: UnaryOperator::Negate,
            value: Box::new(nested),
        };
    }
    let mut program = simple_program();
    let derive = program.nodes.get_mut(&id("copy")).unwrap();
    derive.operation = NodeOperation::Derive {
        expressions: BTreeMap::from([(id("value"), nested)]),
    };
    assert_eq!(
        parse_program(&format_program(&program)).unwrap_err().kind,
        DslErrorKind::NestingTooDeep
    );
}

#[test]
fn duplicate_map_ids_and_noncanonical_exact_literals_are_rejected() {
    let duplicate_node = SIMPLE_CANONICAL.replace(
        "  output result = source(copy, value);",
        "  node threshold {\n    port value: i64 sensitivity private freshness none;\n    op input(parameter(threshold), value);\n  }\n  output result = source(copy, value);",
    );
    assert_eq!(
        parse_program(&duplicate_node).unwrap_err().kind,
        DslErrorKind::DuplicateDeclaration
    );

    let bad_probability =
        format_program(&comprehensive_program()).replace("prob(\"0.720000000\")", "prob(\"0.72\")");
    assert_eq!(
        parse_program(&bad_probability).unwrap_err().kind,
        DslErrorKind::InvalidAtom
    );
}

const SIMPLE_CANONICAL: &str = r#"karma 1;
program tests.round-trip {
  schema karma.program.v1;
  purpose "Round trip";
  tags [];
  capabilities [karma.evaluate, karma.read];
  param threshold: i64 mutable = i64(3);
  node copy {
    bind value: i64 = source(threshold, value);
    port value: i64 sensitivity private freshness none;
    op derive {
      expr value = input(value);
    }
  }
  node threshold {
    port value: i64 sensitivity private freshness none;
    op input(parameter(threshold), value);
  }
  output result = source(copy, value);
}
"#;

fn simple_program() -> ProgramAst {
    ProgramAst {
        schema: ProgramSchema::V1,
        slug: Slug::new("tests.round-trip").unwrap(),
        purpose: "Round trip".to_string(),
        tags: BTreeSet::new(),
        parameters: BTreeMap::from([(
            id("threshold"),
            ParameterDefinition {
                value_type: ValueType::I64,
                default: LiteralValue::I64 { value: 3 },
                mutable: true,
            },
        )]),
        nodes: BTreeMap::from([
            (
                id("threshold"),
                NodeAst {
                    inputs: BTreeMap::new(),
                    outputs: BTreeMap::from([(
                        id("value"),
                        port(ValueType::I64, Sensitivity::Private),
                    )]),
                    operation: NodeOperation::Input {
                        source: InputSource::Parameter {
                            parameter: id("threshold"),
                        },
                        output: id("value"),
                    },
                },
            ),
            (
                id("copy"),
                NodeAst {
                    inputs: BTreeMap::from([(
                        id("value"),
                        InputBinding {
                            source: output("threshold", "value"),
                            expected_type: ValueType::I64,
                        },
                    )]),
                    outputs: BTreeMap::from([(
                        id("value"),
                        port(ValueType::I64, Sensitivity::Private),
                    )]),
                    operation: NodeOperation::Derive {
                        expressions: BTreeMap::from([(
                            id("value"),
                            ExpressionAst::Input { input: id("value") },
                        )]),
                    },
                },
            ),
        ]),
        outputs: BTreeMap::from([(id("result"), output("copy", "value"))]),
        required_capabilities: CapabilitySet::new([
            Capability::KarmaRead,
            Capability::KarmaEvaluate,
        ]),
    }
}

fn comprehensive_program() -> ProgramAst {
    let unit = uid(ReferenceKind::Unit, CONCEPT_UID);
    let record = reference(ReferenceKind::Record, RECORD_UID, Some("apple.stock"));
    let fact = reference(ReferenceKind::Fact, FACT_UID, Some("apple.fact"));
    let decimal = DecimalValue::parse_canonical(3, "1.250").unwrap();
    let priced = DecimalValue::parse_canonical(2, "12.50").unwrap();
    let parameters = BTreeMap::from([
        parameter(
            "bool_value",
            ValueType::Bool,
            LiteralValue::Bool { value: true },
        ),
        parameter("i64_value", ValueType::I64, LiteralValue::I64 { value: -2 }),
        parameter(
            "decimal_value",
            ValueType::Decimal { scale: 3 },
            LiteralValue::Decimal { value: decimal },
        ),
        parameter(
            "prob_value",
            ValueType::Probability,
            LiteralValue::Probability {
                value: Probability::from_parts_per_billion(720_000_000).unwrap(),
            },
        ),
        parameter(
            "conf_value",
            ValueType::Confidence,
            LiteralValue::Confidence {
                value: Confidence::from_parts_per_billion(650_000_000).unwrap(),
            },
        ),
        parameter(
            "text_value",
            ValueType::Text,
            LiteralValue::Text {
                value: "maçã\n🍎".to_string(),
            },
        ),
        parameter(
            "duration_value",
            ValueType::Duration,
            LiteralValue::Duration {
                value: DurationMs::new(3),
            },
        ),
        parameter(
            "timestamp_value",
            ValueType::Timestamp,
            LiteralValue::Timestamp {
                value: TimestampMs::parse_canonical("2026-07-21T08:00:00.125Z").unwrap(),
            },
        ),
        parameter(
            "quantity_value",
            ValueType::Quantity {
                scale: 3,
                unit: unit.clone(),
            },
            LiteralValue::Quantity {
                amount: decimal,
                unit: unit.clone(),
            },
        ),
        parameter(
            "priced_value",
            ValueType::Quantity {
                scale: 2,
                unit: unit.clone(),
            },
            LiteralValue::Quantity {
                amount: priced,
                unit: unit.clone(),
            },
        ),
        parameter(
            "reference_value",
            ValueType::Reference {
                target: ReferenceKind::Record,
            },
            LiteralValue::Reference {
                value: record.clone(),
            },
        ),
        parameter(
            "datum_value",
            ValueType::Datum {
                value: Box::new(ValueType::I64),
            },
            LiteralValue::Datum {
                value_type: Box::new(ValueType::I64),
                state: DatumState::Value,
                value: Some(Box::new(LiteralValue::I64 { value: 7 })),
            },
        ),
        parameter(
            "datum_missing",
            ValueType::Datum {
                value: Box::new(ValueType::Text),
            },
            LiteralValue::Datum {
                value_type: Box::new(ValueType::Text),
                state: DatumState::Missing,
                value: None,
            },
        ),
    ]);

    let mut nodes = BTreeMap::new();
    for (name, source) in [
        ("manual_trigger", TriggerSource::Manual),
        (
            "fact_trigger",
            TriggerSource::Fact {
                record: Some(record.clone()),
                concept: Some(reference(
                    ReferenceKind::Concept,
                    CONCEPT_UID,
                    Some("apple"),
                )),
            },
        ),
        (
            "frequency_trigger",
            TriggerSource::Frequency {
                frequency: reference(ReferenceKind::Frequency, RECORD_UID, Some("hourly")),
            },
        ),
        (
            "signal_trigger",
            TriggerSource::Signal {
                signal: reference(ReferenceKind::Signal, RECORD_UID, None),
            },
        ),
        (
            "decision_trigger",
            TriggerSource::Decision {
                decision: reference(ReferenceKind::Decision, RECORD_UID, None),
            },
        ),
        (
            "receipt_trigger",
            TriggerSource::Receipt {
                receipt: reference(ReferenceKind::Receipt, RECORD_UID, None),
            },
        ),
        ("sync_trigger", TriggerSource::Sync),
    ] {
        nodes.insert(
            id(name),
            NodeAst {
                inputs: BTreeMap::new(),
                outputs: BTreeMap::from([(
                    id("event"),
                    port(ValueType::Bool, Sensitivity::Public),
                )]),
                operation: NodeOperation::Trigger {
                    source,
                    output: id("event"),
                },
            },
        );
    }

    for (name, source, value_type, sensitivity) in [
        (
            "parameter_input",
            InputSource::Parameter {
                parameter: id("i64_value"),
            },
            ValueType::I64,
            Sensitivity::Private,
        ),
        (
            "record_input",
            InputSource::RecordQuantity {
                record: record.clone(),
            },
            ValueType::Quantity {
                scale: 3,
                unit: unit.clone(),
            },
            Sensitivity::Shared,
        ),
        (
            "protein_input",
            InputSource::SavedProtein {
                view: reference(ReferenceKind::View, RECORD_UID, Some("apple.offers")),
            },
            ValueType::List {
                item: Box::new(ValueType::Reference {
                    target: ReferenceKind::Transfer,
                }),
            },
            Sensitivity::Private,
        ),
        (
            "signal_input",
            InputSource::Signal {
                signal: reference(ReferenceKind::Signal, RECORD_UID, None),
            },
            ValueType::Decimal { scale: 3 },
            Sensitivity::Private,
        ),
        (
            "secret_input",
            InputSource::SecretMetadata {
                secret: id("api_key"),
            },
            ValueType::Text,
            Sensitivity::Secret,
        ),
        (
            "fact_input",
            InputSource::CapturedFact { fact },
            ValueType::Map {
                key: Box::new(ValueType::Text),
                value: Box::new(ValueType::Estimate {
                    value: Box::new(ValueType::Reference {
                        target: ReferenceKind::Record,
                    }),
                }),
            },
            Sensitivity::Private,
        ),
    ] {
        nodes.insert(
            id(name),
            NodeAst {
                inputs: BTreeMap::new(),
                outputs: BTreeMap::from([(
                    id("value"),
                    PortContract {
                        value_type,
                        sensitivity,
                        freshness: Some(DurationMs::new(5_000)),
                    },
                )]),
                operation: NodeOperation::Input {
                    source,
                    output: id("value"),
                },
            },
        );
    }

    nodes.insert(
        id("derive"),
        NodeAst {
            inputs: BTreeMap::from([(
                id("value"),
                InputBinding {
                    source: output("parameter_input", "value"),
                    expected_type: ValueType::I64,
                },
            )]),
            outputs: BTreeMap::from([
                (id("number"), port(ValueType::I64, Sensitivity::Private)),
                (id("selected"), port(ValueType::I64, Sensitivity::Private)),
            ]),
            operation: NodeOperation::Derive {
                expressions: BTreeMap::from([
                    (
                        id("number"),
                        ExpressionAst::Binary {
                            operator: BinaryOperator::Add,
                            left: Box::new(ExpressionAst::Input { input: id("value") }),
                            right: Box::new(ExpressionAst::Unary {
                                operator: UnaryOperator::Negate,
                                value: Box::new(ExpressionAst::Literal {
                                    value: LiteralValue::I64 { value: 2 },
                                }),
                            }),
                            precision: None,
                        },
                    ),
                    (
                        id("selected"),
                        ExpressionAst::If {
                            condition: Box::new(ExpressionAst::Literal {
                                value: LiteralValue::Bool { value: true },
                            }),
                            then_value: Box::new(ExpressionAst::Input { input: id("value") }),
                            else_value: Box::new(ExpressionAst::Literal {
                                value: LiteralValue::I64 { value: 0 },
                            }),
                        },
                    ),
                ]),
            },
        },
    );
    nodes.insert(
        id("delay"),
        NodeAst {
            inputs: BTreeMap::from([(
                id("next"),
                InputBinding {
                    source: output("derive", "number"),
                    expected_type: ValueType::I64,
                },
            )]),
            outputs: BTreeMap::from([(id("previous"), port(ValueType::I64, Sensitivity::Private))]),
            operation: NodeOperation::Delay {
                input: id("next"),
                output: id("previous"),
                initial: LiteralValue::I64 { value: 0 },
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

    ProgramAst {
        schema: ProgramSchema::V1,
        slug: Slug::new("tests.everything").unwrap(),
        purpose: "Every current AST form".to_string(),
        tags: BTreeSet::from([Slug::new("tests.dsl").unwrap(), Slug::new("apple").unwrap()]),
        parameters,
        nodes,
        outputs: BTreeMap::from([
            (id("current"), output("derive", "selected")),
            (id("previous"), output("delay", "previous")),
        ]),
        required_capabilities: CapabilitySet::new([
            Capability::KarmaRead,
            Capability::KarmaEvaluate,
            Capability::RecordAddQuantity,
        ]),
    }
}

fn parameter(
    name: &str,
    value_type: ValueType,
    default: LiteralValue,
) -> (LocalId, ParameterDefinition) {
    (
        id(name),
        ParameterDefinition {
            value_type,
            default,
            mutable: true,
        },
    )
}

fn id(value: &str) -> LocalId {
    LocalId::new(value).unwrap()
}

fn output(node: &str, port: &str) -> OutputRef {
    OutputRef {
        node: id(node),
        port: id(port),
    }
}

fn port(value_type: ValueType, sensitivity: Sensitivity) -> PortContract {
    PortContract {
        value_type,
        sensitivity,
        freshness: None,
    }
}

fn uid(kind: ReferenceKind, value: &str) -> TypedUid {
    TypedUid::new(kind, value).unwrap()
}

fn reference(kind: ReferenceKind, value: &str, slug: Option<&str>) -> ResolvedReference {
    ResolvedReference {
        target: uid(kind, value),
        display_slug: slug.map(|slug| Slug::new(slug).unwrap()),
    }
}
