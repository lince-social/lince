use std::collections::BTreeMap;

use crate::{
    ExpressionAst::{self, Binary, If, Input, Literal, Unary},
    LiteralValue, LocalId,
};

use super::{EvaluationError, EvaluationErrorCode, FuelMeter, Site};

mod arithmetic;

use arithmetic::{evaluate_binary, evaluate_unary};

pub(super) fn evaluate_expression(
    expression: &ExpressionAst,
    inputs: &BTreeMap<LocalId, LiteralValue>,
    site: Site<'_>,
    depth: u16,
    meter: &mut FuelMeter,
) -> Result<LiteralValue, EvaluationError> {
    if depth >= meter.limits.max_expression_depth {
        return Err(meter.error(
            EvaluationErrorCode::ExpressionDepthExceeded,
            site,
            "expression exceeds deterministic depth limit",
        ));
    }
    meter.consume(site)?;
    match expression {
        Literal { value } => Ok(value.clone()),
        Input { input } => inputs
            .get(input)
            .cloned()
            .ok_or_else(|| meter.invariant(site, "Proof accepted an unavailable expression input")),
        Unary { operator, value } => {
            let value = evaluate_expression(value, inputs, site, depth + 1, meter)?;
            evaluate_unary(*operator, value, site, meter)
        }
        Binary {
            operator,
            left,
            right,
            precision,
        } => {
            let left = evaluate_expression(left, inputs, site, depth + 1, meter)?;
            let right = evaluate_expression(right, inputs, site, depth + 1, meter)?;
            evaluate_binary(*operator, left, right, precision.as_ref(), site, meter)
        }
        If {
            condition,
            then_value,
            else_value,
        } => {
            let condition = evaluate_expression(condition, inputs, site, depth + 1, meter)?;
            match condition {
                LiteralValue::Bool { value } => {
                    let branch = if value { then_value } else { else_value };
                    evaluate_expression(branch, inputs, site, depth + 1, meter)
                }
                _ => Err(meter.invariant(site, "Proof accepted a non-Boolean if condition")),
            }
        }
    }
}
