use crate::expr::{BinOp, Expr, UnOp};
use crate::karma::Rounding;
use crate::{DecimalValue, NucleusError};

pub const SCALE: u8 = 18;
pub const ROUNDING: Rounding = Rounding::HalfEven;

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum EffectMode {
    #[default]
    Quantity,
    Fulfilment,
}

#[derive(
    Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct PrivateEffect {
    pub record: String,
    pub formula: String,
    #[serde(default)]
    pub mode: EffectMode,
}

fn invalid(message: &str) -> NucleusError {
    NucleusError::Eval(message.into())
}

fn normalized(value: DecimalValue) -> DecimalValue {
    let mut scale = value.scale();
    let mut mantissa = value.mantissa();
    while scale > 0 && mantissa % 10 == 0 {
        scale -= 1;
        mantissa /= 10;
    }
    DecimalValue::from_mantissa(scale, mantissa).unwrap()
}

pub fn amount(value: f64) -> Result<DecimalValue, NucleusError> {
    DecimalValue::from_f64_lossy(value)
        .map(normalized)
        .map_err(|_| invalid("quantity must be a finite decimal with at most 18 places"))
}

pub fn sum(values: impl IntoIterator<Item = DecimalValue>) -> Result<DecimalValue, NucleusError> {
    values.into_iter().try_fold(
        DecimalValue::from_mantissa(0, 0).unwrap(),
        |total, value| {
            total
                .aligned_add(value)
                .map(normalized)
                .ok_or_else(|| invalid("quantity sum exceeds the supported decimal range"))
        },
    )
}

pub fn difference(after: DecimalValue, before: DecimalValue) -> Result<DecimalValue, NucleusError> {
    after
        .aligned_sub(before)
        .map(normalized)
        .ok_or_else(|| invalid("quantity difference exceeds the supported decimal range"))
}

pub fn evaluate_exact(formula: &str, incoming: DecimalValue) -> Result<DecimalValue, NucleusError> {
    if formula.is_empty() || formula.chars().count() > 2_000 {
        return Err(invalid(
            "application formula must contain 1 to 2000 characters",
        ));
    }
    fn evaluate(
        expr: &Expr,
        incoming: DecimalValue,
        depth: u8,
    ) -> Result<DecimalValue, NucleusError> {
        if depth > 64 {
            return Err(invalid("application formula is too deeply nested"));
        }
        let next = |expr: &Expr| evaluate(expr, incoming, depth + 1);
        let result = match expr {
            Expr::Num(text) => {
                Some(DecimalValue::parse_inferred(text).map_err(|_| {
                    invalid("application numbers require at most 18 decimal places")
                })?)
            }
            Expr::Fn(name, args) if name == "incoming" && args.is_empty() => Some(incoming),
            Expr::Unary(UnOp::Neg, value) => next(value)?.checked_neg(),
            Expr::Bin(operator, left, right) => {
                let left = next(left)?;
                let right = next(right)?;
                match operator {
                    BinOp::Add => left.aligned_add(right),
                    BinOp::Sub => left.aligned_sub(right),
                    BinOp::Mul => left
                        .mul_exact(right, SCALE, ROUNDING)
                        .map(|result| result.value),
                    BinOp::Div => left
                        .div_exact(right, SCALE, ROUNDING)
                        .map(|result| result.value),
                    BinOp::Rem => {
                        let scale = left.scale().max(right.scale());
                        left.rescale(scale)
                            .zip(right.rescale(scale))
                            .and_then(|(left, right)| left.mantissa().checked_rem(right.mantissa()))
                            .and_then(|value| DecimalValue::from_mantissa(scale, value).ok())
                    }
                    _ => {
                        return Err(invalid(
                            "application formulas support only incoming(), decimal numbers and arithmetic",
                        ));
                    }
                }
            }
            _ => {
                return Err(invalid(
                    "application formulas support only incoming(), decimal numbers and arithmetic",
                ));
            }
        };
        result.map(normalized).ok_or_else(|| {
            invalid(
                "application calculation divides by zero or exceeds the supported decimal range",
            )
        })
    }
    evaluate(&Expr::parse(formula)?, normalized(incoming), 0)
}

pub fn cumulative_delta(
    formula: &str,
    incoming: DecimalValue,
    applied: DecimalValue,
) -> Result<DecimalValue, NucleusError> {
    difference(evaluate_exact(formula, incoming)?, applied)
}

pub fn evaluate(formula: &str, incoming: f64) -> Result<f64, NucleusError> {
    Ok(evaluate_exact(formula, amount(incoming)?)?.to_f64())
}

pub fn validate(formula: &str) -> Result<String, NucleusError> {
    let formula = formula.trim();
    for incoming in [-1, 0, 1] {
        evaluate_exact(formula, DecimalValue::from_mantissa(0, incoming).unwrap())?;
    }
    Ok(formula.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decimal(value: &str) -> DecimalValue {
        DecimalValue::parse_inferred(value).unwrap()
    }

    #[test]
    fn full_and_split_effects_have_the_same_exact_total() {
        for (formula, amounts) in [
            ("-incoming() / 2", vec!["0.1", "0.2", "0.3"]),
            ("incoming() / 3", vec!["1", "2", "3"]),
            ("incoming() * incoming()", vec!["0.01", "0.02", "0.03"]),
        ] {
            let mut applied = decimal("0");
            for total in &amounts {
                let delta = cumulative_delta(formula, decimal(total), applied).unwrap();
                applied = sum([applied, delta]).unwrap();
            }
            assert_eq!(
                applied,
                evaluate_exact(formula, decimal(amounts.last().unwrap())).unwrap()
            );
        }
        assert_eq!(
            evaluate_exact("incoming() + 0.2", decimal("0.1")).unwrap(),
            decimal("0.3")
        );
        assert_eq!(
            evaluate_exact("-incoming()/2", decimal("10")).unwrap(),
            decimal("-5")
        );
    }

    #[test]
    fn decimal_rounding_and_invalid_calculations_are_explicit() {
        assert_eq!(
            evaluate_exact("incoming()/2", decimal("0.000000000000000001")).unwrap(),
            decimal("0")
        );
        assert_eq!(
            evaluate_exact("incoming()/2", decimal("0.000000000000000003")).unwrap(),
            decimal("0.000000000000000002")
        );
        assert_eq!(
            evaluate_exact("incoming()/3", decimal("1")).unwrap(),
            decimal("0.333333333333333333")
        );
        for formula in [
            "incoming()/0",
            "incoming()%0",
            "record('private')",
            "incoming() > 0",
            "170141183460469231731687303715884105727 + 1",
        ] {
            assert!(evaluate_exact(formula, decimal("1")).is_err(), "{formula}");
        }
        assert!(amount(f64::INFINITY).is_err());
    }
}
