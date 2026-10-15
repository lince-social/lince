use crate::grammar::grammar as ast;
use crate::{Diagnostic, diagnostic};

pub fn parse(source: &str) -> Result<ast::Expression, Diagnostic> {
    if source.len() > 65536 {
        return Err(diagnostic("Expression exceeds 64 KiB"));
    }
    let wrapped = format!("print({source});");
    let mut document = crate::parse_syntax(&wrapped)?;
    if document.declarations.len() != 1 {
        return Err(diagnostic("Expected one expression"));
    }
    let ast::Declaration::Statement(ast::Statement::Call(mut statement)) =
        document.declarations.remove(0)
    else {
        return Err(diagnostic("Expected one expression"));
    };
    if statement.call.name.0 != "print" || statement.call.arguments.len() != 1 {
        return Err(diagnostic("Expected one expression"));
    }
    Ok(statement.call.arguments.remove(0))
}

pub fn format(expression: &ast::Expression) -> String {
    use ast::ExpressionKind as E;
    match &expression.node.value {
        E::Number(value) => value.0.clone(),
        E::Duration(value) => value.0.clone(),
        E::Text(value) => quoted(&value.0),
        E::Body(value) => quoted(&value.0),
        E::Reference(value) => format!("@{}", value.0),
        E::Concept(value) => format!("#{}", value.0),
        E::True(_) => "true".into(),
        E::False(_) => "false".into(),
        E::Null(_) => "null".into(),
        E::Name(value) => value.0.clone(),
        E::Call(value) => call(value),
        E::List(value) => format!(
            "[{}]",
            value
                .values
                .iter()
                .map(format)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        E::Object(value) => format!(
            "{{{}}}",
            value
                .fields
                .iter()
                .map(|field| {
                    let key = match &field.key {
                        ast::ExpressionKey::Name(name) => name.0.clone(),
                        ast::ExpressionKey::Text(text) => quoted(&text.0),
                    };
                    format!("{key}: {}", format(&field.value))
                })
                .collect::<Vec<_>>()
                .join(", ")
        ),
        E::Parenthesized(_, value, _) => format(value),
        E::Unary(operator, value) => format!(
            "({}{})",
            match operator {
                ast::UnaryOperator::Negative(_) => "-",
                ast::UnaryOperator::Positive(_) => "+",
                ast::UnaryOperator::Not(_) => "!",
            },
            format(value)
        ),
        E::Product(left, operator, right) => binary(
            left,
            match operator {
                ast::ProductOperator::Multiply(_) => "*",
                ast::ProductOperator::Divide(_) => "/",
                ast::ProductOperator::Remainder(_) => "%",
            },
            right,
        ),
        E::Sum(left, operator, right) => binary(
            left,
            match operator {
                ast::SumOperator::Add(_) => "+",
                ast::SumOperator::Subtract(_) => "-",
            },
            right,
        ),
        E::Compare(left, operator, right) => binary(
            left,
            match operator {
                ast::CompareOperator::Less(_) => "<",
                ast::CompareOperator::LessEqual(_) => "<=",
                ast::CompareOperator::Greater(_) => ">",
                ast::CompareOperator::GreaterEqual(_) => ">=",
                ast::CompareOperator::Equal(_) => "==",
                ast::CompareOperator::NotEqual(_) => "!=",
            },
            right,
        ),
        E::And(left, _, right) => binary(left, "&&", right),
        E::Or(left, _, right) => binary(left, "||", right),
    }
}

fn binary(left: &ast::Expression, operator: &str, right: &ast::Expression) -> String {
    format!("({} {operator} {})", format(left), format(right))
}

fn quoted(value: &str) -> String {
    serde_json::to_string(value).expect("text serialization")
}

pub fn call(value: &ast::Call) -> String {
    format!(
        "{}({})",
        value.name.0,
        value
            .arguments
            .iter()
            .map(format)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

pub fn statement(value: &ast::Statement) -> String {
    statement_at(value, 0)
}

fn statement_at(value: &ast::Statement, depth: usize) -> String {
    let indentation = "  ".repeat(depth);
    match value {
        ast::Statement::Binding(value) => {
            format!(
                "{indentation}let {} = {};\n",
                value.name.0,
                format(&value.value)
            )
        }
        ast::Statement::Assignment(value) => {
            format!(
                "{indentation}{} = {};\n",
                value.name.0,
                format(&value.value)
            )
        }
        ast::Statement::Call(value) => format!("{indentation}{};\n", call(&value.call)),
        ast::Statement::Conditional(value) => {
            format!("{indentation}{}\n", conditional(value, depth))
        }
        ast::Statement::ForLoop(value) => format!(
            "{indentation}for {} in {} {}\n",
            value.name.0,
            format(&value.iterable),
            block(&value.body, depth)
        ),
    }
}

fn block(value: &ast::Block, depth: usize) -> String {
    let body = value
        .statements
        .iter()
        .map(|value| statement_at(value, depth + 1))
        .collect::<String>();
    format!("{{\n{body}{}}}", "  ".repeat(depth))
}

fn conditional(value: &ast::Conditional, depth: usize) -> String {
    let mut output = format!(
        "if {} {}",
        format(&value.condition),
        block(&value.then_block, depth)
    );
    if let Some(otherwise) = &value.otherwise {
        output.push_str(" else ");
        output.push_str(&match &otherwise.branch {
            ast::ElseBranch::Block(value) => block(value, depth),
            ast::ElseBranch::Conditional(value) => conditional(value, depth),
        });
    }
    output
}
