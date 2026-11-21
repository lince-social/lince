use std::borrow::Cow;

use crate::{
    BinaryOperator::{
        self, Add, And, Divide, Equal, Greater, GreaterOrEqual, Less, LessOrEqual, Multiply,
        NotEqual, Or, Remainder, Subtract,
    },
    DecimalPrecision, DeclaredUnit,
    ExpressionAst::{self, Binary, If, Input, Literal, Unary},
    LocalId, MAX_DECIMAL_SCALE, ReferenceKind, TypedUid, UnaryOperator,
    ValueType::{self, Bool, Decimal, Duration, I64, Quantity, Timestamp},
};

use super::{
    ProofIssueCode::{
        self, ExpressionTypeMismatch, InvalidLiteral, InvalidPrecision, UndeclaredExpressionInput,
    },
    node::NodeValidation,
};

impl<'program> NodeValidation<'program> {
    pub(super) fn infer_expression(
        &mut self,
        expression: &ExpressionAst,
        output: &LocalId,
    ) -> Option<Cow<'program, ValueType>> {
        match expression {
            Literal { value } => match value.value_type() {
                Ok(value_type) => Some(Cow::Owned(value_type)),
                Err(error) => {
                    self.expression_issue(InvalidLiteral, output, error.to_string());
                    None
                }
            },
            Input { input } => match self.node.inputs.get(input) {
                Some(binding) => Some(Cow::Borrowed(&binding.expected_type)),
                None => {
                    self.expression_issue(
                        UndeclaredExpressionInput,
                        output,
                        format!("expression input `{input:?}` is not declared by this node"),
                    );
                    None
                }
            },
            Unary { operator, value } => {
                let value_type = self.infer_expression(value, output)?;
                match operator {
                    UnaryOperator::Not if value_type.as_ref() == &Bool => Some(value_type),
                    UnaryOperator::Negate if value_type.is_negatable() => Some(value_type),
                    _ => {
                        self.expression_issue(
                            ExpressionTypeMismatch,
                            output,
                            format!("invalid {operator:?} for {value_type:?}"),
                        );
                        None
                    }
                }
            }
            Binary {
                operator,
                left,
                right,
                precision,
            } => {
                let left = self.infer_expression(left, output)?;
                let right = self.infer_expression(right, output)?;
                match infer_binary(*operator, &left, &right, precision.as_ref()) {
                    Ok(value_type) => Some(Cow::Owned(value_type)),
                    Err(BinaryIssue::Types) => {
                        self.expression_issue(
                            ExpressionTypeMismatch,
                            output,
                            format!("invalid {operator:?} for {left:?} and {right:?}"),
                        );
                        None
                    }
                    Err(BinaryIssue::Precision(message)) => {
                        self.expression_issue(
                            InvalidPrecision,
                            output,
                            format!("invalid {operator:?} precision: {message}"),
                        );
                        None
                    }
                }
            }
            If {
                condition,
                then_value,
                else_value,
            } => {
                let condition = self.infer_expression(condition, output)?;
                let then_type = self.infer_expression(then_value, output)?;
                let else_type = self.infer_expression(else_value, output)?;
                if condition.as_ref() == &Bool && then_type == else_type {
                    Some(then_type)
                } else {
                    self.expression_issue(
                        ExpressionTypeMismatch,
                        output,
                        "if requires a bool condition and exactly matching branch types",
                    );
                    None
                }
            }
        }
    }

    pub(super) fn expression_issue(
        &mut self,
        code: ProofIssueCode,
        output: &LocalId,
        message: impl Into<String>,
    ) {
        self.issue(
            code,
            format!("{}/operation/expressions/{}", self.path, output.as_str()),
            message,
        );
    }
}

enum BinaryIssue {
    Types,
    Precision(Cow<'static, str>),
}

enum Dimension<'a> {
    Plain,
    Unit(&'a TypedUid),
}

fn dimension(value_type: &ValueType) -> Option<Dimension<'_>> {
    match value_type {
        Decimal { .. } => Some(Dimension::Plain),
        Quantity { unit, .. } => Some(Dimension::Unit(unit)),
        _ => None,
    }
}

fn infer_binary(
    operator: BinaryOperator,
    left: &ValueType,
    right: &ValueType,
    precision: Option<&DecimalPrecision>,
) -> Result<ValueType, BinaryIssue> {
    let exact_product = matches!(operator, Multiply | Divide)
        && (dimension(left).is_some() || dimension(right).is_some());
    if exact_product {
        return infer_exact_product(operator, left, right, precision);
    }
    if precision.is_some() {
        return Err(BinaryIssue::Precision(
            "only multiplication and division over exact values declare a precision".into(),
        ));
    }
    infer_plain_binary(operator, left, right).ok_or(BinaryIssue::Types)
}

fn infer_exact_product(
    operator: BinaryOperator,
    left: &ValueType,
    right: &ValueType,
    precision: Option<&DecimalPrecision>,
) -> Result<ValueType, BinaryIssue> {
    let Some(precision) = precision else {
        return Err(BinaryIssue::Precision(
            "multiplication and division over exact values must declare a result scale and \
             rounding, because neither operation is closed over fixed-point decimals"
                .into(),
        ));
    };
    if precision.scale > MAX_DECIMAL_SCALE {
        return Err(BinaryIssue::Precision(
            format!(
                "declared scale {} exceeds maximum {MAX_DECIMAL_SCALE}",
                precision.scale
            )
            .into(),
        ));
    }
    let scale = precision.scale;
    let (Some(left_dim), Some(right_dim)) = (dimension(left), dimension(right)) else {
        return Err(BinaryIssue::Types);
    };
    let multiply = operator == Multiply;

    let unit = match (left_dim, right_dim) {
        (Dimension::Plain, Dimension::Plain) => {
            keeps_own_dimension(precision)?;
            None
        }
        (Dimension::Unit(unit), Dimension::Plain) => {
            keeps_own_dimension(precision)?;
            Some(unit)
        }
        (Dimension::Plain, Dimension::Unit(unit)) if multiply => {
            keeps_own_dimension(precision)?;
            Some(unit)
        }
        _ => match &precision.result_unit {
            Some(DeclaredUnit::Dimensionless) => None,
            Some(DeclaredUnit::Unit { unit }) if unit.kind() == ReferenceKind::Unit => Some(unit),
            Some(DeclaredUnit::Unit { .. }) => return Err(BinaryIssue::Precision(
                "declared result unit must be a unit reference".into(),
            )),
            None => return Err(BinaryIssue::Precision(
                "combining two dimensioned values must declare the result unit, or declare it dimensionless".into(),
            )),
        },
    };
    Ok(unit.map_or(Decimal { scale }, |unit| Quantity {
        scale,
        unit: unit.clone(),
    }))
}

fn keeps_own_dimension(precision: &DecimalPrecision) -> Result<(), BinaryIssue> {
    if precision.result_unit.is_some() {
        return Err(BinaryIssue::Precision(
            "scaling by a plain decimal keeps the value's own unit; remove the declared result unit"
                .into(),
        ));
    }
    Ok(())
}

fn infer_plain_binary(
    operator: BinaryOperator,
    left: &ValueType,
    right: &ValueType,
) -> Option<ValueType> {
    match operator {
        And | Or if left == &Bool && right == &Bool => Some(Bool),
        Equal | NotEqual if left == right => Some(Bool),
        Less | LessOrEqual | Greater | GreaterOrEqual
            if left == right && left.is_ordered_scalar() =>
        {
            Some(Bool)
        }
        Add | Subtract if left == right && left.is_additive() => Some(left.clone()),
        Add if (left == &Timestamp && right == &Duration)
            || (left == &Duration && right == &Timestamp) =>
        {
            Some(Timestamp)
        }
        Subtract if left == &Timestamp && right == &Duration => Some(Timestamp),
        Subtract if left == &Timestamp && right == &Timestamp => Some(Duration),
        Multiply | Divide if left == &I64 && right == &I64 => Some(I64),
        Remainder if left == right && matches!(left, I64 | Duration) => Some(left.clone()),
        _ => None,
    }
}
