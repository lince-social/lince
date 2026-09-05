use serde::{Deserialize, Serialize};
use std::fmt;

use crate::expr::{BinOp, Expr, UnOp};
use crate::karma::{DecimalValue, MAX_DECIMAL_SCALE};

const MIN_DIVISION_SCALE: u8 = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConditionError {
    Parse(String),
    UnknownFunction(String),
    UnknownReference(String),
    Overflow,
    DivisionByZero,
    MissingWindow(String),
}

impl fmt::Display for ConditionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(m) => write!(f, "cannot read that condition: {m}"),
            Self::UnknownFunction(m) => write!(f, "no such reading: {m}"),
            Self::UnknownReference(m) => write!(f, "nothing here is called {m}"),
            Self::Overflow => write!(f, "that number is too large to work with"),
            Self::DivisionByZero => write!(f, "divided by zero"),
            Self::MissingWindow(m) => write!(f, "{m} needs a period, like 30d"),
        }
    }
}

impl std::error::Error for ConditionError {}

pub trait ExactResolver {
    fn lookup(
        &mut self,
        func: &str,
        slug: &str,
        window_secs: Option<i64>,
    ) -> Result<DecimalValue, ConditionError>;
}

fn zero() -> DecimalValue {
    DecimalValue::from_mantissa(0, 0).expect("scale 0 is valid")
}

fn one() -> DecimalValue {
    DecimalValue::from_mantissa(0, 1).expect("scale 0 is valid")
}

fn pow10(exp: u32) -> Option<i128> {
    10i128.checked_pow(exp)
}

fn upscale(value: DecimalValue, scale: u8) -> Option<DecimalValue> {
    if scale < value.scale() || scale > MAX_DECIMAL_SCALE {
        return None;
    }
    let factor = pow10((scale - value.scale()) as u32)?;
    DecimalValue::from_mantissa(scale, value.mantissa().checked_mul(factor)?).ok()
}

fn downscale(scale: u8, mantissa: i128, target: u8) -> Option<DecimalValue> {
    if target >= scale {
        return DecimalValue::from_mantissa(scale, mantissa).ok();
    }
    let factor = pow10((scale - target) as u32)?;
    let quotient = mantissa / factor;
    let remainder = (mantissa % factor).abs();
    let rounded = if remainder * 2 >= factor {
        quotient + if mantissa.is_negative() { -1 } else { 1 }
    } else {
        quotient
    };
    DecimalValue::from_mantissa(target, rounded).ok()
}

fn align(a: DecimalValue, b: DecimalValue) -> Option<(DecimalValue, DecimalValue)> {
    let scale = a.scale().max(b.scale());
    Some((upscale(a, scale)?, upscale(b, scale)?))
}

fn add(a: DecimalValue, b: DecimalValue) -> Result<DecimalValue, ConditionError> {
    let (a, b) = align(a, b).ok_or(ConditionError::Overflow)?;
    a.checked_add(b).ok_or(ConditionError::Overflow)
}

fn sub(a: DecimalValue, b: DecimalValue) -> Result<DecimalValue, ConditionError> {
    let (a, b) = align(a, b).ok_or(ConditionError::Overflow)?;
    a.checked_sub(b).ok_or(ConditionError::Overflow)
}

fn mul(a: DecimalValue, b: DecimalValue) -> Result<DecimalValue, ConditionError> {
    let mantissa = a
        .mantissa()
        .checked_mul(b.mantissa())
        .ok_or(ConditionError::Overflow)?;
    let scale = a.scale() as u16 + b.scale() as u16;
    if scale <= MAX_DECIMAL_SCALE as u16 {
        return DecimalValue::from_mantissa(scale as u8, mantissa)
            .map_err(|_| ConditionError::Overflow);
    }
    downscale(scale.min(255) as u8, mantissa, MAX_DECIMAL_SCALE).ok_or(ConditionError::Overflow)
}

fn div(a: DecimalValue, b: DecimalValue) -> Result<DecimalValue, ConditionError> {
    if b.mantissa() == 0 {
        return Err(ConditionError::DivisionByZero);
    }
    let target = a.scale().max(b.scale()).max(MIN_DIVISION_SCALE);
    let shift = (target - a.scale()) as u32 + b.scale() as u32 + 1;
    let factor = pow10(shift).ok_or(ConditionError::Overflow)?;
    let numerator = a
        .mantissa()
        .checked_mul(factor)
        .ok_or(ConditionError::Overflow)?;
    let quotient = numerator / b.mantissa();
    downscale(target + 1, quotient, target).ok_or(ConditionError::Overflow)
}

fn compare(a: DecimalValue, b: DecimalValue) -> Result<std::cmp::Ordering, ConditionError> {
    let (a, b) = align(a, b).ok_or(ConditionError::Overflow)?;
    Ok(a.mantissa().cmp(&b.mantissa()))
}

fn truth(value: bool) -> DecimalValue {
    if value { one() } else { zero() }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Gate {
    NonZero,
    Always,
    Lt { value: DecimalValue },
    Le { value: DecimalValue },
    Gt { value: DecimalValue },
    Ge { value: DecimalValue },
    Eq { value: DecimalValue },
}

impl Gate {
    pub fn parse(text: &str) -> Result<Gate, ConditionError> {
        let text = text.trim();
        if text == "always" {
            return Ok(Gate::Always);
        }
        if text == "!=0" {
            return Ok(Gate::NonZero);
        }
        let (build, rest): (fn(DecimalValue) -> Gate, &str) =
            if let Some(r) = text.strip_prefix("<=") {
                ((|v| Gate::Le { value: v }), r)
            } else if let Some(r) = text.strip_prefix(">=") {
                ((|v| Gate::Ge { value: v }), r)
            } else if let Some(r) = text.strip_prefix("==") {
                ((|v| Gate::Eq { value: v }), r)
            } else if let Some(r) = text.strip_prefix('<') {
                ((|v| Gate::Lt { value: v }), r)
            } else if let Some(r) = text.strip_prefix('>') {
                ((|v| Gate::Gt { value: v }), r)
            } else {
                return Err(ConditionError::Parse(format!("`{text}` is not a gate")));
            };
        let value = DecimalValue::parse_inferred(rest.trim())
            .map_err(|_| ConditionError::Parse(format!("`{text}` has no number")))?;
        Ok(build(value))
    }

    pub fn passes(&self, computed: DecimalValue) -> Result<bool, ConditionError> {
        use std::cmp::Ordering::*;
        Ok(match self {
            Gate::NonZero => computed.mantissa() != 0,
            Gate::Always => true,
            Gate::Lt { value } => compare(computed, *value)? == Less,
            Gate::Le { value } => matches!(compare(computed, *value)?, Less | Equal),
            Gate::Gt { value } => compare(computed, *value)? == Greater,
            Gate::Ge { value } => matches!(compare(computed, *value)?, Greater | Equal),
            Gate::Eq { value } => compare(computed, *value)? == Equal,
        })
    }

    pub fn as_text(&self) -> String {
        match self {
            Gate::NonZero => "!=0".into(),
            Gate::Always => "always".into(),
            Gate::Lt { value } => format!("<{value}"),
            Gate::Le { value } => format!("<={value}"),
            Gate::Gt { value } => format!(">{value}"),
            Gate::Ge { value } => format!(">={value}"),
            Gate::Eq { value } => format!("=={value}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Carry {
    Value,
    One,
    Const { value: DecimalValue },
}

impl Carry {
    pub fn parse(text: &str) -> Result<Carry, ConditionError> {
        let text = text.trim();
        match text {
            "value" => Ok(Carry::Value),
            "one" => Ok(Carry::One),
            _ => text
                .strip_prefix("const:")
                .and_then(|n| DecimalValue::parse_inferred(n.trim()).ok())
                .map(|value| Carry::Const { value })
                .ok_or_else(|| ConditionError::Parse(format!("`{text}` is not a carry"))),
        }
    }

    pub fn apply(&self, computed: DecimalValue) -> DecimalValue {
        match self {
            Carry::Value => computed,
            Carry::One => one(),
            Carry::Const { value } => *value,
        }
    }

    pub fn as_text(&self) -> String {
        match self {
            Carry::Value => "value".into(),
            Carry::One => "one".into(),
            Carry::Const { value } => format!("const:{value}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Condition {
    source: String,
    expr: Expr,
}

impl Condition {
    pub fn parse(source: &str) -> Result<Condition, ConditionError> {
        let expr = Expr::parse(source).map_err(|error| ConditionError::Parse(error.to_string()))?;
        let readings = expr.tokens();
        if readings.is_empty() {
            return Err(ConditionError::Parse(
                "this reads nothing, so nothing could ever make it true".into(),
            ));
        }
        if readings.iter().filter(|token| token.func == "freq").count() > 1 {
            return Err(ConditionError::Parse(
                "a condition may name at most one frequency".into(),
            ));
        }
        Ok(Condition {
            source: source.to_string(),
            expr,
        })
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn reads(&self) -> Vec<crate::expr::TokenKey> {
        self.expr.tokens()
    }

    pub fn evaluate(
        &self,
        resolver: &mut dyn ExactResolver,
    ) -> Result<DecimalValue, ConditionError> {
        evaluate(&self.expr, resolver)
    }
}

fn evaluate(expr: &Expr, resolver: &mut dyn ExactResolver) -> Result<DecimalValue, ConditionError> {
    match expr {
        Expr::Num(text) => DecimalValue::parse_inferred(text)
            .map_err(|_| ConditionError::Parse(format!("`{text}` is not a number"))),
        Expr::Dur(seconds) => {
            DecimalValue::from_mantissa(0, *seconds as i128).map_err(|_| ConditionError::Overflow)
        }
        Expr::Ref(slug) => resolver.lookup("quantity", slug, None),
        Expr::Fn(name, args) => call(name, args, resolver),
        Expr::Unary(op, inner) => {
            let value = evaluate(inner, resolver)?;
            Ok(match op {
                UnOp::Neg => value.checked_neg().ok_or(ConditionError::Overflow)?,
                UnOp::Not => truth(value.mantissa() == 0),
            })
        }
        Expr::Bin(op, left, right) => {
            if matches!(op, BinOp::And | BinOp::Or) {
                let left = evaluate(left, resolver)?;
                let left_true = left.mantissa() != 0;
                return match (op, left_true) {
                    (BinOp::And, false) => Ok(zero()),
                    (BinOp::Or, true) => Ok(one()),
                    _ => Ok(truth(evaluate(right, resolver)?.mantissa() != 0)),
                };
            }
            let a = evaluate(left, resolver)?;
            let b = evaluate(right, resolver)?;
            use std::cmp::Ordering::*;
            Ok(match op {
                BinOp::Add => add(a, b)?,
                BinOp::Sub => sub(a, b)?,
                BinOp::Mul => mul(a, b)?,
                BinOp::Div => div(a, b)?,
                BinOp::Rem => {
                    let (a, b) = align(a, b).ok_or(ConditionError::Overflow)?;
                    if b.mantissa() == 0 {
                        return Err(ConditionError::DivisionByZero);
                    }
                    DecimalValue::from_mantissa(a.scale(), a.mantissa() % b.mantissa())
                        .map_err(|_| ConditionError::Overflow)?
                }
                BinOp::Lt => truth(compare(a, b)? == Less),
                BinOp::Le => truth(matches!(compare(a, b)?, Less | Equal)),
                BinOp::Gt => truth(compare(a, b)? == Greater),
                BinOp::Ge => truth(matches!(compare(a, b)?, Greater | Equal)),
                BinOp::Eq => truth(compare(a, b)? == Equal),
                BinOp::Ne => truth(compare(a, b)? != Equal),
                BinOp::And | BinOp::Or => unreachable!("handled above"),
            })
        }
    }
}

const WINDOWED: [&str; 3] = ["sum", "sum_pos", "sum_neg"];

fn call(
    name: &str,
    args: &[Expr],
    resolver: &mut dyn ExactResolver,
) -> Result<DecimalValue, ConditionError> {
    let slug = match args.first() {
        Some(Expr::Ref(slug)) => slug.clone(),
        _ => {
            return Err(ConditionError::Parse(format!(
                "{name}() needs a @reference as its first argument"
            )));
        }
    };
    let window = match args.get(1) {
        Some(Expr::Dur(seconds)) => Some(*seconds),
        None => None,
        _ => {
            return Err(ConditionError::Parse(format!(
                "{name}()'s second argument must be a period like 30d"
            )));
        }
    };
    if WINDOWED.contains(&name) && window.is_none() {
        return Err(ConditionError::MissingWindow(format!("{name}(@{slug})")));
    }
    resolver.lookup(name, &slug, window)
}

pub fn decide(
    condition: &Condition,
    gate: &Gate,
    carry: &Carry,
    resolver: &mut dyn ExactResolver,
) -> Result<Option<DecimalValue>, ConditionError> {
    let computed = condition.evaluate(resolver)?;
    if !gate.passes(computed)? {
        return Ok(None);
    }
    Ok(Some(carry.apply(computed)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Map(HashMap<String, DecimalValue>);

    impl Map {
        fn set(&mut self, func: &str, slug: &str, value: &str) {
            self.0.insert(
                format!("{func}:{slug}"),
                DecimalValue::parse_inferred(value).unwrap(),
            );
        }
    }

    impl ExactResolver for Map {
        fn lookup(
            &mut self,
            func: &str,
            slug: &str,
            _window: Option<i64>,
        ) -> Result<DecimalValue, ConditionError> {
            self.0
                .get(&format!("{func}:{slug}"))
                .copied()
                .ok_or_else(|| ConditionError::UnknownReference(slug.to_string()))
        }
    }

    fn eval(source: &str, map: &mut Map) -> DecimalValue {
        Condition::parse(source).unwrap().evaluate(map).unwrap()
    }

    #[test]
    fn a_tenth_plus_two_tenths_is_exactly_three_tenths() {
        let mut map = Map::default();
        map.set("quantity", "opening", "0.1");
        assert_eq!(eval("@opening + 0.2", &mut map).to_string(), "0.3");
    }

    #[test]
    fn two_place_arithmetic_stays_exact() {
        let mut map = Map::default();
        map.set("quantity", "flour.jar", "1000.10");
        assert_eq!(eval("@flour.jar - 0.20", &mut map).to_string(), "999.90");
        map.set("quantity", "cent", "0.01");
        assert_eq!(eval("@cent * 3", &mut map).to_string(), "0.03");
    }

    #[test]
    fn the_old_operators_survive_the_port() {
        let mut map = Map::default();
        map.set("freq", "daily-7am", "0");
        let condition = Condition::parse("-1 * freq(@daily-7am)").unwrap();
        assert_eq!(
            decide(&condition, &Gate::NonZero, &Carry::Value, &mut map).unwrap(),
            None,
            "a timer that did not fire must not fire the rule"
        );

        map.set("freq", "daily-7am", "1");
        let carried = decide(&condition, &Gate::NonZero, &Carry::Value, &mut map)
            .unwrap()
            .unwrap();
        assert_eq!(carried.to_string(), "-1");
    }

    #[test]
    fn a_gate_decides_whether_and_a_carry_decides_what() {
        let mut map = Map::default();
        map.set("quantity", "apples.stock", "8");
        let condition = Condition::parse("@apples.stock").unwrap();
        let gate = Gate::parse("<3").unwrap();
        assert_eq!(
            decide(&condition, &gate, &Carry::One, &mut map).unwrap(),
            None
        );

        map.set("quantity", "apples.stock", "2");
        let carried = decide(&condition, &gate, &Carry::One, &mut map)
            .unwrap()
            .unwrap();
        assert_eq!(carried.to_string(), "1", "carry=one decouples the payload");
        let carried = decide(&condition, &gate, &Carry::Value, &mut map)
            .unwrap()
            .unwrap();
        assert_eq!(carried.to_string(), "2", "carry=value keeps the number");
        let carried = decide(
            &condition,
            &gate,
            &Carry::parse("const:7.5").unwrap(),
            &mut map,
        )
        .unwrap()
        .unwrap();
        assert_eq!(carried.to_string(), "7.5");
    }

    #[test]
    fn comparisons_and_booleans_are_numbers() {
        let mut map = Map::default();
        map.set("quantity", "a", "5");
        map.set("quantity", "b", "2");
        assert_eq!(eval("@a > @b", &mut map).to_string(), "1");
        assert_eq!(eval("@a < @b", &mut map).to_string(), "0");
        assert_eq!(eval("(@a > @b) && (@b > 10)", &mut map).to_string(), "0");
        assert_eq!(eval("(@a > @b) || (@b > 10)", &mut map).to_string(), "1");
        assert_eq!(
            eval("(@b > 10) && @nothing.here", &mut map).to_string(),
            "0"
        );
    }

    #[test]
    fn division_rounds_once_and_says_so() {
        let mut map = Map::default();
        map.set("quantity", "one", "1");
        map.set("quantity", "two", "2");
        map.set("quantity", "ten", "10");
        assert_eq!(eval("@one / 3", &mut map).to_string(), "0.333333");
        assert_eq!(eval("@two / 3", &mut map).to_string(), "0.666667");
        assert_eq!(eval("@ten / 4", &mut map).to_string(), "2.500000");
    }

    #[test]
    fn dividing_by_zero_is_refused_rather_than_infinite() {
        let mut map = Map::default();
        map.set("quantity", "one", "1");
        assert_eq!(
            Condition::parse("@one / 0").unwrap().evaluate(&mut map),
            Err(ConditionError::DivisionByZero)
        );
    }

    #[test]
    fn a_window_is_required_where_it_is_meaningless_without_one() {
        let mut map = Map::default();
        map.set("sum", "spend", "10");
        assert!(matches!(
            Condition::parse("sum(@spend)").unwrap().evaluate(&mut map),
            Err(ConditionError::MissingWindow(_))
        ));
        assert_eq!(eval("sum(@spend, 30d)", &mut map).to_string(), "10");
    }

    #[test]
    fn gates_and_carries_round_trip_through_their_text() {
        for text in ["!=0", "always", "<3", "<=3", ">3", ">=3", "==3"] {
            assert_eq!(Gate::parse(text).unwrap().as_text(), text);
        }
        for text in ["value", "one", "const:7.5"] {
            assert_eq!(Carry::parse(text).unwrap().as_text(), text);
        }
        assert!(Gate::parse("~5").is_err());
        assert!(Carry::parse("whatever").is_err());
    }

    #[test]
    fn a_condition_that_reads_nothing_is_refused() {
        assert!(matches!(
            Condition::parse("1 + 1"),
            Err(ConditionError::Parse(_))
        ));
        assert!(matches!(
            Condition::parse("2 > 5"),
            Err(ConditionError::Parse(_))
        ));
        assert!(Condition::parse("@a > 5").is_ok());
    }

    #[test]
    fn a_second_frequency_is_refused_at_parse_time() {
        assert!(Condition::parse("-1 * freq(@payday)").is_ok());
        assert!(matches!(
            Condition::parse("freq(@payday) + freq(@rent)"),
            Err(ConditionError::Parse(_))
        ));
        assert!(
            matches!(
                Condition::parse("freq(@payday) + freq(@payday)"),
                Err(ConditionError::Parse(_))
            ),
            "the same frequency named twice is still a second timer"
        );
        assert!(Condition::parse("freq(@payday) * @apples.stock").is_ok());
    }

    #[test]
    fn a_condition_reports_what_it_reads() {
        let condition = Condition::parse("@a + freq(@b) * sum(@c, 30d)").unwrap();
        let reads: Vec<String> = condition.reads().into_iter().map(|t| t.slug).collect();
        assert!(reads.contains(&"a".to_string()));
        assert!(reads.contains(&"b".to_string()));
        assert!(reads.contains(&"c".to_string()));
    }
}
