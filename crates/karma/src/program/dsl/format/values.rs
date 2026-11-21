use std::fmt::{Display, from_fn};

use crate::{
    DecimalPrecision, DeclaredUnit, ExpressionAst, LiteralValue, ResolvedReference, Slug, TypedUid,
    ValueType,
};

use super::{enum_atom, json_string, write_joined};

pub(super) fn format_expression(expression: &ExpressionAst) -> impl Display + '_ {
    from_fn(move |out| match expression {
        ExpressionAst::Literal { value } => write!(out, "literal({})", format_literal(value)),
        ExpressionAst::Input { input } => write!(out, "input({})", input.as_str()),
        ExpressionAst::Unary { operator, value } => write!(
            out,
            "unary({}, {})",
            enum_atom(operator),
            format_expression(value)
        ),
        ExpressionAst::Binary {
            operator,
            left,
            right,
            precision,
        } => {
            write!(
                out,
                "binary({}, {}, {}",
                enum_atom(operator),
                format_expression(left),
                format_expression(right)
            )?;
            if let Some(precision) = precision {
                write!(out, ", {}", format_precision(precision))?;
            }
            out.write_str(")")
        }
        ExpressionAst::If {
            condition,
            then_value,
            else_value,
        } => write!(
            out,
            "if({}, {}, {})",
            format_expression(condition),
            format_expression(then_value),
            format_expression(else_value)
        ),
    })
}

pub(super) fn format_type(value_type: &ValueType) -> impl Display + '_ {
    from_fn(move |out| match value_type {
        ValueType::Bool => out.write_str("bool"),
        ValueType::I64 => out.write_str("i64"),
        ValueType::Decimal { scale } => write!(out, "decimal({scale})"),
        ValueType::Probability => out.write_str("prob"),
        ValueType::Confidence => out.write_str("conf"),
        ValueType::Text => out.write_str("text"),
        ValueType::Duration => out.write_str("duration"),
        ValueType::Timestamp => out.write_str("timestamp"),
        ValueType::Quantity { scale, unit } => {
            write!(out, "quantity({scale}, {})", format_uid(unit))
        }
        ValueType::Reference { target } => write!(out, "reference({})", enum_atom(target)),
        ValueType::List { item } => write!(out, "list({})", format_type(item)),
        ValueType::Set { item } => write!(out, "set({})", format_type(item)),
        ValueType::Map { key, value } => {
            write!(out, "map({}, {})", format_type(key), format_type(value))
        }
        ValueType::Datum { value } => write!(out, "datum({})", format_type(value)),
        ValueType::Estimate { value } => write!(out, "estimate({})", format_type(value)),
        ValueType::Candidate {
            route,
            template,
            fields,
        } => {
            write!(
                out,
                "candidate({}, {}, {{",
                enum_atom(route),
                template.as_str()
            )?;
            write_joined(
                out,
                fields.iter().map(|(field, value)| {
                    from_fn(move |out| write!(out, "{}: {}", field.as_str(), format_type(value)))
                }),
            )?;
            out.write_str("})")
        }
    })
}

pub(super) fn format_literal(literal: &LiteralValue) -> impl Display + '_ {
    from_fn(move |out| match literal {
        LiteralValue::Bool { value } => write!(out, "bool({value})"),
        LiteralValue::I64 { value } => write!(out, "i64({value})"),
        LiteralValue::Decimal { value } => write!(
            out,
            "decimal({}, {})",
            value.scale(),
            json_string(&value.to_string())
        ),
        LiteralValue::Probability { value } => {
            write!(out, "prob({})", json_string(&value.to_string()))
        }
        LiteralValue::Confidence { value } => {
            write!(out, "conf({})", json_string(&value.to_string()))
        }
        LiteralValue::Text { value } => write!(out, "text({})", json_string(value)),
        LiteralValue::Duration { value } => write!(out, "duration({})", value.get()),
        LiteralValue::Timestamp { value } => {
            write!(out, "timestamp({})", json_string(&value.to_string()))
        }
        LiteralValue::Quantity { amount, unit } => write!(
            out,
            "quantity({}, {}, {})",
            amount.scale(),
            json_string(&amount.to_string()),
            format_uid(unit)
        ),
        LiteralValue::Reference { value } => write!(out, "reference({})", format_reference(value)),
        LiteralValue::Datum {
            value_type,
            state,
            value,
        } => {
            write!(
                out,
                "datum({}, {}",
                format_type(value_type),
                enum_atom(state)
            )?;
            if let Some(value) = value {
                write!(out, ", {}", format_literal(value))?;
            }
            out.write_str(")")
        }
        LiteralValue::Candidate {
            route,
            template,
            fields,
        } => {
            write!(
                out,
                "candidate({}, {}, {{",
                enum_atom(route),
                template.as_str()
            )?;
            write_joined(
                out,
                fields.iter().map(|(field, value)| {
                    from_fn(move |out| {
                        write!(out, "{} = {}", field.as_str(), format_literal(value))
                    })
                }),
            )?;
            out.write_str("})")
        }
    })
}

fn format_uid(uid: &TypedUid) -> impl Display + '_ {
    from_fn(move |out| write!(out, "uid({}, {})", enum_atom(&uid.kind()), uid.as_str()))
}

fn format_precision(precision: &DecimalPrecision) -> impl Display + '_ {
    from_fn(move |out| {
        write!(
            out,
            "precision({}, {}",
            precision.scale,
            enum_atom(&precision.rounding)
        )?;
        match &precision.result_unit {
            Some(DeclaredUnit::Dimensionless) => out.write_str(", dimensionless")?,
            Some(DeclaredUnit::Unit { unit }) => write!(out, ", {}", format_uid(unit))?,
            None => {}
        }
        out.write_str(")")
    })
}

pub(super) fn format_reference(reference: &ResolvedReference) -> impl Display + '_ {
    from_fn(move |out| {
        write!(
            out,
            "ref({}, {}, {})",
            enum_atom(&reference.target.kind()),
            reference.target.as_str(),
            reference.display_slug.as_ref().map_or("none", Slug::as_str)
        )
    })
}

pub(super) fn format_optional_reference(
    reference: Option<&ResolvedReference>,
) -> impl Display + '_ {
    from_fn(move |out| match reference {
        Some(reference) => write!(out, "{}", format_reference(reference)),
        None => out.write_str("none"),
    })
}
