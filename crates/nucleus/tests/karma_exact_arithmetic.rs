use std::collections::{BTreeMap, BTreeSet};

use nucleus::karma::{
    BinaryOperator, CapabilitySet, DecimalPrecision, DecimalValue, DeclaredUnit,
    EvaluationErrorCode, EvaluationLimits, ExpressionAst, FrozenEvaluationContext, LiteralValue,
    LocalId, NodeAst, NodeOperation, OutputRef, PortContract, ProgramAst, ProgramSchema,
    ProofIssueCode, ProofStatus, ReferenceKind, Rounding, Sensitivity, Slug, TypedUid, ValueType,
    evaluate_program, prove_program,
};

const KG: &str = "c_01ARZ3NDEKTSV4RRFFQ69G5FAV";
const METRE: &str = "c_01ARZ3NDEKTSV4RRFFQ69G5FAW";

#[test]
fn scaling_a_quantity_by_a_plain_decimal_keeps_its_unit() {
    let program = product_program(
        BinaryOperator::Multiply,
        quantity("12.00", KG),
        decimal("0.15"),
        precision(2, Rounding::HalfUp, None),
        ValueType::Quantity {
            scale: 2,
            unit: uid(KG),
        },
    );
    let result = evaluate_program(
        &program,
        &FrozenEvaluationContext::default(),
        EvaluationLimits::default(),
    )
    .unwrap();

    assert_eq!(
        result.outputs.get(&id("answer")),
        Some(&quantity("1.80", KG))
    );
    assert!(result.rounding.is_empty());
}

#[test]
fn division_rounds_by_the_declared_rule_and_reports_what_it_discarded() {
    let program = product_program(
        BinaryOperator::Divide,
        decimal("10.00"),
        decimal("3"),
        precision(2, Rounding::HalfUp, None),
        ValueType::Decimal { scale: 2 },
    );
    let result = evaluate_program(
        &program,
        &FrozenEvaluationContext::default(),
        EvaluationLimits::default(),
    )
    .unwrap();

    assert_eq!(result.outputs.get(&id("answer")), Some(&decimal("3.33")));
    assert_eq!(result.rounding.len(), 1);
    let note = &result.rounding[0];
    assert_eq!(note.operator, BinaryOperator::Divide);
    assert_eq!(note.scale, 2);
    assert_eq!(note.rounding, Rounding::HalfUp);
    assert_eq!(note.result, value("3.33"));
}

#[test]
fn each_rounding_rule_gives_the_answer_it_names() {
    for (rounding, expected) in [
        (Rounding::HalfUp, "1.01"),
        (Rounding::HalfEven, "1.00"),
        (Rounding::TowardZero, "1.00"),
        (Rounding::AwayFromZero, "1.01"),
    ] {
        let program = product_program(
            BinaryOperator::Multiply,
            decimal("1.005"),
            decimal("1"),
            precision(2, rounding, None),
            ValueType::Decimal { scale: 2 },
        );
        let result = evaluate_program(
            &program,
            &FrozenEvaluationContext::default(),
            EvaluationLimits::default(),
        )
        .unwrap();
        assert_eq!(
            result.outputs.get(&id("answer")),
            Some(&decimal(expected)),
            "{rounding:?} should produce {expected}"
        );
        assert_eq!(
            result.rounding.len(),
            1,
            "{rounding:?} discarded a remainder"
        );
    }
}

#[test]
fn the_rounding_rule_is_part_of_the_revision_hash() {
    let half_up = product_program(
        BinaryOperator::Divide,
        decimal("10.00"),
        decimal("3"),
        precision(2, Rounding::HalfUp, None),
        ValueType::Decimal { scale: 2 },
    );
    let half_even = product_program(
        BinaryOperator::Divide,
        decimal("10.00"),
        decimal("3"),
        precision(2, Rounding::HalfEven, None),
        ValueType::Decimal { scale: 2 },
    );

    let first = prove_program(&half_up).revision_hash.unwrap();
    let second = prove_program(&half_even).revision_hash.unwrap();
    assert_ne!(first, second);

    let wider = product_program(
        BinaryOperator::Divide,
        decimal("10.00"),
        decimal("3"),
        precision(4, Rounding::HalfUp, None),
        ValueType::Decimal { scale: 4 },
    );
    assert_ne!(prove_program(&wider).revision_hash.unwrap(), first);
}

#[test]
fn dividing_an_exact_value_by_zero_is_a_typed_failure() {
    let program = product_program(
        BinaryOperator::Divide,
        decimal("10.00"),
        decimal("0.00"),
        precision(2, Rounding::HalfUp, None),
        ValueType::Decimal { scale: 2 },
    );
    let error = evaluate_program(
        &program,
        &FrozenEvaluationContext::default(),
        EvaluationLimits::default(),
    )
    .unwrap_err();
    assert_eq!(error.code, EvaluationErrorCode::DivisionByZero);
}

#[test]
fn an_undeclared_precision_is_refused_at_publish() {
    let program = product_program(
        BinaryOperator::Multiply,
        decimal("12.00"),
        decimal("0.15"),
        None,
        ValueType::Decimal { scale: 2 },
    );
    let proof = prove_program(&program);
    assert_eq!(proof.status, ProofStatus::Rejected);
    assert!(
        proof
            .issues
            .iter()
            .any(|issue| issue.code == ProofIssueCode::InvalidPrecision)
    );
}

#[test]
fn a_precision_on_an_operation_that_cannot_use_one_is_refused() {
    let program = product_program(
        BinaryOperator::Add,
        decimal("1.00"),
        decimal("2.00"),
        precision(2, Rounding::HalfUp, None),
        ValueType::Decimal { scale: 2 },
    );
    let proof = prove_program(&program);
    assert_eq!(proof.status, ProofStatus::Rejected);
    assert!(
        proof
            .issues
            .iter()
            .any(|issue| issue.code == ProofIssueCode::InvalidPrecision)
    );
}

#[test]
fn a_scale_beyond_the_maximum_is_refused_at_publish() {
    let program = product_program(
        BinaryOperator::Divide,
        decimal("10.00"),
        decimal("3"),
        precision(19, Rounding::HalfUp, None),
        ValueType::Decimal { scale: 2 },
    );
    let proof = prove_program(&program);
    assert_eq!(proof.status, ProofStatus::Rejected);
    assert!(
        proof
            .issues
            .iter()
            .any(|issue| issue.code == ProofIssueCode::InvalidPrecision)
    );
}

#[test]
fn combining_two_dimensioned_values_makes_the_author_name_the_result() {
    let undeclared = product_program(
        BinaryOperator::Divide,
        quantity("12.00", KG),
        quantity("3.00", METRE),
        precision(2, Rounding::HalfUp, None),
        ValueType::Decimal { scale: 2 },
    );
    let proof = prove_program(&undeclared);
    assert_eq!(proof.status, ProofStatus::Rejected);
    assert!(
        proof
            .issues
            .iter()
            .any(|issue| issue.code == ProofIssueCode::InvalidPrecision)
    );

    let ratio = product_program(
        BinaryOperator::Divide,
        quantity("12.00", KG),
        quantity("3.00", KG),
        precision(2, Rounding::HalfUp, Some(DeclaredUnit::Dimensionless)),
        ValueType::Decimal { scale: 2 },
    );
    assert_eq!(prove_program(&ratio).status, ProofStatus::Accepted);
    let result = evaluate_program(
        &ratio,
        &FrozenEvaluationContext::default(),
        EvaluationLimits::default(),
    )
    .unwrap();
    assert_eq!(result.outputs.get(&id("answer")), Some(&decimal("4.00")));

    let declared = product_program(
        BinaryOperator::Divide,
        quantity("12.00", KG),
        quantity("3.00", METRE),
        precision(
            2,
            Rounding::HalfUp,
            Some(DeclaredUnit::Unit { unit: uid(KG) }),
        ),
        ValueType::Quantity {
            scale: 2,
            unit: uid(KG),
        },
    );
    assert_eq!(prove_program(&declared).status, ProofStatus::Accepted);
}

#[test]
fn declaring_a_unit_where_the_value_already_has_one_is_refused() {
    let program = product_program(
        BinaryOperator::Multiply,
        quantity("12.00", KG),
        decimal("0.15"),
        precision(
            2,
            Rounding::HalfUp,
            Some(DeclaredUnit::Unit { unit: uid(METRE) }),
        ),
        ValueType::Quantity {
            scale: 2,
            unit: uid(METRE),
        },
    );
    let proof = prove_program(&program);
    assert_eq!(proof.status, ProofStatus::Rejected);
    assert!(
        proof
            .issues
            .iter()
            .any(|issue| issue.code == ProofIssueCode::InvalidPrecision)
    );
}

#[test]
fn integer_multiplication_keeps_working_without_a_precision() {
    let program = product_program(
        BinaryOperator::Multiply,
        LiteralValue::I64 { value: 6 },
        LiteralValue::I64 { value: 7 },
        None,
        ValueType::I64,
    );
    assert_eq!(prove_program(&program).status, ProofStatus::Accepted);
    let result = evaluate_program(
        &program,
        &FrozenEvaluationContext::default(),
        EvaluationLimits::default(),
    )
    .unwrap();
    assert_eq!(
        result.outputs.get(&id("answer")),
        Some(&LiteralValue::I64 { value: 42 })
    );
    assert!(result.rounding.is_empty());
}

#[test]
fn a_product_that_fits_is_not_reported_as_overflow() {
    let left = DecimalValue::parse_canonical(9, "1000000.000000000").unwrap();
    let right = DecimalValue::parse_canonical(9, "1000000.000000000").unwrap();
    let product = left.mul_exact(right, 18, Rounding::HalfUp).unwrap();
    assert!(product.exact);
    assert_eq!(
        product.value,
        DecimalValue::parse_canonical(18, "1000000000000.000000000000000000").unwrap()
    );

    let narrow = left.mul_exact(right, 2, Rounding::HalfUp).unwrap();
    assert!(narrow.exact);
    assert_eq!(
        narrow.value,
        DecimalValue::parse_canonical(2, "1000000000000.00").unwrap()
    );
}

#[test]
fn narrowing_cancels_into_the_operands_before_multiplying_them() {
    let left = DecimalValue::parse_canonical(9, "50000000000.000000000").unwrap();
    let right = DecimalValue::parse_canonical(9, "50000000000.000000000").unwrap();
    let product = left.mul_exact(right, 2, Rounding::HalfUp).unwrap();
    assert!(product.exact);
    assert_eq!(
        product.value,
        DecimalValue::parse_canonical(2, "2500000000000000000000.00").unwrap()
    );
}

#[test]
fn cancellation_never_drops_a_digit_that_would_change_the_rounding() {
    let value = DecimalValue::parse_canonical(3, "0.125").unwrap();
    let one = DecimalValue::parse_canonical(0, "1").unwrap();

    let half_up = value.mul_exact(one, 2, Rounding::HalfUp).unwrap();
    assert!(!half_up.exact);
    assert_eq!(
        half_up.value,
        DecimalValue::parse_canonical(2, "0.13").unwrap()
    );

    let half_even = value.mul_exact(one, 2, Rounding::HalfEven).unwrap();
    assert!(!half_even.exact);
    assert_eq!(
        half_even.value,
        DecimalValue::parse_canonical(2, "0.12").unwrap()
    );
}

#[test]
fn overflow_is_a_typed_failure_rather_than_a_wrapped_value() {
    let huge = DecimalValue::parse_canonical(0, "170141183460469231731687303715884105727").unwrap();
    assert!(huge.mul_exact(huge, 0, Rounding::HalfUp).is_none());
    assert!(huge.mul_exact(huge, 18, Rounding::HalfUp).is_none());
}

fn product_program(
    operator: BinaryOperator,
    left: LiteralValue,
    right: LiteralValue,
    precision: Option<DecimalPrecision>,
    result_type: ValueType,
) -> ProgramAst {
    let mut program = ProgramAst {
        schema: ProgramSchema::V1,
        slug: Slug::new("test.product").unwrap(),
        purpose: "Exact multiplication and division".to_string(),
        tags: BTreeSet::new(),
        parameters: BTreeMap::new(),
        nodes: BTreeMap::new(),
        outputs: BTreeMap::new(),
        required_capabilities: CapabilitySet::default(),
    };
    program.nodes.insert(
        id("product"),
        NodeAst {
            inputs: BTreeMap::new(),
            outputs: BTreeMap::from([(
                id("value"),
                PortContract {
                    value_type: result_type,
                    sensitivity: Sensitivity::Private,
                    freshness: None,
                },
            )]),
            operation: NodeOperation::Derive {
                expressions: BTreeMap::from([(
                    id("value"),
                    ExpressionAst::Binary {
                        operator,
                        left: Box::new(ExpressionAst::Literal { value: left }),
                        right: Box::new(ExpressionAst::Literal { value: right }),
                        precision,
                    },
                )]),
            },
        },
    );
    program.outputs.insert(
        id("answer"),
        OutputRef {
            node: id("product"),
            port: id("value"),
        },
    );
    program
}

fn precision(
    scale: u8,
    rounding: Rounding,
    result_unit: Option<DeclaredUnit>,
) -> Option<DecimalPrecision> {
    Some(DecimalPrecision {
        scale,
        rounding,
        result_unit,
    })
}

fn value(text: &str) -> DecimalValue {
    DecimalValue::parse_inferred(text).unwrap()
}

fn decimal(text: &str) -> LiteralValue {
    LiteralValue::Decimal { value: value(text) }
}

fn quantity(text: &str, unit: &str) -> LiteralValue {
    LiteralValue::Quantity {
        amount: value(text),
        unit: uid(unit),
    }
}

fn uid(value: &str) -> TypedUid {
    TypedUid::new(ReferenceKind::Unit, value).unwrap()
}

fn id(value: &str) -> LocalId {
    LocalId::new(value).unwrap()
}
