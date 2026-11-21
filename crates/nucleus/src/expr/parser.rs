use anicca::grammar::grammar::{self as ast, ExpressionKind as E};

use super::{ASSERTION, BinOp, Expr, UnOp};
use crate::error::NucleusError;

impl Expr {
    /// Parses one expression using the shared Lingua syntax.
    ///
    /// # Errors
    ///
    /// Returns a parse error for invalid syntax, unsupported values, or excessive nesting.
    pub fn parse(src: &str) -> Result<Expr, NucleusError> {
        let expression =
            anicca::expression::parse(src).map_err(|error| NucleusError::Parse(error.message))?;
        Self::from_lingua(expression, 0)
    }

    fn from_lingua(value: ast::Expression, depth: usize) -> Result<Self, NucleusError> {
        if depth > 128 {
            return Err(NucleusError::Parse("Expression nesting exceeds 128".into()));
        }
        let convert = |value| Self::from_lingua(value, depth + 1);
        let binary = |left: Box<ast::Expression>, operator, right: Box<ast::Expression>| {
            Ok(Self::Bin(
                operator,
                Box::new(convert(*left)?),
                Box::new(convert(*right)?),
            ))
        };
        match value.node.value {
            E::Number(value) => Ok(Self::Num(value.0)),
            E::Duration(value) => parse_duration(&value.0)
                .map(Self::Dur)
                .ok_or_else(|| NucleusError::Parse("Invalid duration".into())),
            E::Text(value) => Ok(Self::Text(value.0)),
            E::Body(value) => Ok(Self::Text(value.0)),
            E::Reference(value) => Ok(Self::Ref(value.0)),
            E::Concept(value) => Ok(Self::Fn(ASSERTION.into(), vec![Self::Ref(value.0)])),
            E::True(_) => Ok(Self::Num("1".into())),
            E::False(_) => Ok(Self::Num("0".into())),
            E::Call(value) => Ok(Self::Fn(
                value.name.0,
                value
                    .arguments
                    .into_iter()
                    .map(convert)
                    .collect::<Result<_, _>>()?,
            )),
            E::Parenthesized(_, value, _) | E::Unary(ast::UnaryOperator::Positive(_), value) => {
                convert(*value)
            }
            E::Unary(ast::UnaryOperator::Negative(_), value) => {
                Ok(Self::Unary(UnOp::Neg, Box::new(convert(*value)?)))
            }
            E::Unary(ast::UnaryOperator::Not(_), value) => {
                Ok(Self::Unary(UnOp::Not, Box::new(convert(*value)?)))
            }
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
}

/// Parses seconds, minutes, hours, or days into whole seconds.
///
/// Fractional seconds truncate toward zero. Values outside the integer range
/// saturate, and a NaN numeric prefix converts to zero.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    reason = "keeps the public duration parser's truncation and saturation"
)]
pub fn parse_duration(text: &str) -> Option<i64> {
    let text = text.trim();
    let (end, unit) = text.char_indices().next_back()?;
    let seconds = match unit {
        's' => 1.0,
        'm' => 60.0,
        'h' => 3600.0,
        'd' => 86400.0,
        _ => return None,
    };
    let count: f64 = text.get(..end)?.parse().ok()?;
    Some((count * seconds) as i64)
}
