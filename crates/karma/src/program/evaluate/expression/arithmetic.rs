use std::cmp::Ordering;

use crate::{
    BinaryOperator::{
        self, Add, And, Divide, Equal, Greater, GreaterOrEqual, Less, LessOrEqual, Multiply,
        NotEqual, Or, Remainder, Subtract,
    },
    DecimalPrecision, DecimalValue, DeclaredUnit, DurationMs,
    LiteralValue::{self, Bool, Datum, Decimal, Duration, I64, Quantity, Reference, Timestamp},
    RoundedDecimal,
    UnaryOperator::{self, Negate, Not},
};

use super::{EvaluationError, EvaluationErrorCode, FuelMeter, Site};

pub(super) fn evaluate_unary(
    operator: UnaryOperator,
    value: LiteralValue,
    site: Site<'_>,
    meter: &FuelMeter,
) -> Result<LiteralValue, EvaluationError> {
    let overflow = || {
        meter.error(
            EvaluationErrorCode::ArithmeticOverflow,
            site,
            "exact unary arithmetic overflowed",
        )
    };
    match (operator, value) {
        (Not, Bool { value }) => Ok(Bool { value: !value }),
        (Negate, I64 { value }) => value
            .checked_neg()
            .map(|value| I64 { value })
            .ok_or_else(overflow),
        (Negate, Decimal { value }) => value
            .checked_neg()
            .map(|value| Decimal { value })
            .ok_or_else(overflow),
        (Negate, Duration { value }) => value
            .get()
            .checked_neg()
            .map(|value| Duration {
                value: DurationMs::new(value),
            })
            .ok_or_else(overflow),
        (Negate, Quantity { amount, unit }) => amount
            .checked_neg()
            .map(|amount| Quantity { amount, unit })
            .ok_or_else(overflow),
        _ => Err(meter.invariant(site, "Proof accepted an invalid unary expression")),
    }
}

pub(super) fn evaluate_binary(
    operator: BinaryOperator,
    left: LiteralValue,
    right: LiteralValue,
    precision: Option<&DecimalPrecision>,
    site: Site<'_>,
    meter: &mut FuelMeter,
) -> Result<LiteralValue, EvaluationError> {
    match operator {
        Equal | NotEqual => Ok(Bool {
            value: semantic_equal(&left, &right) == (operator == Equal),
        }),
        Less | LessOrEqual | Greater | GreaterOrEqual => {
            let ordering = left.semantic_cmp(&right).ok_or_else(|| {
                meter.invariant(site, "Proof accepted values without a semantic ordering")
            })?;
            let value = match operator {
                Less => ordering == Ordering::Less,
                LessOrEqual => ordering != Ordering::Greater,
                Greater => ordering == Ordering::Greater,
                GreaterOrEqual => ordering != Ordering::Less,
                _ => return invariant_expression(site, meter),
            };
            Ok(Bool { value })
        }
        And | Or => match (left, right) {
            (Bool { value: left }, Bool { value: right }) => Ok(Bool {
                value: if operator == And {
                    left && right
                } else {
                    left || right
                },
            }),
            _ => invariant_expression(site, meter),
        },
        Add | Subtract => evaluate_add_sub(operator, left, right, site, meter),
        Multiply | Divide | Remainder => {
            evaluate_product(operator, left, right, precision, site, meter)
        }
    }
}

fn evaluate_add_sub(
    operator: BinaryOperator,
    left: LiteralValue,
    right: LiteralValue,
    site: Site<'_>,
    meter: &FuelMeter,
) -> Result<LiteralValue, EvaluationError> {
    let add = operator == Add;
    let integers = |left: i64, right| {
        if add {
            left.checked_add(right)
        } else {
            left.checked_sub(right)
        }
    };
    let decimals = |left: DecimalValue, right| {
        if add {
            left.checked_add(right)
        } else {
            left.checked_sub(right)
        }
    };
    let overflow = || arithmetic_overflow(site, meter);
    match (left, right) {
        (I64 { value: left }, I64 { value: right }) => integers(left, right)
            .map(|value| I64 { value })
            .ok_or_else(overflow),
        (Decimal { value: left }, Decimal { value: right }) => decimals(left, right)
            .map(|value| Decimal { value })
            .ok_or_else(overflow),
        (Duration { value: left }, Duration { value: right }) => integers(left.get(), right.get())
            .map(|value| Duration {
                value: DurationMs::new(value),
            })
            .ok_or_else(overflow),
        (Quantity { amount: left, unit }, Quantity { amount: right, .. }) => decimals(left, right)
            .map(|amount| Quantity { amount, unit })
            .ok_or_else(overflow),
        (Timestamp { value: timestamp }, Duration { value: duration }) => {
            let duration = if add {
                Some(duration)
            } else {
                duration.get().checked_neg().map(DurationMs::new)
            }
            .ok_or_else(overflow)?;
            timestamp
                .checked_add(duration)
                .map(|value| Timestamp { value })
                .ok_or_else(overflow)
        }
        (Duration { value: duration }, Timestamp { value: timestamp }) if add => timestamp
            .checked_add(duration)
            .map(|value| Timestamp { value })
            .ok_or_else(overflow),
        (Timestamp { value: left }, Timestamp { value: right }) if !add => left
            .as_millis()
            .checked_sub(right.as_millis())
            .map(|value| Duration {
                value: DurationMs::new(value),
            })
            .ok_or_else(overflow),
        _ => invariant_expression(site, meter),
    }
}

fn evaluate_product(
    operator: BinaryOperator,
    left: LiteralValue,
    right: LiteralValue,
    precision: Option<&DecimalPrecision>,
    site: Site<'_>,
    meter: &mut FuelMeter,
) -> Result<LiteralValue, EvaluationError> {
    if let Some(precision) = precision {
        return evaluate_exact_product(operator, left, right, precision, site, meter);
    }
    match (left, right) {
        (I64 { value: left }, I64 { value: right }) => {
            if matches!(operator, Divide | Remainder) && right == 0 {
                return Err(meter.error(
                    EvaluationErrorCode::DivisionByZero,
                    site,
                    "integer division or remainder by zero",
                ));
            }
            let value = match operator {
                Multiply => left.checked_mul(right),
                Divide => left.checked_div(right),
                Remainder => left.checked_rem(right),
                _ => return invariant_expression(site, meter),
            };
            value
                .map(|value| I64 { value })
                .ok_or_else(|| arithmetic_overflow(site, meter))
        }
        (Duration { value: left }, Duration { value: right }) if operator == Remainder => {
            if right.get() == 0 {
                return Err(meter.error(
                    EvaluationErrorCode::DivisionByZero,
                    site,
                    "duration remainder by zero",
                ));
            }
            left.get()
                .checked_rem(right.get())
                .map(|value| Duration {
                    value: DurationMs::new(value),
                })
                .ok_or_else(|| arithmetic_overflow(site, meter))
        }
        _ => invariant_expression(site, meter),
    }
}

fn evaluate_exact_product(
    operator: BinaryOperator,
    left: LiteralValue,
    right: LiteralValue,
    precision: &DecimalPrecision,
    site: Site<'_>,
    meter: &mut FuelMeter,
) -> Result<LiteralValue, EvaluationError> {
    let (Some(left_amount), Some(right_amount)) = (exact_amount(&left), exact_amount(&right))
    else {
        return invariant_expression(site, meter);
    };
    if operator == Divide && right_amount.is_zero() {
        return Err(meter.error(
            EvaluationErrorCode::DivisionByZero,
            site,
            "division by zero",
        ));
    }

    let RoundedDecimal { value, exact } = match operator {
        Multiply => left_amount.mul_exact(right_amount, precision.scale, precision.rounding),
        Divide => left_amount.div_exact(right_amount, precision.scale, precision.rounding),
        _ => return invariant_expression(site, meter),
    }
    .ok_or_else(|| arithmetic_overflow(site, meter))?;

    if !exact {
        meter.note_rounding(site, operator, precision, value);
    }
    Ok(exact_literal(left, right, operator, precision, value))
}

fn exact_amount(value: &LiteralValue) -> Option<DecimalValue> {
    match value {
        Decimal { value } => Some(*value),
        Quantity { amount, .. } => Some(*amount),
        _ => None,
    }
}

fn exact_literal(
    left: LiteralValue,
    right: LiteralValue,
    operator: BinaryOperator,
    precision: &DecimalPrecision,
    amount: DecimalValue,
) -> LiteralValue {
    let multiply = operator == Multiply;
    match (left, right) {
        (Quantity { unit, .. }, Decimal { .. }) => Quantity { amount, unit },
        (Decimal { .. }, Quantity { unit, .. }) if multiply => Quantity { amount, unit },
        _ => match &precision.result_unit {
            Some(DeclaredUnit::Unit { unit }) => Quantity {
                amount,
                unit: unit.clone(),
            },
            _ => Decimal { value: amount },
        },
    }
}

fn semantic_equal(left: &LiteralValue, right: &LiteralValue) -> bool {
    match (left, right) {
        (Reference { value: left }, Reference { value: right }) => left.target == right.target,
        (
            Datum {
                state: left_state,
                value: left_value,
                value_type: left_type,
            },
            Datum {
                state: right_state,
                value: right_value,
                value_type: right_type,
            },
        ) => {
            left_state == right_state
                && left_type == right_type
                && match (left_value, right_value) {
                    (Some(left), Some(right)) => semantic_equal(left, right),
                    (None, None) => true,
                    _ => false,
                }
        }
        _ => left == right,
    }
}

fn invariant_expression(
    site: Site<'_>,
    meter: &FuelMeter,
) -> Result<LiteralValue, EvaluationError> {
    Err(meter.invariant(site, "Proof accepted an invalid expression operation"))
}

fn arithmetic_overflow(site: Site<'_>, meter: &FuelMeter) -> EvaluationError {
    meter.error(
        EvaluationErrorCode::ArithmeticOverflow,
        site,
        "exact arithmetic overflowed",
    )
}
