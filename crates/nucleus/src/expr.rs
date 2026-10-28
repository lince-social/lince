use crate::error::NucleusError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Num(String),
    Dur(i64),
    Text(String),
    Ref(String),
    Fn(String, Vec<Expr>),
    Unary(UnOp, Box<Expr>),
    Bin(BinOp, Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Num(f64),
    Dur(i64),
    Ref(String),
    Text(String),
}

impl Value {
    pub fn as_num(&self) -> Result<f64, NucleusError> {
        match self {
            Value::Num(n) => Ok(*n),
            Value::Dur(s) => Ok(*s as f64),
            Value::Ref(r) => Err(NucleusError::Eval(format!(
                "reference @{r} used where a number was expected"
            ))),
            Value::Text(_) => Err(NucleusError::Eval(
                "text used where a number was expected".into(),
            )),
        }
    }
}

pub trait Resolver {
    fn call(&mut self, name: &str, args: &[Value]) -> Result<Value, NucleusError>;
}

pub const ASSERTION: &str = "assertion";
pub const HAS_ASSERTION: &str = "has_assertion";

pub fn assertion_token(args: &[Expr]) -> Result<TokenKey, NucleusError> {
    if !(2..=3).contains(&args.len()) {
        return Err(NucleusError::Parse(
            "Use has_assertion(@record, #predicate, @object) or omit @object".into(),
        ));
    }
    let mut references = Vec::new();
    for (position, argument) in args.iter().enumerate() {
        let reference = match argument {
            Expr::Ref(reference) => reference,
            Expr::Fn(name, inner) if position == 1 && name == ASSERTION => match inner.as_slice() {
                [Expr::Ref(reference)] => reference,
                _ => return Err(NucleusError::Parse("Choose an assertion predicate".into())),
            },
            _ => {
                return Err(NucleusError::Parse(
                    "Use Record references and an assertion predicate".into(),
                ));
            }
        };
        references.push(reference.as_str());
    }
    Ok(TokenKey {
        func: HAS_ASSERTION.into(),
        slug: references.join("|"),
        dur_secs: None,
    })
}

pub fn assertion_binding(position: usize) -> &'static str {
    match position {
        0 => "has_assertion.subject",
        1 => "has_assertion.predicate",
        _ => "has_assertion.object",
    }
}

pub fn extension_reading(namespace: &str, property: &str) -> String {
    format!(
        "extension:{}",
        serde_json::to_string(&(namespace, property)).unwrap()
    )
}

pub fn extension_parts(reading: &str) -> Option<(String, String)> {
    serde_json::from_str(reading.strip_prefix("extension:")?).ok()
}

pub fn binding_reading(reading: &str) -> &str {
    if reading.starts_with("extension:") {
        "extension"
    } else {
        reading
    }
}

pub fn extension_token(args: &[Expr]) -> Result<TokenKey, NucleusError> {
    let [
        Expr::Ref(record),
        Expr::Text(namespace),
        Expr::Text(property),
    ] = args
    else {
        return Err(NucleusError::Parse(
            "Use extension(@record, \"namespace\", \"property\")".into(),
        ));
    };
    if [namespace, property].iter().any(|name| {
        name.is_empty()
            || name.trim() != name.as_str()
            || name.len() > 200
            || name.chars().any(char::is_control)
    }) || property.contains('.')
    {
        return Err(NucleusError::Parse(
            "Choose a valid extension namespace and property".into(),
        ));
    }
    Ok(TokenKey {
        func: extension_reading(namespace, property),
        slug: record.clone(),
        dur_secs: None,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TokenKey {
    pub func: String,
    pub slug: String,
    pub dur_secs: Option<i64>,
}

impl Expr {
    pub fn parse(src: &str) -> Result<Expr, NucleusError> {
        let expression =
            anicca::expression::parse(src).map_err(|error| NucleusError::Parse(error.message))?;
        Self::from_lingua(&expression, 0)
    }

    fn from_lingua(
        value: &anicca::grammar::grammar::Expression,
        depth: usize,
    ) -> Result<Self, NucleusError> {
        use anicca::grammar::grammar as ast;
        use ast::ExpressionKind as E;
        if depth > 128 {
            return Err(NucleusError::Parse("Expression nesting exceeds 128".into()));
        }
        let convert = |value: &ast::Expression| Self::from_lingua(value, depth + 1);
        let binary = |left: &ast::Expression, operator, right: &ast::Expression| {
            Ok(Self::Bin(
                operator,
                Box::new(convert(left)?),
                Box::new(convert(right)?),
            ))
        };
        match &value.node.value {
            E::Number(value) => Ok(Self::Num(value.0.clone())),
            E::Duration(value) => parse_duration(&value.0)
                .map(Self::Dur)
                .ok_or_else(|| NucleusError::Parse("Invalid duration".into())),
            E::Text(value) => Ok(Self::Text(value.0.clone())),
            E::Body(value) => Ok(Self::Text(value.0.clone())),
            E::Reference(value) => Ok(Self::Ref(value.0.clone())),
            E::Concept(value) => Ok(Self::Fn(ASSERTION.into(), vec![Self::Ref(value.0.clone())])),
            E::True(_) => Ok(Self::Num("1".into())),
            E::False(_) => Ok(Self::Num("0".into())),
            E::Call(value) => Ok(Self::Fn(
                value.name.0.clone(),
                value
                    .arguments
                    .iter()
                    .map(convert)
                    .collect::<Result<_, _>>()?,
            )),
            E::Parenthesized(_, value, _) => convert(value),
            E::Unary(ast::UnaryOperator::Positive(_), value) => convert(value),
            E::Unary(operator, value) => Ok(Self::Unary(
                match operator {
                    ast::UnaryOperator::Negative(_) => UnOp::Neg,
                    ast::UnaryOperator::Not(_) => UnOp::Not,
                    ast::UnaryOperator::Positive(_) => unreachable!(),
                },
                Box::new(convert(value)?),
            )),
            E::Sum(left, operator, right) => binary(
                left,
                match operator {
                    ast::SumOperator::Add(_) => BinOp::Add,
                    ast::SumOperator::Subtract(_) => BinOp::Sub,
                },
                right,
            ),
            E::Product(left, operator, right) => binary(
                left,
                match operator {
                    ast::ProductOperator::Multiply(_) => BinOp::Mul,
                    ast::ProductOperator::Divide(_) => BinOp::Div,
                    ast::ProductOperator::Remainder(_) => BinOp::Rem,
                },
                right,
            ),
            E::Compare(left, operator, right) => binary(
                left,
                match operator {
                    ast::CompareOperator::Less(_) => BinOp::Lt,
                    ast::CompareOperator::LessEqual(_) => BinOp::Le,
                    ast::CompareOperator::Greater(_) => BinOp::Gt,
                    ast::CompareOperator::GreaterEqual(_) => BinOp::Ge,
                    ast::CompareOperator::Equal(_) => BinOp::Eq,
                    ast::CompareOperator::NotEqual(_) => BinOp::Ne,
                },
                right,
            ),
            E::And(left, _, right) => binary(left, BinOp::And, right),
            E::Or(left, _, right) => binary(left, BinOp::Or, right),
            E::Name(_) | E::Null(_) | E::List(_) | E::Object(_) => Err(NucleusError::Parse(
                "This value is only supported in scripts".into(),
            )),
        }
    }

    pub fn tokens(&self) -> Vec<TokenKey> {
        let mut out = Vec::new();
        self.collect_tokens(&mut out);
        out
    }

    fn collect_tokens(&self, out: &mut Vec<TokenKey>) {
        match self {
            Expr::Ref(slug) => out.push(TokenKey {
                func: "quantity".into(),
                slug: slug.clone(),
                dur_secs: None,
            }),
            Expr::Fn(name, args) => {
                if name == HAS_ASSERTION {
                    if let Ok(token) = assertion_token(args) {
                        out.push(token);
                    }
                    return;
                }
                if name == "extension" {
                    if let Ok(token) = extension_token(args) {
                        out.push(token);
                    }
                    return;
                }
                let dur = args.iter().find_map(|a| match a {
                    Expr::Dur(s) => Some(*s),
                    _ => None,
                });
                let refs: Vec<&String> = args
                    .iter()
                    .filter_map(|a| match a {
                        Expr::Ref(slug) => Some(slug),
                        _ => None,
                    })
                    .collect();
                if !refs.is_empty() {
                    out.push(TokenKey {
                        func: name.clone(),
                        slug: refs
                            .iter()
                            .map(|s| s.as_str())
                            .collect::<Vec<_>>()
                            .join("|"),
                        dur_secs: dur,
                    });
                }
                for a in args {
                    if !matches!(a, Expr::Ref(_)) {
                        a.collect_tokens(out);
                    }
                }
            }
            Expr::Unary(_, e) => e.collect_tokens(out),
            Expr::Bin(_, a, b) => {
                a.collect_tokens(out);
                b.collect_tokens(out);
            }
            Expr::Num(_) | Expr::Dur(_) | Expr::Text(_) => {}
        }
    }

    pub fn eval(&self, r: &mut dyn Resolver) -> Result<f64, NucleusError> {
        self.eval_value(r)?.as_num()
    }

    fn eval_value(&self, r: &mut dyn Resolver) -> Result<Value, NucleusError> {
        Ok(match self {
            Expr::Num(text) => Value::Num(text.parse().unwrap_or(0.0)),
            Expr::Dur(s) => Value::Dur(*s),
            Expr::Text(text) => Value::Text(text.clone()),
            Expr::Ref(slug) => r.call("quantity", &[Value::Ref(slug.clone())])?,
            Expr::Fn(name, args) => {
                let mut vals = Vec::with_capacity(args.len());
                if name == HAS_ASSERTION {
                    let token = assertion_token(args)?;
                    let arguments = token
                        .slug
                        .split('|')
                        .map(|reference| Value::Ref(reference.into()))
                        .collect::<Vec<_>>();
                    return r.call(name, &arguments);
                }
                for a in args {
                    vals.push(match a {
                        Expr::Ref(slug) => Value::Ref(slug.clone()),
                        other => other.eval_value(r)?,
                    });
                }
                r.call(name, &vals)?
            }
            Expr::Unary(op, e) => {
                let v = e.eval(r)?;
                Value::Num(match op {
                    UnOp::Neg => -v,
                    UnOp::Not => bool_num(v == 0.0),
                })
            }
            Expr::Bin(op, a, b) => {
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
                    BinOp::And => bool_num(y != 0.0),
                    BinOp::Or => bool_num(y != 0.0),
                })
            }
        })
    }
}

fn bool_num(b: bool) -> f64 {
    if b { 1.0 } else { 0.0 }
}

#[derive(Debug, Default, Clone)]
pub struct MapResolver {
    pub values: HashMap<TokenKey, f64>,
}

impl MapResolver {
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
        let refs: Vec<&str> = args
            .iter()
            .filter_map(|a| match a {
                Value::Ref(s) => Some(s.as_str()),
                _ => None,
            })
            .collect();
        if refs.is_empty() {
            return Err(NucleusError::Eval(format!(
                "function {name} needs a @reference argument"
            )));
        }
        let slug = refs.join("|");
        let dur = args.iter().find_map(|a| match a {
            Value::Dur(s) => Some(*s),
            _ => None,
        });
        let key = TokenKey {
            func: reading,
            slug: slug.clone(),
            dur_secs: dur,
        };
        self.values
            .get(&key)
            .copied()
            .map(Value::Num)
            .ok_or(NucleusError::UnknownToken(format!("{name}(@{slug})")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(src: &str, setup: impl FnOnce(&mut MapResolver)) -> f64 {
        let e = Expr::parse(src).unwrap();
        let mut r = MapResolver::default();
        setup(&mut r);
        e.eval(&mut r).unwrap()
    }

    #[test]
    fn plain_math() {
        assert_eq!(eval("1 + 2 * 3", |_| {}), 7.0);
        assert_eq!(eval("-(2 + 3) * 2", |_| {}), -10.0);
        assert_eq!(eval("10 % 3", |_| {}), 1.0);
    }

    #[test]
    fn booleans_are_numbers_no_times_one_workaround() {
        assert_eq!(
            eval("(@apples.stock < 3) + 2", |r| r.set(
                "quantity",
                "apples.stock",
                None,
                2.0
            )),
            3.0
        );
        assert_eq!(eval("!0", |_| {}), 1.0);
        assert_eq!(eval("1 && 0 || 1", |_| {}), 1.0);
    }

    #[test]
    fn bare_ref_is_quantity_sugar() {
        assert_eq!(
            eval("@apples.stock + quantity(@apples.stock)", |r| {
                r.set("quantity", "apples.stock", None, 4.0)
            }),
            8.0
        );
    }

    #[test]
    fn full_math_composition_from_the_blueprint() {
        assert_eq!(
            eval("-1 * freq(@daily-7am)", |r| r.set(
                "freq",
                "daily-7am",
                None,
                1.0
            )),
            -1.0
        );
        assert_eq!(
            eval("(@checking - value(@rules.monthly-burn)) < 500", |r| {
                r.set("quantity", "checking", None, 900.0);
                r.set("value", "rules.monthly-burn", None, 600.0);
            }),
            1.0
        );
    }

    #[test]
    fn durations() {
        assert_eq!(
            eval("sum(@checking, 30d) / 30", |r| {
                r.set("sum", "checking", Some(30 * 86400), 900.0)
            }),
            30.0
        );
        assert_eq!(eval("90s + 1m", |_| {}), 150.0);
    }

    #[test]
    fn token_extraction() {
        let e = Expr::parse("freq(@weekly) * @books + sum(@reading.log, 7d)").unwrap();
        let t = e.tokens();
        assert!(t.contains(&TokenKey {
            func: "freq".into(),
            slug: "weekly".into(),
            dur_secs: None
        }));
        assert!(t.contains(&TokenKey {
            func: "quantity".into(),
            slug: "books".into(),
            dur_secs: None
        }));
        assert!(t.contains(&TokenKey {
            func: "sum".into(),
            slug: "reading.log".into(),
            dur_secs: Some(7 * 86400)
        }));
    }

    #[test]
    fn unknown_token_errors() {
        let e = Expr::parse("@ghost").unwrap();
        let mut r = MapResolver::default();
        assert!(matches!(e.eval(&mut r), Err(NucleusError::UnknownToken(_))));
    }
}

pub fn parse_duration(text: &str) -> Option<i64> {
    let text = text.trim();
    let (number, unit) = text.split_at(text.len().checked_sub(1)?);
    let count: f64 = number.parse().ok()?;
    let seconds = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "d" => 86400,
        _ => return None,
    };
    Some((count * seconds as f64) as i64)
}

#[cfg(test)]
mod duration_tests {
    use super::parse_duration;

    #[test]
    fn the_four_units_read_as_seconds() {
        assert_eq!(parse_duration("90s"), Some(90));
        assert_eq!(parse_duration("5m"), Some(300));
        assert_eq!(parse_duration("2h"), Some(7_200));
        assert_eq!(parse_duration("30d"), Some(2_592_000));
    }

    #[test]
    fn anything_that_is_not_a_duration_is_refused_rather_than_guessed() {
        assert_eq!(parse_duration(""), None);
        assert_eq!(parse_duration("30"), None);
        assert_eq!(parse_duration("30w"), None, "weeks are not in this grammar");
        assert_eq!(parse_duration("d"), None);
    }
}
