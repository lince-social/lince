use std::cmp::Ordering;

use super::ConditionError;
use crate::{
    expr::BinOp,
    karma::{DecimalValue, MAX_DECIMAL_SCALE},
};

const MIN_DIVISION_SCALE: u8 = 6;

pub(crate) fn arithmetic(
    operator: BinOp,
    left: DecimalValue,
    right: DecimalValue,
) -> Result<DecimalValue, ConditionError> {
    match operator {
        BinOp::Add => left.aligned_add(right).ok_or(ConditionError::Overflow),
        BinOp::Sub => left.aligned_sub(right).ok_or(ConditionError::Overflow),
        BinOp::Mul => multiply(left, right),
        BinOp::Div => divide(left, right),
        BinOp::Rem if right.is_zero() => Err(ConditionError::DivisionByZero),
        BinOp::Rem => remainder(left, right),
        _ => Err(ConditionError::Parse("Expected arithmetic operator".into())),
    }
}

pub(super) fn compare(a: DecimalValue, b: DecimalValue) -> Result<Ordering, ConditionError> {
    let (a, b) = align(a, b)?;
    Ok(a.mantissa().cmp(&b.mantissa()))
}

// Conditions align before checking for a zero divisor. Scripts check for
// division by zero first, as handled in arithmetic above.
pub(super) fn remainder(a: DecimalValue, b: DecimalValue) -> Result<DecimalValue, ConditionError> {
    let (a, b) = align(a, b)?;
    if b.is_zero() {
        return Err(ConditionError::DivisionByZero);
    }
    let mantissa = a
        .mantissa()
        .checked_rem(b.mantissa())
        .ok_or(ConditionError::Overflow)?;
    DecimalValue::from_mantissa(a.scale(), mantissa).map_err(|_| ConditionError::Overflow)
}

fn align(a: DecimalValue, b: DecimalValue) -> Result<(DecimalValue, DecimalValue), ConditionError> {
    let scale = a.scale().max(b.scale());
    a.rescale(scale)
        .zip(b.rescale(scale))
        .ok_or(ConditionError::Overflow)
}

fn multiply(a: DecimalValue, b: DecimalValue) -> Result<DecimalValue, ConditionError> {
    let mantissa = a
        .mantissa()
        .checked_mul(b.mantissa())
        .ok_or(ConditionError::Overflow)?;
    let scale = a
        .scale()
        .checked_add(b.scale())
        .ok_or(ConditionError::Overflow)?;
    round_to_scale(scale, mantissa, MAX_DECIMAL_SCALE).ok_or(ConditionError::Overflow)
}

fn divide(a: DecimalValue, b: DecimalValue) -> Result<DecimalValue, ConditionError> {
    if b.is_zero() {
        return Err(ConditionError::DivisionByZero);
    }
    let scale = a.scale().max(b.scale()).max(MIN_DIVISION_SCALE);
    let shift = u32::from(scale - a.scale()) + u32::from(b.scale()) + 1;
    let numerator = 10_i128
        .checked_pow(shift)
        .and_then(|factor| a.mantissa().checked_mul(factor))
        .ok_or(ConditionError::Overflow)?;
    let quotient = numerator
        .checked_div(b.mantissa())
        .ok_or(ConditionError::Overflow)?;
    round_to_scale(scale + 1, quotient, scale).ok_or(ConditionError::Overflow)
}

fn round_to_scale(scale: u8, mantissa: i128, target: u8) -> Option<DecimalValue> {
    if target >= scale {
        return DecimalValue::from_mantissa(scale, mantissa).ok();
    }
    let factor = 10_i128.checked_pow(u32::from(scale - target))?;
    let quotient = mantissa / factor;
    let remainder = (mantissa % factor).abs();
    let rounded = if remainder >= factor / 2 {
        quotient.checked_add(mantissa.signum())?
    } else {
        quotient
    };
    DecimalValue::from_mantissa(target, rounded).ok()
}
