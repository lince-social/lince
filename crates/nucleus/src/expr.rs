use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::NucleusError;

mod parser;
mod tokens;

pub use parser::parse_duration;
pub use tokens::{
    ASSERTION, HAS_ASSERTION, assertion_binding, assertion_token, binding_reading, extension_parts,
    extension_reading, extension_token,
};
use tokens::{assertion_references, joined_references};

#[cfg(test)]
mod tests;

/// A parsed expression for numeric evaluation and Record reads.
#[expect(
    clippy::exhaustive_enums,
    reason = "exhaustive matches are part of the public API"
)]
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Number in its original source spelling.
    Num(String),
    /// Duration in seconds.
    Dur(i64),
    /// Text literal.
    Text(String),
    /// Record reference.
    Ref(String),
    /// A resolver call with its name and arguments.
    Fn(String, Vec<Expr>),
    /// A unary operation and operand.
    Unary(UnOp, Box<Expr>),
    /// A binary operation and its left and right operands.
    Bin(BinOp, Box<Expr>, Box<Expr>),
}

/// Arithmetic or logical operation on one operand.
#[expect(
    clippy::exhaustive_enums,
    reason = "exhaustive matches are part of the public API"
)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnOp {
    /// Arithmetic negation.
    Neg,
    /// Logical negation, treating zero as false.
    Not,
}

/// Arithmetic, comparison, or short-circuit logical operation on two operands.
#[expect(
    clippy::exhaustive_enums,
    reason = "exhaustive matches are part of the public API"
)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinOp {
    /// Addition.
    Add,
    /// Subtraction.
    Sub,
    /// Multiplication.
    Mul,
    /// Division.
    Div,
    /// Remainder.
    Rem,
    /// Less than.
    Lt,
    /// Less than or equal.
    Le,
    /// Greater than.
    Gt,
    /// Greater than or equal.
    Ge,
    /// Equal.
    Eq,
    /// Not equal.
    Ne,
    /// Logical AND, evaluating the right operand only when the left is nonzero.
    And,
    /// Logical OR, evaluating the right operand only when the left is zero.
    Or,
}

/// A value passed to or returned by a reading resolver.
#[expect(
    clippy::exhaustive_enums,
    reason = "exhaustive matches are part of the public API"
)]
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Floating-point number.
    Num(f64),
    /// Duration in seconds.
    Dur(i64),
    /// An unresolved Record reference.
    Ref(String),
    /// Text value.
    Text(String),
}

impl Value {
    /// Returns a number or rounds a duration in seconds to the nearest `f64`.
    ///
    /// # Errors
    ///
    /// Returns an evaluation error for a Record reference or text.
    #[expect(
        clippy::as_conversions,
        clippy::cast_precision_loss,
        reason = "duration seconds follow the evaluator's existing f64 rounding"
    )]
    pub fn as_num(&self) -> Result<f64, NucleusError> {
        match self {
            Self::Num(n) => Ok(*n),
            Self::Dur(s) => Ok(*s as f64),
            Self::Ref(r) => Err(NucleusError::Eval(format!(
                "reference @{r} used where a number was expected"
            ))),
            Self::Text(_) => Err(NucleusError::Eval(
                "text used where a number was expected".into(),
            )),
        }
    }
}

/// Supplies the value of a named reading during evaluation.
pub trait Resolver {
    /// Resolves a call with the provided argument values.
    ///
    /// # Errors
    ///
    /// Returns the resolver's error for an invalid or unavailable reading.
    fn call(&mut self, name: &str, args: &[Value]) -> Result<Value, NucleusError>;
}

/// Identifies a reading by function, reference arguments and optional duration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TokenKey {
    /// Function name or encoded extension reading.
    pub func: String,
    /// Reference arguments in call order, joined with `|`.
    pub slug: String,
    /// Duration argument in seconds, when supplied.
    pub dur_secs: Option<i64>,
}

impl Expr {
    /// Evaluates numerically against the given resolver.
    ///
    /// # Errors
    ///
    /// Returns a resolver error or an evaluation error for an incompatible value.
    pub fn eval(&self, r: &mut dyn Resolver) -> Result<f64, NucleusError> {
        self.eval_value(r)?.as_num()
    }

    fn eval_value(&self, r: &mut dyn Resolver) -> Result<Value, NucleusError> {
        Ok(match self {
            Self::Num(text) => Value::Num(text.parse().unwrap_or(0.0)),
            Self::Dur(s) => Value::Dur(*s),
            Self::Text(text) => Value::Text(text.clone()),
            Self::Ref(slug) => r.call("quantity", &[Value::Ref(slug.clone())])?,
            Self::Fn(name, args) => {
                if name == HAS_ASSERTION {
                    // Directly constructed expressions can contain the token separator.
                    let arguments = assertion_references(args)?
                        .flat_map(|reference| reference.split('|'))
                        .map(|reference| Value::Ref(reference.into()))
                        .collect::<Vec<_>>();
                    return r.call(name, &arguments);
                }
                let values = args
                    .iter()
                    .map(|arg| match arg {
                        Self::Ref(slug) => Ok(Value::Ref(slug.clone())),
                        other => other.eval_value(r),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                r.call(name, &values)?
            }
            Self::Unary(op, e) => {
                let v = e.eval(r)?;
                Value::Num(match op {
                    UnOp::Neg => -v,
                    UnOp::Not => bool_num(v == 0.0),
                })
            }
            Self::Bin(op, a, b) => {
                let x = a.eval(r)?;
                if *op == BinOp::And && x == 0.0 {
                    return Ok(Value::Num(0.0));
                }
                if *op == BinOp::Or && x != 0.0 {
                    return Ok(Value::Num(1.0));
                }
                let y = b.eval(r)?;
                Value::Num(match op {
                    BinOp::Add => x + y,
                    BinOp::Sub => x - y,
                    BinOp::Mul => x * y,
                    BinOp::Div => x / y,
                    BinOp::Rem => x % y,
                    BinOp::Lt => bool_num(x < y),
                    BinOp::Le => bool_num(x <= y),
                    BinOp::Gt => bool_num(x > y),
                    BinOp::Ge => bool_num(x >= y),
                    BinOp::Eq => bool_num(x == y),
                    BinOp::Ne => bool_num(x != y),
                    BinOp::And | BinOp::Or => bool_num(y != 0.0),
                })
            }
        })
    }
}

fn bool_num(b: bool) -> f64 {
    if b { 1.0 } else { 0.0 }
}

/// Resolves readings from an in-memory token map.
#[derive(Debug, Default, Clone)]
pub struct MapResolver {
    /// Numbers keyed by reading token.
    pub values: HashMap<TokenKey, f64>,
}

impl MapResolver {
    /// Inserts or replaces a reading value.
    pub fn set(&mut self, func: &str, slug: &str, dur_secs: Option<i64>, value: f64) {
        self.values.insert(
            TokenKey {
                func: func.into(),
                slug: slug.into(),
                dur_secs,
            },
            value,
        );
    }
}

impl Resolver for MapResolver {
    fn call(&mut self, name: &str, args: &[Value]) -> Result<Value, NucleusError> {
        let reading = if name == "extension" {
            match args {
                [Value::Ref(_), Value::Text(namespace), Value::Text(property)] => {
                    extension_reading(namespace, property)
                }
                _ => return Err(NucleusError::Eval("Invalid extension reading".into())),
            }
        } else {
            name.into()
        };
        let references = args.iter().filter_map(|arg| match arg {
            Value::Ref(slug) => Some(slug.as_str()),
            _ => None,
        });
        let slug = joined_references(references).ok_or_else(|| {
            NucleusError::Eval(format!("function {name} needs a @reference argument"))
        })?;
        let dur = args.iter().find_map(|a| match a {
            Value::Dur(s) => Some(*s),
            _ => None,
        });
        let key = TokenKey {
            func: reading,
            slug,
            dur_secs: dur,
        };
        self.values
            .get(&key)
            .copied()
            .map(Value::Num)
            .ok_or_else(|| NucleusError::UnknownToken(format!("{name}(@{})", key.slug)))
    }
}
