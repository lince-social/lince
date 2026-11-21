//! Exact condition evaluation and consequence admission.

use std::{fmt, sync::Arc};

use serde::{Deserialize, Serialize};

use crate::{
    expr::{
        BinOp, Expr, HAS_ASSERTION, TokenKey, UnOp, assertion_binding, assertion_token,
        extension_token,
    },
    karma::DecimalValue,
    performance::{self, Span},
    transfer::karma::{is_agreement_reading, is_reading},
};

mod numeric;
pub(crate) use numeric::arithmetic;
use numeric::{compare, remainder};

#[cfg(test)]
mod tests;

const ZERO: DecimalValue = DecimalValue::from_integer(0);
const ONE: DecimalValue = DecimalValue::from_integer(1);

/// Failure to parse or evaluate a condition or apply its gate.
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

/// Supplies exact values for condition readings.
pub trait ExactResolver {
    /// Reads a reference with an optional window in seconds.
    ///
    /// # Errors
    ///
    /// Returns an error if the reading is unavailable or cannot be represented.
    fn lookup(
        &mut self,
        func: &str,
        slug: &str,
        window_secs: Option<i64>,
    ) -> Result<DecimalValue, ConditionError>;
}

fn truth(value: bool) -> DecimalValue {
    if value { ONE } else { ZERO }
}

/// Determines whether a computed value admits a consequence.
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
    /// Parses a named gate or threshold comparison.
    ///
    /// # Errors
    ///
    /// Returns an error if the gate or threshold is invalid.
    pub fn parse(text: &str) -> Result<Gate, ConditionError> {
        let text = text.trim();
        if text == "always" {
            return Ok(Gate::Always);
        }
        if text.strip_prefix("!=").is_some_and(|rest| {
            DecimalValue::parse_inferred(rest.trim()).is_ok_and(|value| value.mantissa() == 0)
        }) {
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

    /// Checks the computed value against this gate.
    ///
    /// # Errors
    ///
    /// Returns an error if aligning the decimal scales overflows.
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

    /// Formats the rule in editor syntax.
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

/// Selects the exact value carried to an admitted consequence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Carry {
    Value,
    One,
    Const { value: DecimalValue },
}

impl Carry {
    /// Parses a carry rule from the editor syntax.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown rule or invalid constant.
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

    /// Selects the exact value that will be carried.
    pub fn apply(&self, computed: DecimalValue) -> DecimalValue {
        match self {
            Carry::Value => computed,
            Carry::One => ONE,
            Carry::Const { value } => *value,
        }
    }

    /// Formats the rule in editor syntax.
    pub fn as_text(&self) -> String {
        match self {
            Carry::Value => "value".into(),
            Carry::One => "one".into(),
            Carry::Const { value } => format!("const:{value}"),
        }
    }
}

/// Validated condition expression and its prepared reading keys.
#[derive(Clone)]
pub struct Condition(Arc<Parsed>);

struct Parsed {
    source: Arc<str>,
    expr: Expr,
    readings: Vec<TokenKey>,
}

impl fmt::Debug for Condition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Condition")
            .field("source", &self.0.source)
            .field("expr", &self.0.expr)
            .finish()
    }
}

impl PartialEq for Condition {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
            || (self.0.source == other.0.source && self.0.expr == other.0.expr)
    }
}

impl Condition {
    /// Parses a condition and collects its reading keys in expression order.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid readings, no readings, or multiple frequencies.
    pub fn parse(source: &str) -> Result<Condition, ConditionError> {
        let _parse = Span::new("karma-condition-parse");
        performance::add("karma-condition-parse-attempts", 1);
        let expr = Expr::parse(source).map_err(|error| ConditionError::Parse(error.to_string()))?;
        fn validate(expr: &Expr) -> Result<(), ConditionError> {
            match expr {
                Expr::Fn(name, args) if name == HAS_ASSERTION => {
                    assertion_token(args)
                        .map_err(|error| ConditionError::Parse(error.to_string()))?;
                }
                Expr::Fn(name, args) if name == "extension" => {
                    extension_token(args)
                        .map_err(|error| ConditionError::Parse(error.to_string()))?;
                }
                Expr::Fn(name, args)
                    if is_reading(name)
                        || name == "distance"
                        || name == "query_command"
                        || name == "startup" =>
                {
                    let count = if is_agreement_reading(name) || name == "distance" {
                        2
                    } else {
                        1
                    };
                    if args.len() != count
                        || args
                            .iter()
                            .any(|argument| !matches!(argument, Expr::Ref(_)))
                    {
                        return Err(ConditionError::Parse(format!(
                            "{name}() needs exactly {count} @references"
                        )));
                    }
                }
                Expr::Fn(_, args) => {
                    for argument in args {
                        validate(argument)?;
                    }
                }
                Expr::Unary(_, value) => validate(value)?,
                Expr::Bin(_, left, right) => {
                    validate(left)?;
                    validate(right)?;
                }
                Expr::Text(_) => {
                    return Err(ConditionError::Parse(
                        "Text arguments belong inside an extension reading".into(),
                    ));
                }
                _ => {}
            }
            Ok(())
        }
        validate(&expr)?;
        let readings = expr.tokens();
        performance::add("karma-condition-readings-prepared", readings.len() as u64);
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
        Ok(Condition(Arc::new(Parsed {
            source: source.into(),
            expr,
            readings,
        })))
    }

    /// Returns the written expression, including after reference binding.
    pub fn source(&self) -> &str {
        &self.0.source
    }

    /// Copies the prepared reading keys for callers that need to own them.
    pub fn reads(&self) -> Vec<TokenKey> {
        self.0.readings.clone()
    }

    /// Borrows the keys prepared from the current expression.
    pub fn reads_ref(&self) -> &[TokenKey] {
        &self.0.readings
    }

    /// Binds references in a new expression while keeping the written source.
    ///
    /// # Errors
    ///
    /// Returns the error from a failed binding.
    pub fn map_references(
        &self,
        mut resolve: impl FnMut(&str, &str) -> Result<String, ConditionError>,
    ) -> Result<Self, ConditionError> {
        let _bind = Span::new("karma-condition-bind");
        performance::add("karma-condition-bind-attempts", 1);
        fn visit(
            expr: &mut Expr,
            reading: &str,
            resolve: &mut impl FnMut(&str, &str) -> Result<String, ConditionError>,
        ) -> Result<(), ConditionError> {
            match expr {
                Expr::Ref(name) => *name = resolve(reading, name)?,
                Expr::Fn(name, arguments) => {
                    if name == HAS_ASSERTION {
                        for (position, argument) in arguments.iter_mut().enumerate() {
                            let reading = assertion_binding(position);
                            let reference = match argument {
                                Expr::Fn(_, inner) => inner.first_mut().ok_or_else(|| {
                                    ConditionError::Parse("Choose an assertion predicate".into())
                                })?,
                                reference => reference,
                            };
                            visit(reference, reading, resolve)?;
                        }
                        return Ok(());
                    }
                    if name == "extension" {
                        // Parsing validates the extension address. Binding changes only its Record.
                        let [record @ Expr::Ref(_), Expr::Text(_), Expr::Text(_)] =
                            arguments.as_mut_slice()
                        else {
                            return Err(ConditionError::Parse(
                                r#"Use extension(@record, "namespace", "property")"#.into(),
                            ));
                        };
                        return visit(record, "extension", resolve);
                    }
                    for argument in arguments {
                        visit(argument, name, resolve)?;
                    }
                }
                Expr::Unary(_, value) => visit(value, "quantity", resolve)?,
                Expr::Bin(_, left, right) => {
                    visit(left, "quantity", resolve)?;
                    visit(right, "quantity", resolve)?;
                }
                Expr::Num(_) | Expr::Dur(_) | Expr::Text(_) => {}
            }
            Ok(())
        }
        let mut expr = self.0.expr.clone();
        visit(&mut expr, "quantity", &mut resolve)?;
        let readings = expr.tokens();
        performance::add("karma-condition-readings-prepared", readings.len() as u64);
        Ok(Self(Arc::new(Parsed {
            source: Arc::clone(&self.0.source),
            expr,
            readings,
        })))
    }

    /// Evaluates with exact decimals and short-circuit boolean operators.
    ///
    /// # Errors
    ///
    /// Returns an error for an unavailable reading, invalid argument, or unrepresentable result.
    pub fn evaluate(
        &self,
        resolver: &mut dyn ExactResolver,
    ) -> Result<DecimalValue, ConditionError> {
        evaluate(&self.0.expr, resolver)
    }
}

fn evaluate(expr: &Expr, resolver: &mut dyn ExactResolver) -> Result<DecimalValue, ConditionError> {
    match expr {
        Expr::Num(text) => DecimalValue::parse_inferred(text)
            .map_err(|_| ConditionError::Parse(format!("`{text}` is not a number"))),
        Expr::Dur(seconds) => Ok(DecimalValue::from_integer(i128::from(*seconds))),
        Expr::Ref(slug) => resolver.lookup("quantity", slug, None),
        Expr::Text(_) => Err(ConditionError::Parse(
            "Text cannot be used as a number".into(),
        )),
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
                    (BinOp::And, false) => Ok(ZERO),
                    (BinOp::Or, true) => Ok(ONE),
                    _ => Ok(truth(evaluate(right, resolver)?.mantissa() != 0)),
                };
            }
            let a = evaluate(left, resolver)?;
            let b = evaluate(right, resolver)?;
            use std::cmp::Ordering::*;
            Ok(match op {
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => arithmetic(*op, a, b)?,
                BinOp::Rem => remainder(a, b)?,
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
    if name == HAS_ASSERTION {
        let token =
            assertion_token(args).map_err(|error| ConditionError::Parse(error.to_string()))?;
        return resolver.lookup(&token.func, &token.slug, None);
    }
    if name == "extension" {
        let token =
            extension_token(args).map_err(|error| ConditionError::Parse(error.to_string()))?;
        return resolver.lookup(&token.func, &token.slug, None);
    }
    if is_agreement_reading(name) || name == "distance" {
        let [Expr::Ref(left), Expr::Ref(right)] = args else {
            return Err(ConditionError::Parse(format!(
                "{name}() needs two @references"
            )));
        };
        return resolver.lookup(name, &format!("{left}|{right}"), None);
    }
    let slug = match args.first() {
        Some(Expr::Ref(slug)) => slug,
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
    resolver.lookup(name, slug, window)
}

/// Evaluates a condition, checks its gate, and carries an admitted value.
///
/// # Errors
///
/// Returns any evaluation or gate comparison error.
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
