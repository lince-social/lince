use crate::error::NucleusError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Num(String),
    Dur(i64),
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
}

impl Value {
    pub fn as_num(&self) -> Result<f64, NucleusError> {
        match self {
            Value::Num(n) => Ok(*n),
            Value::Dur(s) => Ok(*s as f64),
            Value::Ref(r) => Err(NucleusError::Eval(format!(
                "reference @{r} used where a number was expected"
            ))),
        }
    }
}

pub trait Resolver {
    fn call(&mut self, name: &str, args: &[Value]) -> Result<Value, NucleusError>;
}

pub const ASSERTION: &str = "assertion";

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TokenKey {
    pub func: String,
    pub slug: String,
    pub dur_secs: Option<i64>,
}

impl Expr {
    pub fn parse(src: &str) -> Result<Expr, NucleusError> {
        let tokens = lex(src)?;
        let mut p = Parser { tokens, pos: 0 };
        let e = p.parse_or()?;
        if p.pos != p.tokens.len() {
            return Err(NucleusError::Parse(format!(
                "unexpected trailing input at token {}",
                p.pos
            )));
        }
        Ok(e)
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
            Expr::Num(_) | Expr::Dur(_) => {}
        }
    }

    pub fn eval(&self, r: &mut dyn Resolver) -> Result<f64, NucleusError> {
        self.eval_value(r)?.as_num()
    }

    fn eval_value(&self, r: &mut dyn Resolver) -> Result<Value, NucleusError> {
        Ok(match self {
            Expr::Num(text) => Value::Num(text.parse().unwrap_or(0.0)),
            Expr::Dur(s) => Value::Dur(*s),
            Expr::Ref(slug) => r.call("quantity", &[Value::Ref(slug.clone())])?,
            Expr::Fn(name, args) => {
                let mut vals = Vec::with_capacity(args.len());
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
            func: name.into(),
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

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(String),
    Dur(i64),
    Ref(String),
    Assert(String),
    Ident(String),
    LParen,
    RParen,
    Comma,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Lt,
    Le,
    Gt,
    Ge,
    EqEq,
    Ne,
    AndAnd,
    OrOr,
    Bang,
}

fn lex(src: &str) -> Result<Vec<Tok>, NucleusError> {
    let mut out = Vec::new();
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' | '\t' | '\n' | '\r' => i += 1,
            '(' => {
                out.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                out.push(Tok::RParen);
                i += 1;
            }
            ',' => {
                out.push(Tok::Comma);
                i += 1;
            }
            '+' => {
                out.push(Tok::Plus);
                i += 1;
            }
            '-' => {
                out.push(Tok::Minus);
                i += 1;
            }
            '*' => {
                out.push(Tok::Star);
                i += 1;
            }
            '/' => {
                out.push(Tok::Slash);
                i += 1;
            }
            '%' => {
                out.push(Tok::Percent);
                i += 1;
            }
            '!' => {
                if chars.get(i + 1) == Some(&'=') {
                    out.push(Tok::Ne);
                    i += 2;
                } else {
                    out.push(Tok::Bang);
                    i += 1;
                }
            }
            '<' => {
                if chars.get(i + 1) == Some(&'=') {
                    out.push(Tok::Le);
                    i += 2;
                } else {
                    out.push(Tok::Lt);
                    i += 1;
                }
            }
            '>' => {
                if chars.get(i + 1) == Some(&'=') {
                    out.push(Tok::Ge);
                    i += 2;
                } else {
                    out.push(Tok::Gt);
                    i += 1;
                }
            }
            '=' => {
                if chars.get(i + 1) == Some(&'=') {
                    out.push(Tok::EqEq);
                    i += 2;
                } else {
                    return Err(NucleusError::Parse("single '=' (use '==')".into()));
                }
            }
            '&' => {
                if chars.get(i + 1) == Some(&'&') {
                    out.push(Tok::AndAnd);
                    i += 2;
                } else {
                    return Err(NucleusError::Parse("single '&' (use '&&')".into()));
                }
            }
            '|' => {
                if chars.get(i + 1) == Some(&'|') {
                    out.push(Tok::OrOr);
                    i += 2;
                } else {
                    return Err(NucleusError::Parse("single '|' (use '||')".into()));
                }
            }
            '@' => {
                let start = i + 1;
                let mut j = start;
                while j < chars.len()
                    && (chars[j].is_ascii_alphanumeric()
                        || chars[j] == '.'
                        || chars[j] == '-'
                        || chars[j] == '_'
                        || chars[j] == '/')
                {
                    j += 1;
                }
                if j == start {
                    return Err(NucleusError::Parse("empty @reference".into()));
                }
                out.push(Tok::Ref(chars[start..j].iter().collect()));
                i = j;
            }
            '#' => {
                let start = i + 1;
                let mut j = start;
                while j < chars.len()
                    && (chars[j].is_ascii_alphanumeric()
                        || chars[j] == '.'
                        || chars[j] == '-'
                        || chars[j] == '_'
                        || chars[j] == '/')
                {
                    j += 1;
                }
                if j == start {
                    return Err(NucleusError::Parse("empty #assertion".into()));
                }
                out.push(Tok::Assert(chars[start..j].iter().collect()));
                i = j;
            }
            c if c.is_ascii_digit() => {
                let start = i;
                let mut j = i;
                while j < chars.len() && (chars[j].is_ascii_digit() || chars[j] == '.') {
                    j += 1;
                }
                let num_str: String = chars[start..j].iter().collect();
                let n: f64 = num_str
                    .parse()
                    .map_err(|_| NucleusError::Parse(format!("bad number `{num_str}`")))?;
                let suffix = chars.get(j).copied();
                let after_ok = chars
                    .get(j + 1)
                    .map(|c2| !c2.is_ascii_alphanumeric() && *c2 != '_')
                    .unwrap_or(true);
                let mult = match suffix {
                    Some('s') if after_ok => Some(1),
                    Some('m') if after_ok => Some(60),
                    Some('h') if after_ok => Some(3600),
                    Some('d') if after_ok => Some(86400),
                    _ => None,
                };
                if let Some(mult) = mult {
                    out.push(Tok::Dur((n * mult as f64) as i64));
                    i = j + 1;
                } else {
                    out.push(Tok::Num(num_str));
                    i = j;
                }
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let start = i;
                let mut j = i;
                while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                    j += 1;
                }
                out.push(Tok::Ident(chars[start..j].iter().collect()));
                i = j;
            }
            other => return Err(NucleusError::Parse(format!("unexpected char `{other}`"))),
        }
    }
    Ok(out)
}

struct Parser {
    tokens: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.pos)
    }

    fn bump(&mut self) -> Option<Tok> {
        let t = self.tokens.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn expect(&mut self, t: Tok) -> Result<(), NucleusError> {
        if self.peek() == Some(&t) {
            self.pos += 1;
            Ok(())
        } else {
            Err(NucleusError::Parse(format!(
                "expected {t:?}, found {:?}",
                self.peek()
            )))
        }
    }

    fn parse_or(&mut self) -> Result<Expr, NucleusError> {
        let mut e = self.parse_and()?;
        while self.peek() == Some(&Tok::OrOr) {
            self.bump();
            e = Expr::Bin(BinOp::Or, Box::new(e), Box::new(self.parse_and()?));
        }
        Ok(e)
    }

    fn parse_and(&mut self) -> Result<Expr, NucleusError> {
        let mut e = self.parse_cmp()?;
        while self.peek() == Some(&Tok::AndAnd) {
            self.bump();
            e = Expr::Bin(BinOp::And, Box::new(e), Box::new(self.parse_cmp()?));
        }
        Ok(e)
    }

    fn parse_cmp(&mut self) -> Result<Expr, NucleusError> {
        let mut e = self.parse_add()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Lt) => BinOp::Lt,
                Some(Tok::Le) => BinOp::Le,
                Some(Tok::Gt) => BinOp::Gt,
                Some(Tok::Ge) => BinOp::Ge,
                Some(Tok::EqEq) => BinOp::Eq,
                Some(Tok::Ne) => BinOp::Ne,
                _ => break,
            };
            self.bump();
            e = Expr::Bin(op, Box::new(e), Box::new(self.parse_add()?));
        }
        Ok(e)
    }

    fn parse_add(&mut self) -> Result<Expr, NucleusError> {
        let mut e = self.parse_mul()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Plus) => BinOp::Add,
                Some(Tok::Minus) => BinOp::Sub,
                _ => break,
            };
            self.bump();
            e = Expr::Bin(op, Box::new(e), Box::new(self.parse_mul()?));
        }
        Ok(e)
    }

    fn parse_mul(&mut self) -> Result<Expr, NucleusError> {
        let mut e = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Star) => BinOp::Mul,
                Some(Tok::Slash) => BinOp::Div,
                Some(Tok::Percent) => BinOp::Rem,
                _ => break,
            };
            self.bump();
            e = Expr::Bin(op, Box::new(e), Box::new(self.parse_unary()?));
        }
        Ok(e)
    }

    fn parse_unary(&mut self) -> Result<Expr, NucleusError> {
        match self.peek() {
            Some(Tok::Minus) => {
                self.bump();
                Ok(Expr::Unary(UnOp::Neg, Box::new(self.parse_unary()?)))
            }
            Some(Tok::Bang) => {
                self.bump();
                Ok(Expr::Unary(UnOp::Not, Box::new(self.parse_unary()?)))
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, NucleusError> {
        match self.bump() {
            Some(Tok::Num(text)) => Ok(Expr::Num(text)),
            Some(Tok::Dur(s)) => Ok(Expr::Dur(s)),
            Some(Tok::Ref(r)) => Ok(Expr::Ref(r)),
            Some(Tok::Assert(name)) => Ok(Expr::Fn(ASSERTION.into(), vec![Expr::Ref(name)])),
            Some(Tok::LParen) => {
                let e = self.parse_or()?;
                self.expect(Tok::RParen)?;
                Ok(e)
            }
            Some(Tok::Ident(name)) => {
                self.expect(Tok::LParen)?;
                let mut args = Vec::new();
                if self.peek() != Some(&Tok::RParen) {
                    loop {
                        args.push(self.parse_or()?);
                        if self.peek() == Some(&Tok::Comma) {
                            self.bump();
                        } else {
                            break;
                        }
                    }
                }
                self.expect(Tok::RParen)?;
                Ok(Expr::Fn(name, args))
            }
            other => Err(NucleusError::Parse(format!("unexpected token {other:?}"))),
        }
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
